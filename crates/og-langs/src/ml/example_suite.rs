use og_core::checker::Tagger as _;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckRequest, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct MlPipeline {
    tokenizer: super::MalayalamWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ml_pipeline() -> std::sync::Arc<MlPipeline> {
    static PIPE: og_core::releasable::Releasable<MlPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(MlPipeline {
            tokenizer: super::MalayalamWordTokenizer::new(),
            tagger: super::malayalam_tagger().expect("malayalam.dict tagger"),
            splitter: og_tokenizer::srx::srx_splitter("ml"),
        }))
    })
    .expect("ml")
}

impl MlPipeline {
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
            for token in tokens.iter_mut().rev() {
                if !token.is_whitespace() && !token.has_pos_tag("SENT_START") {
                    if !token.has_pos_tag("SENT_END") {
                        token.add_sent_end();
                    }
                    break;
                }
            }
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

struct MlSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for MlSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("ml") {
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

struct MlWordTokenizerBridge;

impl og_core::checker::WordTokenizer for MlWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        super::MalayalamWordTokenizer::new()
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
        let tagger = super::malayalam_tagger().expect("malayalam.dict tagger");
        let natives = super::malayalam_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = natives.rules.clone();
        for compiled in compiled_rules().rules.clone() {
            if compiled.default_on && !compiled.deprecated {
                rules.push(Arc::new(crate::engine::XmlPatternRule::new(compiled)));
            }
        }
        Some(std::sync::Arc::new(
            og_core::checker::Checker::new()
                .with_sentence_tokenizer(Arc::new(MlSentenceTokenizer))
                .with_word_tokenizer(Arc::new(MlWordTokenizerBridge))
                .with_tagger(tagger)
                .set_rules(rules)
                .set_text_level_rules(natives.text_level_rules.clone()),
        ))
    })
    .expect("ml")
}

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("ml/src/main/resources/org/languagetool/rules/ml/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ml grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ml grammar.xml"),
        ))
    })
    .expect("ml")
}

fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = ml_pipeline();
    let natives = super::malayalam_native_rules();
    let rules = compiled_rules();
    let engine = PatternRuleEngine::new();
    let sentences = pipeline.analyze_text(text);
    let mut all_matches = Vec::new();
    for rule in &natives.rules {
        for sentence in &sentences {
            all_matches.extend(rule.match_sentence(sentence));
        }
    }
    for compiled in rules.rules.iter().filter(|r| r.default_on && !r.deprecated) {
        for sentence in &sentences {
            all_matches.extend(engine.match_rule(compiled, sentence));
        }
    }
    for rule in &natives.text_level_rules {
        all_matches.extend(rule.match_text(text, &sentences));
    }
    static BASE: BaseLanguageMatchFilter = BaseLanguageMatchFilter;
    og_core::match_filters::same_rule_group_filter(&mut all_matches);
    BASE.filter_rule_matches(&mut all_matches, text);
    clean_overlapping_filter(all_matches, text, &BASE)
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
    pipeline: &MlPipeline,
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
fn malayalam_grammar_examples_with_ml_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = ml_pipeline();

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
                if expected.is_empty() {
                    passed += 1;
                    continue;
                }
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
        "ml module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_ml_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 38,
        "all live <example> elements of ml grammar.xml must be evaluated (the file carries 49, 11 \
         of them sit inside the commented-out rules 6/9/13/16/18)"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all live ml examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        18,
        "ml grammar.xml compiles 18 pattern rules: the active <rule> elements അവളെ, അവനെ, \
         രാജന്, rule4, rule5, rule7, rule8, rule10, rule_11, rule12, rule14, rule15, rule17, \
         rule19, rule20, rule21, rule22, ഭയങ്കര (rules 6/9/13/16/18 are commented out)"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 38,
        "the live example inventory of ml grammar.xml"
    );
}

#[test]
fn srx_matches_malayalam_srx_sentence_tokenizer_test() {
    use og_tokenizer::SentenceTokenizer as _;
    let pipeline = ml_pipeline();
    let splitter = pipeline
        .splitter
        .as_ref()
        .expect("the (ML|ml).* -> Generic languagemap must resolve for ml");
    let first = "1983 ൽ റിച്ചാർഡ് സ്റ്റാൾമാൻ സ്ഥാപിച്ച ഗ്നു എന്ന സംഘടനയിൽ നിന്നും വളർന്നു വന്ന സോഫ്റ്റ്‌വെയറും ടൂളുകളുമാണ് ഇന്ന് ഗ്നൂ/ലിനക്സിൽ ലഭ്യമായിട്ടുള്ള സോഫ്റ്റ്‌വെയറിൽ സിംഹഭാഗവും. ";
    let second = "ഗ്നു സംഘത്തിന്റെ മുഖ്യലക്ഷ്യം സ്വതന്ത്ര സോഫ്റ്റ്‌വെയറുകൾ മാത്രം ഉപയോഗിച്ചുകൊണ്ട് യുണിക്സ് പോലുള്ള ഒരു ഓപ്പറേറ്റിംഗ് സിസ്റ്റം നിർമ്മിക്കുന്നതായിരുന്നു.";
    let texts: Vec<String> = splitter
        .split(&format!("{first}{second}"))
        .into_iter()
        .map(|s| s.text().to_string())
        .collect();
    assert_eq!(
        texts,
        vec![first.to_string(), second.to_string()],
        "MalayalamSRXSentenceTokenizerTest.testTokenize expects exactly this two-way split"
    );
}

#[test]
fn tagger_dictionary_loads() {
    let pipeline = ml_pipeline();
    let text = "എങ്ങനെ";
    let sentence = pipeline.analyze_range(text, 0, text.len());
    assert!(
        sentence
            .tokens()
            .iter()
            .any(|t| !t.readings().is_empty() && t.readings()[0].lemma().is_some()),
        "malayalam.dict must tag at least one reading with a lemma"
    );
}

#[test]
fn module_checker_flags_demo_text() {
    let checker = module_checker();
    let text = "ഞാന്‍ അവളെ ഒരു പുസ്തകം നല്‍കി.";
    let result = checker.check(&CheckRequest::new(text.to_string(), Language::Malayalam));
    let flagged: Vec<(&str, &str)> = result
        .matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    eprintln!("ml module_check demo: {flagged:?}");
    assert!(
        flagged
            .iter()
            .any(|(id, word)| *id == "അവളെ" && *word == "അവളെ"),
        "the grammar.xml rule with the Malayalam id അവളെ must fire on the token അവളെ"
    );
}

#[test]
fn module_check_reports_repeat_and_unpaired_bracket() {
    let text = "കുടി കുടി (ഒരു വാക്ക്യം.";
    let matches = module_check(text);
    let ids: Vec<&str> = matches.iter().map(|m| m.rule().id()).collect();
    eprintln!("ml module_check natives: {ids:?}");
    assert!(
        ids.contains(&"WORD_REPEAT_RULE"),
        "the repeated കുടി must flag WORD_REPEAT_RULE"
    );
    assert!(
        ids.contains(&"UNPAIRED_BRACKETS"),
        "the unclosed '(' must flag UNPAIRED_BRACKETS"
    );
}
