use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;

pub(crate) struct IsPipeline {
    tokenizer: og_tokenizer::DefaultWordTokenizer,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn is_pipeline() -> std::sync::Arc<IsPipeline> {
    static PIPE: og_core::releasable::Releasable<IsPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(IsPipeline {
            tokenizer: og_tokenizer::DefaultWordTokenizer::new_generic(),
            splitter: og_tokenizer::srx::srx_splitter("is"),
        }))
    })
    .expect("is pipeline")
}

impl IsPipeline {
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
        let tokens = sentence.tokens_mut();
        for token in tokens.iter_mut().rev() {
            if !token.is_whitespace() && !token.has_pos_tag("SENT_START") {
                if !token.has_pos_tag("SENT_END") {
                    token.add_sent_end();
                }
                break;
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

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("is/src/main/resources/org/languagetool/rules/is/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("is grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile is grammar.xml"),
        ))
    })
    .expect("is")
}

fn module_check(text: &str) -> Vec<RuleMatch> {
    let pipeline = is_pipeline();
    let natives = crate::is::icelandic_native_rules();
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
fn icelandic_grammar_examples_with_is_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = is_pipeline();
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
        "is module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_is_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 45,
        "all 45 <example> elements of is grammar.xml must be evaluated"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all 45 is examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        39,
        "is grammar.xml compiles 39 pattern rules: 31 standalone <rule id=...> elements plus the \
         4 FRAMVINDUHORF, 2 LEYTI and 2 LANGREYÐUR rulegroup sub-rules (35 attribute-bearing \
         <rule ...> plus 4 bare <rule> elements)"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 45,
        "the live example inventory of is grammar.xml"
    );
}

#[test]
fn srx_languagemap_resolves_the_icelandic_cascade() {
    let splitter =
        og_tokenizer::srx::srx_splitter("is").expect("the (IS|is).* languagemap must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentence_count = |text: &str| splitter.split(text).len();
    assert_eq!(
        sentence_count("Þetta er setning. Þetta er önnur setning."),
        2
    );
    assert_eq!(sentence_count("Er þetta setning? Já, það er."), 2);
    assert_eq!(
        sentence_count("Sjá bls. 10 fyrir frekari upplýsingar."),
        1,
        "bls. is an Icelandic no-break abbreviation (segment.srx Icelandic block)"
    );
    assert_eq!(
        sentence_count("Þetta er tölfræðileg athugun í 1. janúar 2010."),
        1,
        "a digit dot followed by a month name is a no-break date rule"
    );
}

#[test]
fn plain_word_tokenizer_splits_icelandic_punctuation() {
    use og_tokenizer::WordTokenizer as _;
    let tokenizer = og_tokenizer::DefaultWordTokenizer::new_generic();
    let tokens: Vec<String> = tokenizer
        .tokenize("Þetta er setning, og hún er góð.")
        .into_iter()
        .map(|t| t.text().to_string())
        .collect();
    let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
    assert_eq!(
        tokens,
        vec![
            "Þetta", " ", "er", " ", "setning", ",", " ", "og", " ", "hún", " ", "er", " ", "góð",
            "."
        ],
        "the plain Java WordTokenizer (DefaultWordTokenizer::new_generic) keeps whitespace \
         tokens and splits , and . as single tokens"
    );
}

#[test]
fn module_check_reports_the_java_example_trio() {
    let text = "Ég sá annað hvort Jón eða Guðmund.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![("ANNAÐHVORT_EÐA", "annað hvort")],
        "the ANNAÐHVORT_EÐA example: annað + skip-2 hvort ahead of eða flags the pair, \
         suggesting the compound annaðhvort (grammar.xml)"
    );
}

#[test]
fn module_check_flags_the_repeated_word_and_uppercase_start() {
    let text = "þetta er er setning.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![
            ("UPPERCASE_SENTENCE_START", "þetta"),
            ("WORD_REPEAT_RULE", "er"),
        ],
        "Icelandic.java:79-80 registers UppercaseSentenceStartRule and WordRepeatRule; both fire \
         on a lowercase sentence start with a repeated word"
    );
}

#[test]
fn module_check_surfaces_the_shared_punctuation_natives() {
    let text = "Þetta er setning , með tvö.. punkta.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![
            ("COMMA_PARENTHESIS_WHITESPACE", " ,"),
            ("DOUBLE_PUNCTUATION", ".."),
        ],
        "Icelandic.java:75-76 registers CommaWhitespaceRule and DoublePunctuationRule; both fire \
         on a comma preceded by a space and a doubled dot"
    );
}
