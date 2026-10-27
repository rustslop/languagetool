use og_core::checker::Disambiguator as _;
use og_core::checker::Tagger as _;
use og_core::match_filters::LanguageMatchFilter as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct RoPipeline {
    tokenizer: super::RomanianWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ro_resource_dir() -> std::path::PathBuf {
    super::ro_resource_dir().expect("ro resource dir")
}

pub(crate) fn ro_pipeline() -> std::sync::Arc<RoPipeline> {
    static PIPE: og_core::releasable::Releasable<RoPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        let resource_dir = ro_resource_dir();
        Some(std::sync::Arc::new(RoPipeline {
            tokenizer: super::RomanianWordTokenizer::new(),
            tagger: super::romanian_tagger().expect("romanian.dict tagger"),
            disambiguator: super::build_ro_disambiguator(&resource_dir),
            splitter: og_tokenizer::srx::srx_splitter("ro"),
        }))
    })
    .expect("ro pipeline")
}

impl RoPipeline {
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

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("ro/src/main/resources/org/languagetool/rules/ro/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ro grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ro grammar.xml"),
        ))
    })
    .expect("ro grammar")
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

fn rule_groups(rule_set: &CompiledRuleSet) -> Vec<Vec<&CompiledRule>> {
    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&CompiledRule>> = Vec::new();
    for rule in &rule_set.rules {
        let key = (rule.id.clone(), rule.sub_id.clone());
        let next = groups.len();
        let idx = *group_index.entry(key).or_insert(next);
        if idx == next {
            groups.push(vec![rule]);
        } else {
            groups[idx].push(rule);
        }
    }
    groups
}

fn shared_pattern_engine() -> std::sync::Arc<PatternRuleEngine> {
    static ENGINE: og_core::releasable::Releasable<PatternRuleEngine> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&ENGINE, || {
        let resource_dir = ro_resource_dir();
        let synth =
            og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "ro")
                .map(Arc::new);
        let mut engine = PatternRuleEngine::new();
        if let Some(synth) = synth.clone() {
            engine = engine.with_synthesizer(synth.clone());
            engine = engine.with_suggestion_synthesizer(synth);
        }
        Some(std::sync::Arc::new(engine))
    })
    .expect("ro pattern engine")
}

#[test]
#[ignore]
fn romanian_grammar_examples_with_ro_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = shared_pattern_engine();
    let pipeline = ro_pipeline();
    og_rules::pattern_rule::set_test_date_hack(true);

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
        "ro module pipeline: {}/{} passed (no_match: {no_match}, fp: {fp}, marker: {marker}, \
         correction: {})",
        passed,
        total,
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_ro_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(evaluated, RO_EXAMPLE_COUNT);
    assert!(
        fail_lines.is_empty(),
        "every ro example must match at match/marker level with no false positives, got {}: {:#?}",
        fail_lines.len(),
        &fail_lines[..fail_lines.len().min(10)]
    );
    assert_eq!(
        correction_failures.len(),
        RO_CORRECTION_RESIDUE,
        "the correction residue must stay pinned; new failures mean an engine regression, \
         fewer mean one of the two shared-engine gap families was fixed: {:#?}",
        &correction_failures[..correction_failures.len().min(10)]
    );
    let paren_form: Vec<&String> = correction_failures
        .iter()
        .filter(|f| {
            f.split_once(" got ")
                .is_some_and(|(_, got)| got.contains('('))
        })
        .collect();
    assert_eq!(
        paren_form.len(),
        28,
        "28 of the corrections are the '(token)lemma' placeholder family: og-xml drops the \
         <match> element content (types.rs XmlMatch has no content field), so og-rules \
         pattern_rule.rs synthesizes the matched token's lemma and falls back to the \
         '(token_str)' placeholder at pattern_rule.rs:920/:935/:991 instead of Java's \
         synthesize(content_lemma, replaced_postag_of_token_no)"
    );
    let mut histogram: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for f in &correction_failures {
        let id = f
            .split("[")
            .nth(1)
            .and_then(|r| r.split("]").next())
            .unwrap_or("");
        *histogram.entry(id).or_insert(0) += 1;
    }
    assert_eq!(
        histogram,
        [
            ("A_ADAUGA_O_COMPLETARE", 3),
            ("ADAUGA_ADITIVI", 4),
            ("ADAUGA_UN_SPOR", 1),
            ("AL_OPTELEA", 1),
            ("A_PRILEJUIT_OCAZIA", 1),
            ("APOI_A_URMAT", 1),
            ("CERE_O_SOLICITARE", 2),
            ("COASOCIAT", 2),
            ("COPARTAS", 2),
            ("DAT_FIIND__FEM_PLURAL", 2),
            ("DAT_FIIND__MASC_PLURAL", 1),
            ("DESCHIDEREA_VERNISAJULUI", 2),
            ("ESTE_FRUMOSI", 1),
            ("EU_VENEAU", 1),
            ("EU_VENEATI", 1),
            ("EU_VENISERATI", 1),
            ("FEMEIE_XYZ", 2),
            ("ILUSTRAT_CU_EXEMPLE", 2),
            ("MAI_ITI_ESTE", 2),
            ("POATE_FI_UTILIZABIL", 3),
            ("PROCENT_LA_SUTA", 1),
            ("PROTAGONIST_PRINCIPAL", 13),
            ("SE_MERITA", 1),
            ("SFN_ESTE_A", 1),
            ("STARE_CIVILA", 2),
            ("UN_NUMAR_DE_X", 1),
            ("UN_SF_O_SF", 1),
            ("VASAZICA", 1),
        ]
        .into_iter()
        .collect(),
        "the exact per-rule correction residue fingerprint"
    );
}

