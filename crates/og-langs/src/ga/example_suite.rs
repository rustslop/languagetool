use og_core::checker::Disambiguator as _;
use og_core::checker::{Chunker as _, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct GaPipeline {
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::ChainedDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ga_resource_dir() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("ga/src/main/resources/org/languagetool/resource/ga")
}

pub(crate) fn ga_pipeline() -> std::sync::Arc<GaPipeline> {
    static PIPE: og_core::releasable::Releasable<GaPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(GaPipeline {
            tagger: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                &ga_resource_dir(),
                "irish",
            )),
            disambiguator: super::build_ga_disambiguator(&ga_resource_dir()),
            splitter: og_tokenizer::srx::srx_splitter("ga"),
        }))
    })
    .expect("ga")
}

impl GaPipeline {
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
        tokens.extend(
            og_tokenizer::DefaultWordTokenizer::new_generic()
                .tokenize(sentence_text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(AnalyzedToken::new(
                        t.text(),
                        start + t.start(),
                        start + t.end(),
                    ))
                }),
        );
        sentence.set_tokens(tokens);
        Tagger::tag(
            self.tagger.as_ref() as &dyn og_core::checker::Tagger,
            sentence.tokens_mut(),
        );
        og_tagger::chunker::PostDisambiguationChunker.add_chunk_tags(&mut sentence);
        {
            let tokens = sentence.tokens_mut();
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

struct GaSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for GaSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("ga") {
            Some(srx) => srx
                .split(text)
                .into_iter()
                .map(|s| og_core::checker::SentenceRange {
                    start: s.start(),
                    end: s.end(),
                })
                .collect(),
            None => vec![og_core::checker::SentenceRange {
                start: 0,
                end: text.len(),
            }],
        }
    }
}

struct GaWordTokenizerBridge;

impl og_core::checker::WordTokenizer for GaWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        og_tokenizer::DefaultWordTokenizer::new_generic()
            .tokenize(text)
            .into_iter()
            .map(|t| {
                let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}

pub(crate) fn module_checker() -> std::sync::Arc<og_core::checker::Checker> {
    static CHECKER: og_core::releasable::Releasable<og_core::checker::Checker> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CHECKER, || {
        let resource_dir = ga_resource_dir();
        let tagger = Arc::new(og_tagger::GenericTagger::from_resource_dir(
            &resource_dir,
            "irish",
        ));
        let natives = super::irish_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
        ];
        rules.extend(natives.rules);
        for compiled in compiled_rules().rules.clone() {
            rules.push(Arc::new(crate::engine::XmlPatternRule::new(compiled)));
        }
        let mut text_level_rules: Vec<Arc<dyn og_core::rule::TextLevelRule>> = vec![
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
            Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
                    vec!["[", "(", "{", "\"", "\u{201C}"],
                    vec!["]", ")", "}", "\"", "\u{201D}"],
                ),
            ),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
            Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
        ];
        text_level_rules.extend(natives.text_level_rules);
        let mut checker = og_core::checker::Checker::new()
            .with_sentence_tokenizer(Arc::new(GaSentenceTokenizer))
            .with_word_tokenizer(Arc::new(GaWordTokenizerBridge))
            .with_tagger(tagger)
            .with_chunker(Arc::new(og_tagger::chunker::PostDisambiguationChunker))
            .set_rules(rules)
            .set_text_level_rules(text_level_rules);
        if let Some(disambiguator) = super::build_ga_disambiguator(&resource_dir) {
            checker = checker.with_disambiguator(Arc::new(disambiguator));
        }
        Some(std::sync::Arc::new(checker))
    })
    .expect("ga")
}

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("ga/src/main/resources/org/languagetool/rules/ga/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ga grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ga grammar.xml"),
        ))
    })
    .expect("ga")
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

fn matches_for_example(
    pipeline: &GaPipeline,
    engine: &PatternRuleEngine,
    group: &[&CompiledRule],
    clean_text: &str,
) -> Vec<og_core::RuleMatch> {
    let mut matches = Vec::new();
    for aux in group {
        for sentence in pipeline.analyze_text(clean_text) {
            matches.extend(engine.match_rule(aux, &sentence));
        }
    }
    matches
}

