use og_core::checker::Disambiguator as _;
use og_core::checker::Tagger as _;
use og_core::match_filters::LanguageMatchFilter as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub(crate) struct SrPipeline {
    tokenizer: og_tokenizer::DefaultWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn sr_resource_dir() -> std::path::PathBuf {
    super::sr_resource_dir().expect("sr resource dir")
}

fn build_pipeline(variant: super::SerbianVariant) -> SrPipeline {
    let resource_dir = sr_resource_dir();
    SrPipeline {
        tokenizer: og_tokenizer::DefaultWordTokenizer::new_generic(),
        tagger: super::serbian_tagger(variant).expect("serbian.dict tagger"),
        disambiguator: super::build_sr_disambiguator(&resource_dir),
        splitter: og_tokenizer::srx::srx_splitter("sr"),
    }
}

pub(crate) fn sr_pipeline() -> std::sync::Arc<SrPipeline> {
    static PIPE: og_core::releasable::Releasable<SrPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(build_pipeline(
            super::SerbianVariant::Ekavian,
        )))
    })
    .expect("sr pipeline")
}

fn jekavian_pipeline() -> std::sync::Arc<SrPipeline> {
    static PIPE: og_core::releasable::Releasable<SrPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(build_pipeline(
            super::SerbianVariant::Jekavian,
        )))
    })
    .expect("sr jekavian pipeline")
}

impl SrPipeline {
    fn sentence_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        match &self.splitter {
            Some(srx) => srx
                .split(text)
                .iter()
                .map(|s| (s.start(), s.end()))
                .collect(),
            None => vec![(0, text.len())],
        }
    }

    pub(crate) fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, start, end);
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(self.tokenizer.tokenize(sentence_text).into_iter().map(|t| {
            AnalyzedTokenReadings::new(AnalyzedToken::new(
                t.text(),
                start + t.start(),
                start + t.end(),
            ))
        }));
        sentence.set_tokens(tokens);
        self.tagger.tag(sentence.tokens_mut());
        {
            let tokens = sentence.tokens_mut();
            for token in tokens.iter_mut() {
                if token.readings().is_empty() && !token.is_whitespace() {
                    let primary = token.token().clone();
                    *token = AnalyzedTokenReadings::new(primary);
                }
            }
            for token in tokens.iter_mut().rev() {
                if !token.is_whitespace() && !token.has_pos_tag("SENT_START") {
                    if !token.has_pos_tag("SENT_END") {
                        token.add_sent_end();
                    }
                    break;
                }
            }
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }

    pub(crate) fn analyze_text(&self, text: &str) -> Vec<AnalyzedSentence> {
        self.sentence_ranges(text)
            .into_iter()
            .map(|(s, e)| self.analyze_range(text, s, e))
            .collect()
    }
}

fn compile_rule_file(file: &str) -> CompiledRuleSet {
    let path = crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join(format!(
            "sr/src/main/resources/org/languagetool/rules/sr/{file}"
        ));
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    XmlCompiler::new()
        .compile_file_with_base(&xml, path.parent())
        .unwrap_or_else(|e| panic!("compile sr {file}: {e}"))
}

fn compiled_rule_sets() -> Vec<std::sync::Arc<CompiledRuleSet>> {
    static SETS: og_core::releasable::Releasable<Vec<std::sync::Arc<CompiledRuleSet>>> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&SETS, || {
        let mut files = vec!["grammar.xml"];
        files.extend(super::SR_RULE_FILES);
        Some(std::sync::Arc::new(
            files
                .into_iter()
                .map(|file| std::sync::Arc::new(compile_rule_file(file)))
                .collect::<Vec<_>>(),
        ))
    })
    .expect("sr rule sets")
    .as_ref()
    .clone()
}