pub(crate) const RO_EXAMPLE_COUNT: usize = 1154;
pub(crate) const RO_CORRECTION_RESIDUE: usize = 56;

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, RO_EXAMPLE_COUNT,
        "the live example inventory of ro grammar.xml"
    );
}

#[test]
fn tagger_pins_from_the_java_tagger_test() {
    let pipeline = ro_pipeline();
    let readings_for = |word: &str| -> Vec<(String, String)> {
        let text = format!("Cartea este {word}.");
        let sentences = pipeline.analyze_text(&text);
        sentences[0]
            .tokens()
            .iter()
            .find(|t| t.token().token() == word)
            .map(|t| {
                t.readings()
                    .iter()
                    .map(|r| (r.lemma().unwrap_or("").to_string(), r.pos_tags().join("+")))
                    .collect()
            })
            .unwrap_or_default()
    };
    let merseseram = readings_for("merseserăm");
    assert!(
        merseseram
            .iter()
            .any(|(lemma, pos)| lemma == "merge" && pos == "V0p1000im0"),
        "RomanianTaggerTest.testTaggerMerseseram pins merseserăm/[merge]V0p1000im0, got \
         {merseseram:?}"
    );
    let sunt = readings_for("sunt");
    assert!(
        sunt.iter()
            .any(|(lemma, pos)| lemma == "fi" && pos == "V0s1000izf"),
        "testTagger_Fi pins sunt/[fi]V0s1000izf, got {sunt:?}"
    );
    assert!(
        sunt.iter()
            .any(|(lemma, pos)| lemma == "fi" && pos == "V0p3000izf"),
        "testTagger_Fi pins sunt/[fi]V0p3000izf, got {sunt:?}"
    );
    let sentence = pipeline.analyze_text("Cartea este frumoasă.");
    let pins = [
        ("Cartea", "carte", "Sfs3aac000"),
        ("este", "fi", "V0s3000izb"),
        ("frumoasă", "frumos", "Afs3an0000"),
    ];
    for (word, lemma, pos) in pins {
        let found = sentence[0]
            .tokens()
            .iter()
            .find(|t| t.token().token() == word)
            .and_then(|t| {
                t.readings()
                    .iter()
                    .find(|r| r.lemma() == Some(lemma) && r.pos_tags().join("+") == pos)
            });
        assert!(
            found.is_some(),
            "RomanianTaggerTest.testTagger pins {word}/[{lemma}]{pos}"
        );
    }
}

#[test]
fn srx_splitter_resolves_for_romanian() {
    let pipeline = ro_pipeline();
    use og_tokenizer::SentenceTokenizer as _;
    let splitter = pipeline
        .splitter
        .as_ref()
        .expect("the ro SRX cascade must resolve");
    let split = |text: &str| -> Vec<String> {
        splitter
            .split(text)
            .iter()
            .map(|s| s.text().to_string())
            .collect()
    };
    assert_eq!(
        split("Cartea este frumoasă. Ion merge acolo."),
        vec!["Cartea este frumoasă. ", "Ion merge acolo."]
    );
}

#[test]
fn disambiguator_runs_over_the_module_pipeline() {
    let pipeline = ro_pipeline();
    let sentence = pipeline
        .analyze_text("persoana care face asta.")
        .pop()
        .unwrap();
    let readings = sentence
        .tokens()
        .iter()
        .find(|t| t.token().token() == "care")
        .map(|t| {
            t.readings()
                .iter()
                .map(|r| r.pos_tags().join("+"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(!readings.is_empty());
}

pub(crate) fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = ro_pipeline();
    let natives = super::romanian_native_rules();
    let engine = shared_pattern_engine();
    let sentences = pipeline.analyze_text(text);
    let mut all_matches = Vec::new();
    for rule in &natives.rules {
        for sentence in &sentences {
            all_matches.extend(rule.match_sentence(sentence));
        }
    }
    for compiled in compiled_rules()
        .rules
        .iter()
        .filter(|r| r.default_on && !r.deprecated)
    {
        for sentence in &sentences {
            all_matches.extend(engine.match_rule(compiled, sentence));
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
    let text = "Câte și trei au plecat „acasă. El are un număr de 20 de mere.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![
            ("RO_COMPOUND", "Câte și trei"),
            ("UNPAIRED_BRACKETS", "„"),
            ("UN_NUMAR_DE_X", "un număr de 20"),
        ],
        "the ro compound native, the ro unpaired-bracket symbol set and the UNKNOWN-postag \
         grammar rule all fire through the module pipeline"
    );
}