struct SuiteOutcome {
    passed: usize,
    fail_lines: Vec<String>,
    correction_failures: Vec<String>,
}

fn evaluate_all_examples() -> SuiteOutcome {
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = ga_pipeline();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut outcome = SuiteOutcome {
        passed: 0,
        fail_lines: Vec::new(),
        correction_failures: Vec::new(),
    };
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let matches = matches_for_example(&pipeline, &engine, group, &clean_text);
            let failed = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    matches.is_empty()
                        || marker_range.is_some_and(|(m_start, m_end)| {
                            let first = &matches[0];
                            let match_start = first.offset();
                            let match_end = match_start + first.length();
                            !(match_start <= m_start && match_end >= m_end)
                        })
                }
                og_xml::types::XmlExampleType::Correct => !matches.is_empty(),
                og_xml::types::XmlExampleType::TriggersError => false,
            };
            if failed {
                outcome
                    .fail_lines
                    .push(format!("FAIL: [{}]: '{}'", rule.id, clean_text));
                continue;
            }
            if let Some(expected) = example.corrections.first() {
                let offered = matches[0]
                    .replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect::<Vec<_>>();
                let empty_expected_with_no_offer = expected.is_empty() && offered.is_empty();
                if !empty_expected_with_no_offer && !offered.iter().any(|r| r == expected) {
                    outcome.correction_failures.push(format!(
                        "CORRECTION: [{}]: '{}' expected '{expected}' got {offered:?}",
                        rule.id, clean_text
                    ));
                    continue;
                }
            }
            outcome.passed += 1;
        }
    }
    outcome
}

#[test]
#[ignore]
fn irish_grammar_examples_with_ga_pipeline() {
    let _release_guard = crate::release_guard();
    let outcome = evaluate_all_examples();
    let total = outcome.passed + outcome.fail_lines.len() + outcome.correction_failures.len();
    eprintln!(
        "ga module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        outcome.passed,
        total,
        outcome.fail_lines.len(),
        outcome.correction_failures.len()
    );
    let mut all_failures = outcome.fail_lines;
    all_failures.extend(outcome.correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_ga_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 1843,
        "grammar.xml carries 1854 raw <example> elements; 11 sit inside XML comments"
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        1663,
        "ga grammar.xml compiles 1663 rule elements"
    );
    let groups = rule_groups(&rule_set);
    assert_eq!(groups.len(), 1663, "every ga rule carries a unique id key");
    let mut incorrect = 0usize;
    let mut correct = 0usize;
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => incorrect += 1,
                og_xml::types::XmlExampleType::Correct => correct += 1,
                og_xml::types::XmlExampleType::TriggersError => {}
            }
        }
    }
    assert_eq!(
        incorrect, 1739,
        "1739 correction-carrying examples are incorrect"
    );
    assert_eq!(
        correct, 104,
        "104 examples carry no correction and are correct"
    );
    assert_eq!(
        incorrect + correct,
        1843,
        "grammar.xml carries 1854 raw <example> elements; the other 11 sit inside XML comments"
    );
}

#[test]
fn natives_use_java_ids_in_relevant_rules_order() {
    let natives = super::irish_native_rules();
    let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
    assert_eq!(
        ids,
        vec![
            "WORD_REPEAT_RULE",
            "WHITESPACE_PARAGRAPH_BEGIN",
            "GA_LOGAINM",
            "GA_PEOPLE",
            "GA_SPASANNA",
            "GA_COMPOUNDS",
            "GA_PRESTANDARD_REPLACE",
            "GA_REPLACE",
            "GA_FGB_EQ_REPLACE",
            "GA_ENGLISH_HOMOPHONE",
            "GA_DHA_NO_BEIRT",
            "GA_DATIVE_PLURALS_STD",
            "GA_SPECIFIC_CASE",
        ],
        "Irish.java:93-123 getRelevantRules order, minus the rules the shared engine \
         installs generically (CommaWhitespace, DoublePunctuation, unpaired brackets, \
         UppercaseSentenceStart, LongSentence(50), LongParagraph, MultipleWhitespace, \
         SentenceWhitespace, WhiteSpaceBeforeParagraphEnd) and minus the data-blocked \
         MorfologikIrishSpellerRule"
    );
    let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
    assert_eq!(
        text_level_ids,
        vec!["WHITESPACE_PARAGRAPH", "PARAGRAPH_REPEAT_BEGINNING_RULE"]
    );
}