fn rule_groups(sets: &[std::sync::Arc<CompiledRuleSet>]) -> Vec<Vec<&CompiledRule>> {
    let mut group_index: HashMap<(String, Option<String>, String), usize> = HashMap::new();
    let mut groups: Vec<Vec<&CompiledRule>> = Vec::new();
    for rule_set in sets {
        let source = rule_set
            .rules
            .first()
            .and_then(|r| r.source_file.as_deref())
            .unwrap_or("")
            .to_string();
        for rule in &rule_set.rules {
            let key = (rule.id.clone(), rule.sub_id.clone(), source.clone());
            let next = groups.len();
            let idx = *group_index.entry(key).or_insert(next);
            if idx == next {
                groups.push(vec![rule]);
            } else {
                groups[idx].push(rule);
            }
        }
    }
    groups
}

fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

fn shared_pattern_engine() -> &'static PatternRuleEngine {
    static ENGINE: OnceLock<PatternRuleEngine> = OnceLock::new();
    ENGINE.get_or_init(PatternRuleEngine::new)
}

#[test]
#[ignore]
fn serbian_grammar_examples_with_sr_pipeline() {
    let _release_guard = crate::release_guard();
    let sets = compiled_rule_sets();
    let groups = rule_groups(&sets);
    let engine = shared_pattern_engine();
    let pipeline = sr_pipeline();

    let mut passed = 0usize;
    let mut correction_failures = Vec::new();
    let mut fail_lines: Vec<String> = Vec::new();
    let mut no_match = 0usize;
    let mut fp = 0usize;
    let mut marker = 0usize;
    let mut evaluated = 0usize;
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                for sentence in pipeline.analyze_text(&clean_text) {
                    matches.extend(engine.match_rule(aux, &sentence));
                }
            }
            evaluated += 1;
            let failed = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        no_match += 1;
                        Some(format!(
                            "FAIL: [{}]: Expected match on incorrect example but got none: '{}'",
                            rule.id, clean_text
                        ))
                    } else if marker_range.is_some_and(|(m_start, m_end)| {
                        let first = &matches[0];
                        let match_start = first.offset();
                        let match_end = match_start + first.length();
                        !(match_start <= m_start && match_end >= m_end)
                    }) {
                        marker += 1;
                        Some(format!(
                            "FAIL: [{}]: Match does not cover the marker: '{}'",
                            rule.id, clean_text
                        ))
                    } else {
                        None
                    }
                }
                og_xml::types::XmlExampleType::Correct => {
                    if !matches.is_empty() {
                        fp += 1;
                        Some(format!(
                            "FAIL: [{}]: Expected no match on correct example: '{}'",
                            rule.id, clean_text
                        ))
                    } else {
                        None
                    }
                }
                og_xml::types::XmlExampleType::TriggersError => None,
            };
            match failed {
                Some(line) => fail_lines.push(line),
                None => {
                    if let Some(expected) = example.corrections.first()
                        && !expected.is_empty()
                    {
                        let offered = matches[0]
                            .replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect::<Vec<_>>();
                        if !offered.iter().any(|r| r == expected) {
                            correction_failures.push(format!(
                                "CORRECTION: [{}]: '{}' expected '{expected}' got {offered:?}",
                                rule.id, clean_text
                            ));
                            continue;
                        }
                    }
                    passed += 1;
                }
            }
        }
    }
    let total = passed + fail_lines.len() + correction_failures.len();
    eprintln!(
        "sr module pipeline: {}/{} passed (no_match: {no_match}, fp: {fp}, marker: {marker}, \
         correction: {})",
        passed,
        total,
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_sr_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(evaluated, SR_EXAMPLE_COUNT);
    assert_eq!(
        fail_lines, SR_NO_MATCH_RESIDUE,
        "the no-match residue must stay pinned; these two INVALID_DATE examples die in the \
         sentence splitter, not in the rules: the sr SRX cascade (segment.srx Serbian ruleset) \
         has no break=no veto for a letter-month abbreviation followed by a year, so '29. феб. \
         1900' splits as ['Дана 29. феб. ', '1900.'] and the 5-token pattern never sees itself \
         whole; the same cascade runs in Java (loomchild SrxTextIterator, cascade=yes), whose \
         mixed-date exception only lists full month names (segment.srx:6985-6989), and the sr \
         module is excluded from the Java build (pom.xml:62 're-add when a maintainer is \
         found'), so no Java CI ground truth exists for these examples"
    );
    assert_eq!(
        correction_failures.len(),
        SR_CORRECTION_RESIDUE,
        "the correction residue must stay pinned; new failures mean an engine regression, \
         fewer mean a shared-engine gap family was fixed: {:#?}",
        &correction_failures[..correction_failures.len().min(10)]
    );
}

