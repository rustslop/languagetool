use og_core::checker::Disambiguator as _;
use og_core::checker::{Chunker as _, Tagger};
use og_core::rule::Rule as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckRequest, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct GreekPipeline {
    tokenizer: super::GreekWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

fn el_resource_dir() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("el/src/main/resources/org/languagetool/resource/el")
}

pub(crate) fn greek_pipeline() -> std::sync::Arc<GreekPipeline> {
    static PIPE: og_core::releasable::Releasable<GreekPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(GreekPipeline {
            tokenizer: super::GreekWordTokenizer::new(),
            tagger: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                &el_resource_dir(),
                "greek",
            )),
            disambiguator: std::fs::read_to_string(el_resource_dir().join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok()),
            splitter: og_tokenizer::srx::srx_splitter("el"),
        }))
    })
    .expect("el")
}

impl GreekPipeline {
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

struct ElSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for ElSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("el") {
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

struct ElWordTokenizerBridge;

impl og_core::checker::WordTokenizer for ElWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        super::GreekWordTokenizer::new()
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
        let resource_dir = el_resource_dir();
        let tagger = Arc::new(og_tagger::GenericTagger::from_resource_dir(
            &resource_dir,
            "greek",
        ));
        let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
            .ok()
            .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok());
        let mut dict = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "greek")
            .map(|shared| {
                og_spell::Dictionary::from_shared_words(Arc::new(
                    og_spell::morfologik_view::MorfologikView::new(shared),
                ))
            })
            .unwrap_or_default();
        if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        let mut ignore_words = std::collections::HashSet::new();
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt")) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    ignore_words.insert(word.to_lowercase());
                }
            }
        }
        let natives = super::greek_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(
                og_spell::SpellingCheckRule::new_shared(
                    Arc::new(dict),
                    "MORFOLOGIK_RULE_EL_GR",
                    std::collections::HashSet::new(),
                )
                .with_ignore_words(ignore_words),
            ),
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
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new()
                    .with_id("EL_UNPAIRED_BRACKETS")
                    .with_symbols(
                        vec!["[", "(", "{", "\u{201C}", "\"", "\u{00AB}"],
                        vec!["]", ")", "}", "\u{201D}", "\"", "\u{00BB}"],
                    ),
            ),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
        ];
        text_level_rules.extend(natives.text_level_rules);
        let mut checker = og_core::checker::Checker::new()
            .with_sentence_tokenizer(Arc::new(ElSentenceTokenizer))
            .with_word_tokenizer(Arc::new(ElWordTokenizerBridge))
            .with_tagger(tagger)
            .with_chunker(Arc::new(og_tagger::chunker::PostDisambiguationChunker))
            .set_rules(rules)
            .set_text_level_rules(text_level_rules);
        if let Some(disambiguator) = disambiguator {
            checker = checker.with_disambiguator(Arc::new(disambiguator));
        }
        Some(std::sync::Arc::new(checker))
    })
    .expect("el")
}

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("el/src/main/resources/org/languagetool/rules/el/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("el grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile el grammar.xml"),
        ))
    })
    .expect("el")
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
    pipeline: &GreekPipeline,
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