#[test]
fn srx_languagemap_resolves_the_irish_rule_cascade() {
    let splitter = og_tokenizer::srx::srx_splitter("ga").expect("ga SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("Sin fear lr. Bhí sé anseo. Cá bhfuil sé?");
    assert_eq!(
        sentences.len(),
        3,
        "Irish SRX breaks on the dot and the question mark"
    );
}

#[test]
fn tagger_loads_the_manual_added_txt_dictionary() {
    let pipeline = ga_pipeline();
    assert!(
        !pipeline.tagger.has_dictionary(),
        "irish.dict is absent from this checkout; only the added.txt manual data can load"
    );
    let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &["mnáibh"]);
    assert!(
        !tagged[0].readings().is_empty(),
        "'mnáibh' must carry readings from added.txt:6"
    );
    let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &["dheartháireacha"]);
    assert!(
        tagged[0].readings().is_empty(),
        "'dheartháireacha' has no reading: Java gets the 'deartháir' lemma from the \
         binary irish.dict which is absent here"
    );
}

#[test]
fn speller_stays_data_blocked_until_a_ga_ie_dict_lands() {
    let resource_dir = ga_resource_dir();
    assert!(
        !resource_dir.join("hunspell").join("ga_IE.dict").exists(),
        "MorfologikIrishSpellerRule loads /ga/hunspell/ga_IE.dict"
    );
    let mut any_dict = std::fs::read_dir(&resource_dir)
        .expect("ga resource dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "dict"))
        .peekable();
    assert!(
        any_dict.peek().is_none(),
        "no *.dict may land under the ga resource dir without the speller being wired"
    );
    let checker = module_checker();
    let ids: Vec<&str> = checker
        .rules()
        .iter()
        .map(|r| r.id())
        .chain(checker.text_level_rules().iter().map(|r| r.id()))
        .collect();
    assert!(
        !ids.contains(&"MORFOLOGIK_RULE_GA_IE"),
        "the engine must not install the speller while ga_IE.dict is absent, and must \
         not substitute the 50-word hunspell/spelling.txt wordlist as a primary dictionary"
    );
    for input in ["botun", "amaċ", "😂 botun"] {
        let result = checker.check(&og_core::CheckRequest::new(
            input.to_string(),
            Language::Irish,
        ));
        assert!(
            result.matches.is_empty(),
            "MorfologikIrishSpellerRuleTest input '{input}' must stay unflagged while \
             the data is absent (got {ids:?})"
        );
    }
}

fn sentence_with_lemma(text: &str, lemmas: &[(&str, &str, &str)]) -> AnalyzedSentence {
    let mut sentence = AnalyzedSentence::new(text, 0, text.len());
    let mut tokens = vec![AnalyzedTokenReadings::new(
        AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
    )];
    let mut pos = 0usize;
    for word in text.split(' ') {
        let start = pos;
        let end = start + word.len();
        pos = end + 1;
        let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(word, start, end));
        if let Some((_, lemma, tag)) = lemmas.iter().find(|(w, _, _)| *w == word) {
            atr = atr.with_readings(vec![
                AnalyzedToken::new(word, start, end)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![tag.to_string()]),
            ]);
        }
        tokens.push(atr);
    }
    sentence.set_tokens(tokens);
    sentence
}

