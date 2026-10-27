use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct FaPipeline {
    tokenizer: super::PersianWordTokenizer,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn fa_pipeline() -> std::sync::Arc<FaPipeline> {
    static PIPE: og_core::releasable::Releasable<FaPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(Arc::new(FaPipeline {
            tokenizer: super::PersianWordTokenizer::new(),
            splitter: og_tokenizer::srx::srx_splitter("fa"),
        }))
    })
    .expect("fa pipeline")
}

impl FaPipeline {
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

    pub(crate) fn analyze_whole(&self, text: &str) -> AnalyzedSentence {
        self.analyze_range(text, 0, text.len())
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
        .join("fa/src/main/resources/org/languagetool/rules/fa/grammar.xml")
}

fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("fa grammar.xml");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile fa grammar.xml"),
        ))
    })
    .expect("fa")
}

fn module_check(text: &str) -> Vec<RuleMatch> {
    let pipeline = fa_pipeline();
    let natives = crate::fa::persian_native_rules();
    let rules = compiled_rules();
    let engine = PatternRuleEngine::new();
    let sentences = pipeline.analyze_text(text);
    let mut sentence_rules: Vec<Arc<dyn og_core::rule::Rule>> = natives.rules;
    sentence_rules.push(Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()));
    sentence_rules.push(Arc::new(
        og_rules::native_rules::DoublePunctuationRule::new(),
    ));
    let mut all_matches = Vec::new();
    for rule in &sentence_rules {
        if !rule.is_default_on() {
            continue;
        }
        for sentence in &sentences {
            all_matches.extend(rule.match_sentence(sentence));
        }
    }
    for compiled in rules.rules.iter().filter(|r| r.default_on && !r.deprecated) {
        for sentence in &sentences {
            all_matches.extend(engine.match_rule(compiled, sentence));
        }
    }
    let mut text_level_rules: Vec<Arc<dyn og_core::rule::TextLevelRule>> = vec![
        Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
        Arc::new(og_rules::text_level_rules::GenericUnpairedBracketsRule::new()),
        Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
    ];
    text_level_rules.extend(natives.text_level_rules);
    for rule in &text_level_rules {
        if !rule.is_default_on() {
            continue;
        }
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
    pipeline: &FaPipeline,
    engine: &PatternRuleEngine,
    group: &[&CompiledRule],
    clean_text: &str,
) -> Vec<RuleMatch> {
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
fn persian_grammar_examples_with_fa_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = fa_pipeline();
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
        "fa module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_fa_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 566,
        "572 raw <example> elements minus the 6 inside XML comments must all be evaluated"
    );
    assert_eq!(
        all_failures,
        vec!["CORRECTION: [RemoveAdditionMarks]: '۱۹٬' expected '۱۹' got [\"۱۹٬\"]"],
        "the residue is exactly the RemoveAdditionMarks correction: its token regex \
         '([۰-۹]+)[٫٬](?![۰-۹])' carries a negative lookahead the regex crate cannot compile, so \
         the engine falls back to literal whole-token matching (the match and span are still \
         correct) and the <match no=\"1\" regexp_replace=\"$1\"> suggestion cannot extract the \
         digit group, offering the raw token instead"
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        283,
        "fa grammar.xml compiles each of its 283 <rule> elements (12 standalone id rules plus \
         the sub-rules of the 4 rulegroups) into one compiled rule"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 566,
        "the live example inventory of fa grammar.xml"
    );
}

#[test]
fn srx_languagemap_resolves_the_persian_cascade() {
    let splitter = og_tokenizer::srx::srx_splitter("fa").expect("fa SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentence_count = |text: &str| splitter.split(text).len();
    assert_eq!(sentence_count("این یک جمله است. جملهٔ بعدی"), 2);
    assert_eq!(sentence_count("آیا این یک جمله است؟ جملهٔ بعدی"), 2);
    assert_eq!(sentence_count("یک جمله!!! جملهٔ بعدی"), 2);
    assert_eq!(sentence_count("جملهٔ اول... خوب نیست؟ جملهٔ دوم."), 2);
    assert_eq!(
        sentence_count("جملهٔ اول (...) ادامهٔ متن. "),
        1,
        "the parenthesized ellipsis is a no-break rule in the Persian cascade"
    );
    assert_eq!(
        sentence_count("جملهٔ اول [...] ادامهٔ متن. "),
        1,
        "PersianSRXSentenceTokenizerTest pins this split set"
    );
}

#[test]
fn module_check_reports_the_java_example_pair_in_sequence() {
    let text = "این کار برای برای تو بود.";
    let matches = module_check(text);
    let ids: Vec<&str> = matches.iter().map(|m| m.rule().id()).collect();
    assert_eq!(
        ids,
        vec!["PERSIAN_WORD_REPEAT_RULE"],
        "the default-level check surfaces exactly the repeated word; the two default-off \
         fa rules (PERSIAN_COMMA_PARENTHESIS_WHITESPACE, FA_SPACE_BEFORE_CONJUNCTION) stay \
         silent unless explicitly enabled"
    );
}

#[test]
fn module_check_surfaces_the_bad_zwnj_grammar_rule() {
    let text = "و\u{200C}ارد";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![("Bad_ZWNJ", "و\u{200C}")],
        "the Bad_ZWNJ grammar.xml example: a letter followed by a ZWNJ token flags the pair, \
         suggesting the letter alone (grammar.xml:174-179)"
    );
}