pub(crate) const SR_EXAMPLE_COUNT: usize = 49;
pub(crate) const SR_CORRECTION_RESIDUE: usize = 0;
pub(crate) const SR_NO_MATCH_RESIDUE: &[&str] = &[
    "FAIL: [INVALID_DATE]: Expected match on incorrect example but got none: 'То је било 32. \
     авг. 2014.'",
    "FAIL: [INVALID_DATE]: Expected match on incorrect example but got none: 'Дана 29. феб. \
     1900.'",
];

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let sets = compiled_rule_sets();
    let groups = rule_groups(&sets);
    let example_count: usize = groups
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, SR_EXAMPLE_COUNT,
        "the live example inventory across grammar.xml + the five grammar-*.xml files"
    );
    let ids: Vec<(String, Option<&str>)> = groups
        .iter()
        .filter_map(|g| g.first())
        .map(|r| (r.id.clone(), r.sub_id.as_deref()))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("YEAR_20001".to_string(), None),
            ("INVALID_DATE".to_string(), Some("1")),
            ("INVALID_DATE".to_string(), Some("2")),
            ("INVALID_DATE".to_string(), Some("3")),
            ("INVALID_DATE".to_string(), Some("4")),
            ("INVALID_DATE".to_string(), Some("5")),
            ("INVALID_DATE".to_string(), Some("6")),
            ("INVALID_DATE".to_string(), Some("7")),
            ("INVALID_DATE".to_string(), Some("8")),
            ("INVALID_DATE".to_string(), Some("9")),
            ("ZAPETA_TLD".to_string(), Some("1")),
            ("SR_ADJ_LOWER_LETTER_CASE".to_string(), Some("1")),
        ],
        "grammar-logical.xml contributes YEAR_20001 (its standalone rule lands before the \
         INVALID_DATE rulegroup's 9 subrules) + INVALID_DATE, grammar-punctuation.xml ZAPETA_TLD \
         and grammar-style.xml SR_ADJ_LOWER_LETTER_CASE (sub_id 1 inside its rulegroup); \
         grammar.xml holds only unification blocks and grammar-barbarism/spelling are empty"
    );
}

#[test]
fn srx_splitter_resolves_for_serbian() {
    let pipeline = sr_pipeline();
    use og_tokenizer::SentenceTokenizer as _;
    let splitter = pipeline
        .splitter
        .as_ref()
        .expect("the sr SRX cascade must resolve");
    let split = |text: &str| -> Vec<String> {
        splitter
            .split(text)
            .iter()
            .map(|s| s.text().to_string())
            .collect()
    };
    assert_eq!(
        split("Ово је једна реченица. И још једна."),
        vec!["Ово је једна реченица. ", "И још једна."]
    );
    assert_eq!(
        split("Једна реченица! Још једна."),
        vec!["Једна реченица! ", "Још једна."]
    );
    for single in [
        "Данас је 13.12.2004.",
        "Данас је 13. децембар.",
        "Видећемо се 29. фебруара.",
        "Петар I, познат и као Петар Ослободилац.",
        "Жена стиже тачно у 17:00 кући.",
        "Писмо је стигло 3.10.2000 поподне.",
        "Ово је једна(!) реченица.",
    ] {
        assert_eq!(
            split(single),
            vec![single.to_string()],
            "no mid-sentence break"
        );
    }
}