#[test]
fn dha_no_beirt_pins_the_java_test_matrix() {
    use og_core::rule::Rule as _;
    let rule = super::DhaNoBeirtRule::new();
    let pipeline = ga_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()))
    };
    assert!(
        matches_for("Seo abairt bheag.").is_empty(),
        "DhaNoBeirtRuleTest.java:33 asserts correct"
    );
    let matches = matches_for("Seo dhá ab déag");
    assert_eq!(
        matches.len(),
        2,
        "DhaNoBeirtRuleTest.java:35 asserts two matches on 'dhá ab déag'"
    );
    assert_eq!(
        matches[0].message(),
        "Ba chóir duit <suggestion>dháréag</suggestion> a scríobh"
    );
    assert_eq!(matches[1].message(), "Ba chóir duit \"déag\" a scriosadh.");
    let dhá = "Seo dhá ab déag".find("dhá").expect("dhá span");
    assert_eq!(matches[0].offset(), dhá);
    assert_eq!(matches[0].offset() + matches[0].length(), dhá + "dhá".len());
    let deag = "Seo dhá ab déag".find("déag").expect("déag span");
    assert_eq!(matches[1].offset(), deag);
    assert!(
        matches_for("Tá beirt dheartháireacha agam.").is_empty(),
        "DhaNoBeirtRuleTest.java:34 asserts correct"
    );
    for (text, java_count) in [
        ("Tá dhá dheartháireacha agam.", 1),
        ("Tá dhá dheartháireacha níos aosta déag agam.", 2),
    ] {
        assert_eq!(
            matches_for(text).len(),
            0,
            "DhaNoBeirtRuleTest.java:36/:38 expects {java_count} matches on '{text}'; the              isPerson gate needs the 'deartháir' lemma reading which only the absent              irish.dict supplies (DhaNoBeirtRule.java:107-113)"
        );
    }
}

#[test]
fn dha_no_beirt_person_number_matches_via_the_lemma_reading() {
    use og_core::rule::Rule as _;
    let rule = super::DhaNoBeirtRule::new();
    let sentence = sentence_with_lemma(
        "Tá dhá dheartháireacha agam.",
        &[("dheartháireacha", "deartháir", "Noun:Masc:Nom:Pl")],
    );
    let matches = rule.match_sentence(&sentence);
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].message(),
        "Ba chóir duit <suggestion>beirt dheartháireacha</suggestion> a scríobh"
    );
    let sentence = sentence_with_lemma(
        "Tá dhá dheartháireacha níos aosta déag agam.",
        &[("dheartháireacha", "deartháir", "Noun:Masc:Nom:Pl")],
    );
    let matches = rule.match_sentence(&sentence);
    assert_eq!(
        matches.len(),
        2,
        "with the lemma reading the déag pair fires like Java"
    );
    assert_eq!(
        matches[0].message(),
        "Ba chóir duit <suggestion>dháréag</suggestion> a scríobh"
    );
    let text = "Tá dhá dheartháireacha níos aosta déag agam.";
    let dhá = text.find("dhá").expect("dhá span");
    assert_eq!(matches[0].offset(), dhá);
    let deag = text.find("déag").expect("déag span");
    assert_eq!(matches[1].offset(), deag);
}

#[test]
fn specific_case_pins_the_java_example_pair() {
    use og_core::rule::Rule as _;
    let rule = super::IrishSpecificCaseRule::new();
    let pipeline = ga_pipeline();
    let text = "Rugadh agus tógadh i mbéal Feirste é.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "IrishSpecificCaseRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "mBéal Feirste");
    let start = text.find("mbéal").expect("span start");
    assert_eq!(matches[0].offset(), start);
    assert_eq!(
        matches[0].offset() + matches[0].length(),
        start + "mbéal Feirste".len()
    );
    let good = "Rugadh agus tógadh i mBéal Feirste é.";
    assert!(
        rule.match_sentence(&pipeline.analyze_range(good, 0, good.len()))
            .is_empty()
    );
    let text = "Bhí sé i mBéal feirste inné.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "mBéal Feirste");
}

