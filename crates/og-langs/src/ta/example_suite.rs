use og_core::checker::Tagger as _;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckRequest, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct TaPipeline {
    tokenizer: super::TamilWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ta_pipeline() -> std::sync::Arc<TaPipeline> {
    static PIPE: og_core::releasable::Releasable<TaPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(TaPipeline {
            tokenizer: super::TamilWordTokenizer::new(),
            tagger: super::tamil_tagger().expect("tamil.dict tagger"),
            splitter: og_tokenizer::srx::srx_splitter("ta"),
        }))
    })
    .expect("ta pipeline")
}

impl TaPipeline {
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

struct TaSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for TaSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("ta") {
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

struct TaWordTokenizerBridge;

impl og_core::checker::WordTokenizer for TaWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        super::TamilWordTokenizer::new()
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
        let tagger = super::tamil_tagger().expect("tamil.dict tagger");
        let natives = super::tamil_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = natives.rules.clone();
        for compiled in compiled_rules().rules.clone() {
            if compiled.default_on && !compiled.deprecated {
                rules.push(Arc::new(crate::engine::XmlPatternRule::new(compiled)));
            }
        }
        Some(std::sync::Arc::new(
            og_core::checker::Checker::new()
                .with_sentence_tokenizer(Arc::new(TaSentenceTokenizer))
                .with_word_tokenizer(Arc::new(TaWordTokenizerBridge))
                .with_tagger(tagger)
                .set_rules(rules)
                .set_text_level_rules(natives.text_level_rules.clone()),
        ))
    })
    .expect("ta checker")
}

fn grammar_path() -> std::path::PathBuf {
    super::ta_rules_dir()
        .expect("ta rules dir")
        .join("grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ta grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ta grammar.xml"),
        ))
    })
    .expect("ta grammar")
}

fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = ta_pipeline();
    let natives = super::tamil_native_rules();
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

#[test]
#[ignore]
fn tamil_grammar_examples_with_ta_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = ta_pipeline();

    let mut passed = 0usize;
    let mut correction_failures = Vec::new();
    let mut fail_lines: Vec<String> = Vec::new();
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group {
                for sentence in pipeline.analyze_text(&clean_text) {
                    matches.extend(engine.match_rule(aux, &sentence));
                }
            }
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
        "ta module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_ta_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 216,
        "all live <example> elements of ta grammar.xml must be evaluated (the file carries 234 \
         raw ones, 18 sit inside commented-out rules)"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all live ta examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        210,
        "ta grammar.xml compiles 210 pattern rules: 42 <rulegroup>/<rule> ids with their \
         sub-rules; the commented-out drafts (2 rules of ena, Ends_in_A_6, 2 rules of oru and \
         the rest) stay comments"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 216,
        "the live example inventory of ta grammar.xml"
    );
    let keys: Vec<(String, Option<String>)> = rule_set
        .rules
        .iter()
        .map(|r| (r.id.clone(), r.sub_id.clone()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        keys.len(),
        sorted.len(),
        "compiled rule (id, sub_id) keys must stay unique (the ja 'no duplicate ids' invariant)"
    );
}

#[test]
fn srx_resolves_the_java_tamil_ruleset() {
    use og_tokenizer::SentenceTokenizer as _;
    let pipeline = ta_pipeline();
    let splitter = pipeline
        .splitter
        .as_ref()
        .expect("the (TA|ta).* -> Tamil languagemap must resolve for ta");
    let text = "எ.கா. அவன் வந்தான். அவள் சென்றாள்.";
    let texts: Vec<String> = splitter
        .split(text)
        .into_iter()
        .map(|s| s.text().to_string())
        .collect();
    assert_eq!(
        texts,
        vec!["எ.கா. அவன் வந்தான். ", "அவள் சென்றாள்."],
        "segment.srx keeps 'எ.கா. ' unbroken (the first Tamil no-break rule) and breaks on the \
         plain '. ' of the Tamil break rule"
    );
    let month = "மார். 2020ல் வந்தான்.";
    let texts: Vec<String> = splitter
        .split(month)
        .into_iter()
        .map(|s| s.text().to_string())
        .collect();
    assert_eq!(
        texts,
        vec![month.to_string()],
        "the '(ஜன|பிப்|மார்|...)|\\.\\s' no-break rule with afterbreak \\p{{N}} keeps 'மார். 2020' \
         in one sentence"
    );
}

#[test]
fn tagger_dictionary_tags_tamil_words() {
    let pipeline = ta_pipeline();
    let text = "இது பற்றி";
    let sentence = pipeline.analyze_range(text, 0, text.len());
    let readings: Vec<Vec<String>> = sentence
        .tokens()
        .iter()
        .map(|t| {
            t.readings()
                .iter()
                .flat_map(|r| r.pos_tags().iter().cloned())
                .collect::<Vec<_>>()
        })
        .collect();
    eprintln!("ta tagger readings: {readings:?}");
    assert!(
        !sentence.tokens()[1].readings()[0].pos_tags().is_empty(),
        "tamil.dict must give 'இது' at least one POS reading"
    );
}

#[test]
fn module_check_flags_the_java_demo_text_with_the_pinned_rule_order() {
    let checker = module_checker();
    let text = "இந்த பெட்டியில் உங்கள் உரையை ஒட்டி சரிவர சோதிக்கிறதா என பாருங்கள். 'லேங்குவேஜ் டூல்' சில இலக்கணப் பிழைகளைச் சரியாக கண்டுபிடிக்கும். பல பிழைகளைப் பிடிக்க தடுமாறலாம்.";
    let result = checker.check(&CheckRequest::new(text.to_string(), Language::Tamil));
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    eprintln!("ta demo text ids: {ids:?}");
    assert_eq!(
        ids,
        vec![
            "Ends_in_A_1",
            "Ends_in_A_3",
            "Ends_in_A_3",
            "Ends_in_A_4",
            "Ends_in_A_2",
        ],
        "TamilTest.java:35-37 testDemoText pins exactly these five matches in this order"
    );
}

#[test]
fn module_check_reports_shared_punctuation_natives() {
    let text = "அவன் வந்தான் , சென்றான்..";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    eprintln!("ta natives: {flagged:?}");
    assert_eq!(
        flagged,
        vec![
            ("COMMA_PARENTHESIS_WHITESPACE", " ,"),
            ("DOUBLE_PUNCTUATION", ".."),
        ],
        "the two sentence-level natives of Tamil.java fire, and the case-less Tamil text stays \
         free of UppercaseSentenceStart (not in getRelevantRules)"
    );
}