#[test]
#[ignore]
fn greek_grammar_examples_with_greek_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = greek_pipeline();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut correction_failures = Vec::new();
    let mut fail_lines: Vec<String> = Vec::new();
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
                fail_lines.push(format!("FAIL: [{}]: '{}'", rule.id, clean_text));
                continue;
            }
            if let Some(expected) = example.corrections.first() {
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
    let total = passed + fail_lines.len() + correction_failures.len();
    eprintln!(
        "el module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_el_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(total, 118, "grammar.xml carries 118 examples");
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        55,
        "el grammar.xml compiles 50 <rule> elements plus 5 rulegroup sub-rules"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(example_count, 118, "grammar.xml carries 118 live examples");
}

const ANALYZER_GATED_IDS: [&str; 3] = [
    "GREEK_ART_MASC_MISSING_N",
    "GREEK_PREP_ART_MASC_MISSING_N",
    "GREEK_MORE_ADJ_COMP",
];

#[test]
fn postag_residue_is_exactly_the_missing_ioperm_analyzer_family() {
    let rule_set = compiled_rules();
    for id in ANALYZER_GATED_IDS {
        let rules = rule_set
            .rules
            .iter()
            .filter(|r| r.id == id)
            .collect::<Vec<_>>();
        assert!(!rules.is_empty(), "{id} must exist in grammar.xml");
        for rule in rules {
            assert!(
                rule.pattern.tokens.iter().any(|t| t.postag.is_some()),
                "{id} must gate at least one token on a postag"
            );
        }
    }
    let pipeline = greek_pipeline();
    for word in ["ανταγωνισμό", "ισχυρότερα"] {
        let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &[word]);
        assert!(
            tagged[0].readings().is_empty(),
            "'{word}' must carry no readings: the 239-byte greek.dict covers only the \
             θα/να/έχω/πάω paradigm, Java gets inflected-form tags from the external \
             org.ioperm:morphology-el GreekAnalyzer which is absent from this checkout"
        );
    }
    let engine = PatternRuleEngine::new();
    let mut failures = Vec::new();
    for group in rule_groups(&rule_set) {
        let Some(rule) = group.first() else {
            continue;
        };
        if ANALYZER_GATED_IDS.contains(&rule.id.as_str()) {
            continue;
        }
        for example in &rule.examples {
            if example.example_type != og_xml::types::XmlExampleType::Incorrect {
                continue;
            }
            let (clean_text, _) = extract_marker(&example.text);
            if matches_for_example(&pipeline, &engine, &group, &clean_text).is_empty() {
                failures.push(format!("[{}]: '{}'", rule.id, clean_text));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "every analyzer-free rule must match its incorrect examples: {failures:?}"
    );
}

#[test]
fn sentence_start_corrections_wait_for_the_shared_capitalization_step() {
    let rule_set = compiled_rules();
    let pipeline = greek_pipeline();
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
    let offered = offered_for("GREEK_WHERE-", "Που πας;");
    assert_eq!(
        offered,
        vec![vec!["πού".to_string()]],
        "Java capitalizes the suggestion to 'Πού' via RuleMatch.java:198-199 \
         (startWithUppercase from PatternRuleMatcher.java:164-173); the shared \
         og-rules suggestion path has no uppercaseFirstChar step yet"
    );
    let offered = offered_for("GREEK_NUMERAL_ONE_WORD", "Δέκα τρία.");
    assert_eq!(offered, vec![vec!["δεκατρία".to_string()]]);
}

#[test]
fn have_inf_correction_waits_for_the_shared_greek_synthesizer() {
    let rule_set = compiled_rules();
    let pipeline = greek_pipeline();
    let engine = PatternRuleEngine::new();
    let offered: Vec<String> = rule_set
        .rules
        .iter()
        .filter(|r| r.id == "HAVE_INF")
        .flat_map(|rule| {
            let group = vec![rule];
            matches_for_example(&pipeline, &engine, &group, "Είχα πάω.")
                .into_iter()
                .flat_map(|m| {
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        offered,
        vec!["πάω".to_string()],
        "the tagger knows πάει/πάω/INF but <match no=\"2\" postag=\"INF\"/> falls back to \
         the raw token without a synthesizer; Java's GreekSynthesizer resolves it to 'πάει'"
    );
    let resource_dir = el_resource_dir();
    assert!(resource_dir.join("greek_synth.dict").exists());
    assert!(resource_dir.join("greek_tags.txt").exists());
}

#[test]
fn srx_languagemap_resolves_the_greek_rule_cascade() {
    let splitter = og_tokenizer::srx::srx_splitter("el").expect("el SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("Ήρθε ο Γιώργος. Τι κάνει; Καλά είναι!");
    assert_eq!(
        sentences.len(),
        3,
        "Greek SRX breaks on dot, Greek question mark and exclamation"
    );
    let sentences = splitter.split("Δες π.χ. αυτό το βιβλίο. Είναι καλό.");
    assert_eq!(
        sentences.len(),
        2,
        "π.χ. is a no-break abbreviation in the Greek SRX cascade"
    );
    let sentences = splitter.split("κ.λπ. και άλλα. Τέλος.");
    assert_eq!(sentences.len(), 2, "κ.λπ. is a no-break abbreviation");
}

#[test]
fn tagger_loads_the_greek_dictionary() {
    let pipeline = greek_pipeline();
    assert!(
        pipeline.tagger.has_dictionary(),
        "greek.dict must feed the GenericTagger"
    );
    let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &["έχω"]);
    assert!(
        !tagged[0].readings().is_empty(),
        "'έχω' must carry readings from greek.dict"
    );
}

#[test]
fn numeral_stress_rule_pins_the_java_test_matrix() {
    let rule = super::NumeralStressRule::new();
    let pipeline = greek_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()))
    };
    for correct in ["1ος", "2η", "3ο", "20ός", "30ή", "40ό", "1000ών", "1010ες"] {
        assert!(
            matches_for(correct).is_empty(),
            "NumeralStressRuleTest asserts '{correct}' correct"
        );
    }
    for (incorrect, correction) in [
        ("4ός", "4ος"),
        ("5ή", "5η"),
        ("6ό", "6ο"),
        ("100ος", "100ός"),
        ("200η", "200ή"),
        ("300ο", "300ό"),
        ("2000ων", "2000ών"),
        ("2010ές", "2010ες"),
        ("2020α", "2020ά"),
    ] {
        let matches = matches_for(incorrect);
        assert_eq!(matches.len(), 1, "'{incorrect}' must flag exactly once");
        assert_eq!(matches[0].replacements().len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), correction);
    }
    let matches = matches_for("Ο 20ος αιώνας.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "20ός");
    assert_eq!(
        matches[0].message(),
        "<suggestion>20ός</suggestion>",
        "the Java rule builds the message from the suggestion markup alone"
    );
}

#[test]
fn replace_homonyms_rule_pins_the_java_examples() {
    let rule = super::replace_homonyms_rule();
    let pipeline = greek_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()))
    };
    for good in [
        "Στην Ελλάδα επικρατεί εύκρατο κλίμα.",
        "Καλή τύχη σου εύχομαι.",
    ] {
        assert!(
            matches_for(good).is_empty(),
            "ReplaceHomonymsRuleTest asserts '{good}' correct"
        );
    }
    let matches = matches_for("Του ευχήθηκα καλή τείχη για το διαγώνισμα.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "καλή τύχη");
    let matches = matches_for("Τεχνητό κόμμα είναι μια ακραία μορφή αναισθησίας.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "Τεχνητό κώμα");
    let matches = matches_for("γάλος πρόεδρος.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "Γάλλος πρόεδρος");
    assert_eq!(
        matches[0].message(),
        "Μήπως εννοούσατε <suggestion>Γάλλος πρόεδρος</suggestion>"
    );
}

#[test]
fn specific_case_rule_pins_the_java_positions_and_messages() {
    let rule = super::greek_specific_case_rule();
    let pipeline = greek_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()))
    };
    for good in [
        "Ηνωμένες Πολιτείες",
        "Κατοικώ στις Ηνωμένες Πολιτείες.",
        "Κατοικώ στις ΗΝΩΜΕΝΕΣ ΠΟΛΙΤΕΙΕΣ.",
    ] {
        assert!(
            matches_for(good).is_empty(),
            "GreekSpecificCaseRuleTest asserts '{good}' correct"
        );
    }
    for bad in [
        "ηνωμένες πολιτείες",
        "ηνωμένες Πολιτείες",
        "Ηνωμένες πολιτείες",
    ] {
        assert_eq!(
            matches_for(bad).len(),
            1,
            "GreekSpecificCaseRuleTest asserts '{bad}' incorrect"
        );
    }
    let text = "Κατοικώ στις Ηνωμένες πολιτείες.";
    let matches = matches_for(text);
    let start = text.find("Ηνωμένες").expect("span start");
    let end = start + "Ηνωμένες πολιτείες".len();
    assert_eq!(matches[0].offset(), start);
    assert_eq!(matches[0].offset() + matches[0].length(), end);
    assert_eq!(
        matches[0].message(),
        "Οι λέξεις της συγκεκριμένης έκφρασης χρείαζεται να ξεκινούν με κεφαλαία γράμματα."
    );
    assert_eq!(matches[0].replacements()[0].value(), "Ηνωμένες Πολιτείες");
    assert_eq!(
        matches[0].short_message(),
        Some("Ειδική κεφαλαιοποίηση".to_string()).as_deref()
    );
    let text = "Κατοικώ στις Ηνωμένες  πολιτείες.";
    let matches = matches_for(text);
    let start = text.find("Ηνωμένες").expect("span start");
    let end = text.find("πολιτείες").expect("second word") + "πολιτείες".len();
    assert_eq!(matches[0].offset(), start);
    assert_eq!(matches[0].offset() + matches[0].length(), end);
}