#[test]
fn compound_rule_pins_the_java_example_pair() {
    use og_core::rule::Rule as _;
    let rule = super::IrishCompoundRule::new();
    assert_eq!(
        rule.entry_count(),
        2,
        "compounds.txt carries ró úsáid and mí úsáid"
    );
    let pipeline = ga_pipeline();
    let text = "Tá mí úsáid fhisiciúil i gceist.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "mí-úsáid");
    let start = text.find("mí úsáid").expect("compound span");
    assert_eq!(matches[0].offset(), start);
    assert_eq!(
        matches[0].offset() + matches[0].length(),
        start + "mí úsáid".len()
    );
    assert_eq!(
        matches[0].message(),
        "Litrítear an focal seo le fleiscín de ghnáth."
    );
    assert_eq!(
        matches[0].short_message(),
        Some("Fadhb leis an bhfleiscíniú".to_string()).as_deref()
    );
    let good = "Tá mí-úsáid fhisiciúil i gceist.";
    assert!(
        rule.match_sentence(&pipeline.analyze_range(good, 0, good.len()))
            .is_empty()
    );
    let text = "Bhí ró úsáid ann.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "ró-úsáid");
    let text = "Ró úsáid atá ann.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(
        matches.len(),
        1,
        "the sentence-start key is uncapitalised before lookup (AbstractCompoundRule.java:222)"
    );
    assert_eq!(matches[0].replacements()[0].value(), "Ró-úsáid");
}

#[test]
fn replace_rules_pin_their_java_example_pairs() {
    use og_core::rule::Rule as _;
    let pipeline = ga_pipeline();
    let rule = super::irish_replace_rule();
    let text = "Níl beann agam oraibh, ar bhúr gcuid cainte.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "IrishReplaceRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "bhur");
    assert_eq!(matches[0].message(), "Litriú mícheart: \"bhur\".");
    assert_eq!(
        matches[0].short_message(),
        Some("Litriú mícheart".to_string()).as_deref()
    );
    let rule = super::prestandard_replace_rule();
    let text = "“Ní baoghal daoibh,” ar sise.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "PrestandardReplaceRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "baol");
    assert_eq!(matches[0].message(), "Litriú réamhchaighdeánach:\"baol\".");
    let rule = super::irish_fgb_eq_replace_rule();
    let text = "An bhfuil tú urlamh?";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "IrishFGBEqReplaceRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "ullamh");
    assert_eq!(
        matches[0].message(),
        "Focal ceart ach tá \"ullamh\" níos coitianta."
    );
    let rule = super::english_homophone_rule();
    let text = "An bhialann sushi sin ba chúis leis.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "EnglishHomophoneRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "súisí");
    assert_eq!(
        matches[0].message(),
        "Is é súisí an litriú Gaelach ar \"sushi\""
    );
    let rule = super::people_rule();
    let text = "Bhí sí cosúil le claíomh Damocles ar crochadh sa spéir.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "PeopleRule example pair");
    assert_eq!(matches[0].replacements()[0].value(), "Dámaicléas");
    let rule = super::logainm_rule();
    let text = "Bhí mé i London inné.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "placenames.txt:596 Dublin family");
    assert_eq!(matches[0].replacements()[0].value(), "Londain");
    assert_eq!(
        matches[0].message(),
        "Is logainm Béarla é \"London\" ar \"Londain\"."
    );
    let rule = super::spaces_rule();
    let text = "Níl a fhios agam abhfuil sé sin ceart.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "spaces.txt:6 abhfuil");
    assert_eq!(matches[0].replacements()[0].value(), "a bhfuil");
}

#[test]
fn dative_plurals_rule_pins_the_java_example_pair() {
    use og_core::rule::Rule as _;
    let rule = super::dative_plural_standard_replace_rule();
    let pipeline = ga_pipeline();
    let text =
        "Dá mba thruamhéalach é cás an sclábhaí fir, ba mheasa fós do na mnáibh a gcás siúd.";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(
        matches.len(),
        1,
        "DativePluralStandardReplaceRule example pair"
    );
    assert_eq!(matches[0].replacements()[0].value(), "mná");
    let start = text.find("mnáibh").expect("dative span");
    assert_eq!(matches[0].offset(), start);
    assert_eq!(
        matches[0].message(),
        "Is litriú réamhchaighdeánach (tabharthach iolra) é \"mnáibh\"; an \"mná\" a bhí i gceist agat?"
    );
    let text = "do na bheachaibh óga";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(
        matches.len(),
        1,
        "Utils.lenite maps the lenited dative 'bheachaibh'"
    );
    assert_eq!(matches[0].replacements()[0].value(), "bheacha");
    let text = "sna háiteachaibh sin";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(
        matches.len(),
        1,
        "h-prothesis maps vowel-initial 'háiteachaibh'"
    );
    assert_eq!(matches[0].replacements()[0].value(), "háiteanna");
    let text = "sna ingheanaibh sin";
    let matches = rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()));
    assert_eq!(matches.len(), 1, "dative-plurals.txt:20 modern form");
    assert_eq!(matches[0].replacements()[0].value(), "iníonacha");
}

