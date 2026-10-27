use crate::ja::word_tokenizer::JapaneseWordTokenizer;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;

pub(crate) struct JaPipeline {
    tokenizer: JapaneseWordTokenizer,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ja_pipeline() -> std::sync::Arc<JaPipeline> {
    static PIPE: og_core::releasable::Releasable<JaPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(JaPipeline {
            tokenizer: JapaneseWordTokenizer::new(),
            splitter: og_tokenizer::srx::srx_splitter("ja"),
        }))
    })
    .expect("ja pipeline")
}

impl JaPipeline {
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
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, start, end);
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(self.tokenizer.tokenize_readings(sentence_text, start));
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
        .join("ja/src/main/resources/org/languagetool/rules/ja/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ja grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ja grammar.xml"),
        ))
    })
    .expect("ja")
}

pub(crate) fn module_check(text: &str) -> Vec<RuleMatch> {
    let pipeline = ja_pipeline();
    let natives = crate::ja::japanese_native_rules();
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
fn japanese_grammar_examples_with_ja_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = ja_pipeline();

    let mut passed = 0usize;
    let mut correction_failures = Vec::new();
    let mut fail_lines: Vec<String> = Vec::new();
    let mut exact_positions = 0usize;
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
            if !failed
                && let Some((m_start, m_end)) = marker_range
                && let Some(first) = matches.first()
                && first.offset() == m_start
                && first.offset() + first.length() == m_end
            {
                exact_positions += 1;
            }
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
        "ja module pipeline: {}/{} passed (no_match/marker {}, correction {}, position-exact {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len(),
        exact_positions
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_ja_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 735,
        "all 735 <example> elements of ja grammar.xml must be evaluated"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all 735 ja examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        735,
        "ja grammar.xml compiles 735 pattern rules (735 standalone <rule> elements, no rulegroups)"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(example_count, 735, "one live <example> per rule");
}

#[test]
fn srx_languagemap_resolves_the_ideographic_cascade() {
    let splitter =
        og_tokenizer::srx::srx_splitter("ja").expect("the (JA|ja).* languagemap must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentence_texts = |text: &str| -> Vec<String> {
        splitter
            .split(text)
            .iter()
            .map(|s| s.text().to_string())
            .collect()
    };
    assert_eq!(
        sentence_texts("これはテスト用の文です。"),
        vec!["これはテスト用の文です。"]
    );
    assert_eq!(
        sentence_texts("これはテスト用の文です。追加のテスト用の文です。"),
        vec!["これはテスト用の文です。", "追加のテスト用の文です。"],
        "JapaneseSRXSentenceTokenizerTest.testTokenize second case"
    );
    assert_eq!(
        sentence_texts("これは、テスト用の文です。"),
        vec!["これは、テスト用の文です。"]
    );
    assert_eq!(
        sentence_texts("テスト用の文です！追加のテスト用の文です。"),
        vec!["テスト用の文です！", "追加のテスト用の文です。"]
    );
    assert_eq!(
        sentence_texts("テスト用の文です... 追加のテスト用の文です。"),
        vec!["テスト用の文です... ", "追加のテスト用の文です。"]
    );
    assert_eq!(
        sentence_texts("アドレスはhttp://www.test.deです。"),
        vec!["アドレスはhttp://www.test.deです。"],
        "no sentence break inside the URL"
    );
    for text in [
        "これは(!)の文です。",
        "これは(!!)の文です。",
        "これは(?)の文です。",
        "これは(??)の文です。",
    ] {
        assert_eq!(sentence_texts(text), vec![text.to_string()]);
    }
}

#[test]
fn jlanguage_tool_test_parity() {
    assert!(
        module_check("エラーを含まないテスト文です。").is_empty(),
        "JLanguageToolTest.testJapanese first assertion: 0 matches"
    );
    let matches = module_check("エラーお含むテスト文です。");
    assert_eq!(
        matches.len(),
        1,
        "JLanguageToolTest.testJapanese second assertion: exactly 1 match (rule O on \
         [名詞.*][お]), got {:?}",
        matches.iter().map(|m| m.rule().id()).collect::<Vec<_>>()
    );
}

#[test]
fn module_check_fires_the_sent_end_and_literal_rules() {
    let matches = module_check("したがって。");
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), text_of(m, "したがって。")))
        .collect();
    assert_eq!(
        flagged,
        vec![("SITAGATTE_END", "したがって。")],
        "SITAGATTE_END = [したがって][SENT_END]: the pipeline adds SENT_END to the trailing 。"
    );

    let text = "これわ文章を入力して'Check Text'をクリックすると、誤記を探すことができる。";
    let matches = module_check(text);
    let ids: Vec<&str> = matches.iter().map(|m| m.rule().id()).collect();
    assert!(
        ids.contains(&"KOREWA") && ids.contains(&"DOUSI_KOTOGADEKIRU"),
        "the JapaneseTest demo text first sentence fires KOREWA on これわ and \
         DOUSI_KOTOGADEKIRU on ことができる, got {ids:?}"
    );
}

fn text_of<'a>(m: &RuleMatch, text: &'a str) -> &'a str {
    &text[m.offset()..m.offset() + m.length()]
}

#[test]
fn module_check_covers_markers_on_postag_rules() {
    let matches = module_check("詳細の表示する");
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), text_of(m, "詳細の表示する")))
        .collect();
    assert_eq!(
        flagged,
        vec![("NO_SURU", "の")],
        "NO_SURU keeps its <marker> inside the pattern (pattern-level marker), so the match \
         covers only の"
    );

    let matches = module_check("暮らすことができる");
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), text_of(m, "暮らすことができる")))
        .collect();
    assert_eq!(flagged, vec![("DOUSI_KOTOGADEKIRU", "ことができる")]);
}

#[test]
fn natives_fire_on_double_punctuation_and_repeated_whitespace() {
    let matches = module_check("これは..テストです。");
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), text_of(m, "これは..テストです。")))
        .collect();
    assert_eq!(flagged, vec![("DOUBLE_PUNCTUATION", "..")]);

    assert!(
        module_check("これは、、テストです。").is_empty(),
        "DoublePunctuationRule.java:70-74 compares against '.' and getCommaCharacter() ',' \
         only, so the ideographic comma 、 never triggers it"
    );

    let matches = module_check("これは  テストです。");
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), text_of(m, "これは  テストです。")))
        .collect();
    assert_eq!(flagged, vec![("WHITESPACE_RULE", "  ")]);
}