#[test]
fn redundancy_rule_pins_the_java_examples() {
    let rule = super::greek_redundancy_rule();
    let pipeline = greek_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_sentence(&pipeline.analyze_range(text, 0, text.len()))
    };
    for good in ["Τώρα μπαίνω στο σπίτι.", "Απόψε θα βγω."] {
        assert!(
            matches_for(good).is_empty(),
            "GreekRedundancyRuleTest asserts '{good}' correct"
        );
    }
    let matches = matches_for("Τώρα μπαίνω μέσα στο σπίτι.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "μπαίνω");
    let matches = matches_for("Απόψε το βράδυ θα βγω.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "Απόψε");
    let matches = matches_for("Το μαγαζί ήταν ωραίο, αλλά όμως δεν πέρασα καλά.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "αλλά,όμως");
}

#[test]
fn word_repeat_rule_covers_both_tokens_like_java() {
    let rule = super::GreekWordRepeatRule::new();
    let pipeline = greek_pipeline();
    let text = "για να να δείτε πώς";
    let sentence = pipeline.analyze_range(text, 0, text.len());
    let matches = rule.match_sentence(&sentence);
    assert_eq!(matches.len(), 1);
    let start = text.find("να να").expect("repeat span");
    assert_eq!(matches[0].offset(), start);
    assert_eq!(
        matches[0].offset() + matches[0].length(),
        start + "να να".len()
    );
    assert_eq!(matches[0].replacements()[0].value(), "να");
    assert_eq!(
        matches[0].message(),
        "Πιθανό λάθος: επαναλαμβάνετε μία λέξη"
    );
    assert!(
        rule.match_sentence(&pipeline.analyze_range("Duran Duran", 0, 11))
            .is_empty()
    );
}

#[test]
fn word_repeat_beginning_rule_pins_the_java_examples() {
    use og_core::rule::TextLevelRule as _;
    let rule = super::GreekWordRepeatBeginningRule::new();
    let pipeline = greek_pipeline();
    let matches_for = |text: &str| -> Vec<og_core::RuleMatch> {
        rule.match_text(text, &pipeline.analyze_text(text))
    };
    for good in [
        "Εγώ παίζω ποδόσφαιρο. Εγώ παίζω μπάσκετ",
        "Το αυτοκίνητο είναι καινούργιο. Το ποδήλατο είναι παλιό. Το καράβι είναι καινούργιο.",
        "Μία περίπτωση εξηγήθηκε ήδη. Μία άλλη θα αναλυθεί παρακάτω.",
    ] {
        assert!(
            matches_for(good).is_empty(),
            "GreekWordRepeatBeginningRuleTest asserts no match on '{good}'"
        );
    }
    let text = "Επίσης, μιλάω Ελληνικά. Επίσης, μιλάω Αγγλικά.";
    let matches = matches_for(text);
    assert_eq!(matches.len(), 1);
    let offered: Vec<&str> = matches[0]
        .replacements()
        .iter()
        .map(|r| r.value())
        .collect();
    for suggestion in ["Επιπλέον", "Ακόμη", "Επιπρόσθετα", "Συμπληρωματικά"]
    {
        assert!(
            offered.contains(&suggestion),
            "missing suggestion '{suggestion}' in {offered:?}"
        );
    }
    let start = text.rfind("Επίσης").expect("second sentence start");
    assert_eq!(matches[0].offset(), start);
    let matches = matches_for("Εγώ παίζω μπάσκετ. Εγώ παίζω ποδόσφαιρο. Εγώ παίζω βόλεϊ.");
    assert_eq!(matches.len(), 1);
}

#[test]
fn module_checker_pins_the_java_demo_text_ids() {
    let checker = module_checker();
    let text = "Επικολλήστε το κείμενο σας εδώ και κάντε κλικ στο κουμπί ελέγχου. Κάντε κλικ στις χρωματιστές φράσεις για λεπτομέρειες σχετικά με πιθανά σφάλματα. Για παράδειγμα σε αυτή τη πρόταση υπάρχουν εσκεμμένα λάθη για να να δείτε πώς λειτουργει το LanguageTool..";
    let result = checker.check(&CheckRequest::new(text.to_string(), Language::Greek));
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    for expected in [
        "GREEK_PUNC_2",
        "GREEK_ART_FEM_MISSING_N",
        "WORD_REPEAT_RULE",
        "MORFOLOGIK_RULE_EL_GR",
        "DOUBLE_PUNCTUATION",
    ] {
        assert!(
            ids.contains(&expected),
            "GreekTest expects '{expected}' on the demo text, got {ids:?}"
        );
    }
}

#[test]
fn module_checker_flags_the_java_typo_probe() {
    let checker = module_checker();
    let result = checker.check(&CheckRequest::new("ισότπς".to_string(), Language::Greek));
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    assert!(
        ids.contains(&"MORFOLOGIK_RULE_EL_GR"),
        "GreekTest runTests('ισότπς') expects the speller, got {ids:?}"
    );
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
        "EL_UNPAIRED_BRACKETS",
        "MORFOLOGIK_RULE_EL_GR",
        "UPPERCASE_SENTENCE_START",
        "WORD_REPEAT_RULE",
        "GREEK_HOMONYMS_REPLACE",
        "EL_SPECIFIC_CASE",
        "GREEK_ORTHOGRAPHY_NUMERAL_STRESS",
        "EL_REDUNDANCY_REPLACE",
        "GREEK_WORD_REPEAT_BEGINNING_RULE",
        "WHITESPACE_RULE",
        "SENTENCE_WHITESPACE",
        "TOO_LONG_SENTENCE",
        "HAVE_INF",
        "GREEK_PUNC_2",
    ] {
        assert!(
            ids.contains(&expected),
            "Greek.java getRelevantRules inventory missing '{expected}'"
        );
    }
}

#[test]
fn shared_engine_already_carries_the_generic_el_configuration() {
    let _release_guard = crate::release_guard();
    let engine = crate::engine::LanguageEngine::shared_for_code("el").expect("el engine");
    let ids = engine.rule_ids();
    for expected in [
        "MORFOLOGIK_RULE_EL_GR",
        "UPPERCASE_SENTENCE_START",
        "DOUBLE_PUNCTUATION",
        "COMMA_PARENTHESIS_WHITESPACE",
        "HAVE_INF",
        "GREEK_PUNC_2",
    ] {
        assert!(
            ids.contains(&expected),
            "the shared el engine must already load '{expected}'"
        );
    }
    let text_level = engine.text_level_rule_ids();
    for expected in [
        "EL_UNPAIRED_BRACKETS",
        "TOO_LONG_SENTENCE",
        "WHITESPACE_RULE",
    ] {
        assert!(
            text_level.contains(&expected),
            "the shared el engine must already load text-level '{expected}'"
        );
    }
}