#[test]
fn module_checker_registers_the_java_relevant_rule_inventory() {
    let checker = module_checker();
    let ids: Vec<&str> = checker
        .rules()
        .iter()
        .map(|r| r.id())
        .chain(checker.text_level_rules().iter().map(|r| r.id()))
        .collect();
    for expected in [
        "COMMA_PARENTHESIS_WHITESPACE",
        "DOUBLE_PUNCTUATION",
        "UPPERCASE_SENTENCE_START",
        "UNPAIRED_BRACKETS",
        "WHITESPACE_RULE",
        "SENTENCE_WHITESPACE",
        "TOO_LONG_SENTENCE",
        "TOO_LONG_PARAGRAPH",
        "WORD_REPEAT_RULE",
        "WHITESPACE_PARAGRAPH",
        "WHITESPACE_PARAGRAPH_BEGIN",
        "PARAGRAPH_REPEAT_BEGINNING_RULE",
        "GA_LOGAINM",
        "GA_PEOPLE",
        "GA_SPASANNA",
        "GA_COMPOUNDS",
        "GA_PRESTANDARD_REPLACE",
        "GA_REPLACE",
        "GA_FGB_EQ_REPLACE",
        "GA_ENGLISH_HOMOPHONE",
        "GA_DHA_NO_BEIRT",
        "GA_DATIVE_PLURALS_STD",
        "GA_SPECIFIC_CASE",
    ] {
        assert!(
            ids.contains(&expected),
            "Irish.java getRelevantRules inventory missing '{expected}'"
        );
    }
    assert!(
        ids.contains(&"LE_ME_LIOM"),
        "grammar.xml rules must load; LE_ME_LIOM is grammar.xml's first rule"
    );
}

fn rule_gates_on_readings(rule: &og_xml::compiler::CompiledRule) -> bool {
    use og_xml::compiler::{CompiledPattern, CompiledPatternToken};
    fn token_gated(token: &CompiledPatternToken) -> bool {
        token.postag.is_some()
            || token.inflected
            || token
                .exceptions
                .iter()
                .any(|e| e.postag.is_some() || e.inflected)
    }
    fn pattern_gated(pattern: &CompiledPattern) -> bool {
        pattern.tokens.iter().any(token_gated)
    }
    pattern_gated(&rule.pattern) || rule.antipatterns.iter().any(pattern_gated)
}

#[test]
fn no_match_residue_is_exactly_the_missing_irish_dict_family() {
    let _release_guard = crate::release_guard();
    let outcome = evaluate_all_examples();
    assert_eq!(outcome.passed, 1552);
    assert_eq!(outcome.fail_lines.len(), 271);
    assert_eq!(outcome.correction_failures.len(), 20);
    let rule_set = compiled_rules();
    let failing_ids: std::collections::HashSet<&str> = outcome
        .fail_lines
        .iter()
        .filter_map(|l| l.strip_prefix("FAIL: ["))
        .filter_map(|l| l.split(']').next())
        .collect();
    let mut ungated = Vec::new();
    for id in failing_ids {
        let rules = rule_set
            .rules
            .iter()
            .filter(|r| r.id == id)
            .collect::<Vec<_>>();
        assert!(!rules.is_empty(), "{id} must exist in grammar.xml");
        if !rules.iter().all(|r| rule_gates_on_readings(r)) {
            ungated.push(id);
        }
    }
    assert!(
        ungated.is_empty(),
        "every failing rule must gate on postag/inflected readings in its pattern, \
         exceptions or antipatterns (only irish.dict, absent from this checkout, \
         supplies them); ungated: {ungated:?}"
    );
    let resource_dir = ga_resource_dir();
    assert!(!resource_dir.join("irish.dict").exists());
    assert!(!resource_dir.join("hunspell").join("ga_IE.dict").exists());
    let pipeline = ga_pipeline();
    for word in ["dheartháireacha", "theach", "liom"] {
        let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &[word]);
        assert!(
            tagged[0].readings().is_empty(),
            "'{word}' must carry no readings: added.txt is the only live dictionary"
        );
    }
}