#[test]
fn ekavian_speller_pins_the_java_test_corpus() {
    let natives = super::serbian_native_rules(super::SerbianVariant::Ekavian);
    let speller = natives
        .rules
        .iter()
        .find(|r| r.id() == "MORFOLOGIK_RULE_SR_EKAVIAN")
        .expect("ekavian speller native");
    let pipeline = sr_pipeline();
    for correct in [
        "Тамо је леп цвет",
        "Дечак и девојчица играју се заједно.",
        ",",
        "III",
    ] {
        let sentence = pipeline.analyze_text(correct).pop().unwrap();
        assert!(
            speller.match_sentence(&sentence).is_empty(),
            "MorfologikEkavianSpellerRuleTest expects no flag on '{correct}'"
        );
    }
    let sentence = pipeline
        .analyze_text("Изгубила све сам бткие, ал' још водим рат.")
        .pop()
        .unwrap();
    let matches = speller.match_sentence(&sentence);
    assert_eq!(matches.len(), 1);
    assert!(
        matches[0].message().contains("бткие"),
        "the rule's own example pair word must be flagged: {}",
        matches[0].message()
    );
    assert!(
        super::sr_speller_rule(super::SerbianVariant::Ekavian)
            .get_suggestions("бткие", 1000)
            .contains(&"битке".to_string()),
        "битке (edit distance 2, the one-char shift Java's Morfologik speller ranks first) must \
         be a reachable candidate; the top-5 order among equal-distance candidates follows \
         HashSet order, the divergence the ml round documented"
    );
}

#[test]
fn jekavian_speller_pins_the_java_test_corpus() {
    let natives = super::serbian_native_rules(super::SerbianVariant::Jekavian);
    let speller = natives
        .rules
        .iter()
        .find(|r| r.id() == "MORFOLOGIK_RULE_SR_JEKAVIAN")
        .expect("jekavian speller native");
    let pipeline = jekavian_pipeline();
    for correct in [
        "Тамо је лијеп цвијет.",
        "Дјечак и дјевојчица играју се заједно.",
        "III",
    ] {
        let sentence = pipeline.analyze_text(correct).pop().unwrap();
        assert!(
            speller.match_sentence(&sentence).is_empty(),
            "MorfologikJekavianSpellerRuleTest expects no flag on '{correct}'"
        );
    }
    let sentence = pipeline.analyze_text("Misspelled.").pop().unwrap();
    assert_eq!(speller.match_sentence(&sentence).len(), 1);
}

pub(crate) fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = sr_pipeline();
    let natives = super::serbian_native_rules(super::SerbianVariant::Ekavian);
    let engine = shared_pattern_engine();
    let sentences = pipeline.analyze_text(text);
    let mut all_matches = Vec::new();
    for rule in &natives.rules {
        for sentence in &sentences {
            all_matches.extend(rule.match_sentence(sentence));
        }
    }
    let rule_sets = compiled_rule_sets();
    for rule_set in &rule_sets {
        for compiled in rule_set
            .rules
            .iter()
            .filter(|r| r.default_on && !r.deprecated)
        {
            for sentence in &sentences {
                all_matches.extend(engine.match_rule(compiled, sentence));
            }
        }
    }
    for rule in &natives.text_level_rules {
        all_matches.extend(rule.match_text(text, &sentences));
    }
    static BASE: og_core::match_filters::BaseLanguageMatchFilter =
        og_core::match_filters::BaseLanguageMatchFilter;
    og_core::match_filters::same_rule_group_filter(&mut all_matches);
    BASE.filter_rule_matches(&mut all_matches, text);
    og_core::match_filters::clean_overlapping_filter(all_matches, text, &BASE)
}

#[test]
fn module_check_runs_the_java_rule_families() {
    let text = "То је било 31.11. Син је вишљи од оца. „Наводник";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![
            ("INVALID_DATE", "31.11."),
            ("SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE", "вишљи"),
            ("MORFOLOGIK_RULE_SR_EKAVIAN", "оца"),
            ("UNPAIRED_BRACKETS", "„"),
        ],
        "the INVALID_DATE regex-of-pattern rule from grammar-logical.xml, the ekavian \
         grammar-replace native, the ekavian speller and the Serbian unpaired-bracket symbols \
         all fire through the module pipeline; the genitive 'оца' is genuinely absent from \
         serbian.dict (verified via load_fsa_word_list), so Java's Morfologik speller flags it \
         too, and on 'вишљи' the overlap filter keeps the replace rule over the speller"
    );
}