#[test]
fn correction_residue_pins_its_shared_engine_families() {
    let rule_set = compiled_rules();
    let pipeline = ga_pipeline();
    let engine = PatternRuleEngine::new();
    let offered_for = |rule_id: &str, text: &str| -> Vec<Vec<String>> {
        rule_set
            .rules
            .iter()
            .filter(|r| r.id == rule_id)
            .flat_map(|rule| {
                let group = vec![rule];
                matches_for_example(&pipeline, &engine, &group, text)
                    .into_iter()
                    .map(|m| {
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    assert_eq!(
        offered_for("AN_SIONAINN", "an Sionainn"),
        vec![vec!["tSionainn".to_string()]],
        "the <match regexp_match=\"^(.*)$\" regexp_replace=\"t$1\" \
         case_conversion=\"startlower\"/> transform yields 'tSionainn' exactly like Java's \
         own example pin (grammar.xml:18752 correction='tSionainn'): startlower only \
         lowercases the first char, which the t$1 replace already made lowercase - the \
         in-token <match> references port (191a205) closed the gap this pin used to \
         document"
    );
    assert_eq!(
        offered_for("AR_UNECLIPSED", "ár teach"),
        vec![vec!["teach".to_string()]],
        "the <match no=\"2\" postag=\"([^:]*):(.*)\" postag_replace=\"$1:$2:Ecl\"/> \
         suggestion needs the matched token's postag reading (irish.dict) to synthesise \
         the eclipsed 'dteach'"
    );
    assert_eq!(
        offered_for(
            "CE_MEID",
            "Cé mhéid gealladh ar briseadh ar an Indiach bocht?"
        ),
        vec![vec!["cé mhéad".to_string()]],
        "Java capitalises the suggestion at sentence start via RuleMatch.java:198-199; \
         the shared og-rules suggestion path has no uppercaseFirstChar step (the el \
         round's GREEK_WHERE- finding)"
    );
    assert_eq!(
        offered_for("FATA", "fata"),
        vec![vec!["fatapráta".to_string()]],
        "the <match no=\"1\" postag=\"Noun:.*\">práta</match> suggestion part renders \
         the matched text and the literal concatenated instead of the literal alone"
    );
}

#[test]
fn module_checker_flags_a_mixed_text_without_panicking() {
    let checker = module_checker();
    let text = "Bhí mé i London aréir.. Tá dhá ab déag agam, ach níl a fhios agam cén chaoi a abhfuil sé seo ceart.\n\nBhí mé i London arís.";
    let result = checker.check(&og_core::CheckRequest::new(
        text.to_string(),
        Language::Irish,
    ));
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    assert!(
        ids.contains(&"DOUBLE_PUNCTUATION"),
        "the '..' must flag, got {ids:?}"
    );
    assert!(
        ids.contains(&"GA_LOGAINM"),
        "'London' must flag twice (once per sentence), got {ids:?}"
    );
    assert!(
        ids.iter().filter(|id| **id == "GA_LOGAINM").count() == 2,
        "both London occurrences must flag, got {ids:?}"
    );
    assert!(
        ids.contains(&"GA_DHA_NO_BEIRT"),
        "'dhá ab déag' must flag, got {ids:?}"
    );
    assert!(
        ids.contains(&"GA_SPASANNA"),
        "'abhfuil' must flag, got {ids:?}"
    );
}
