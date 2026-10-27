use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;

pub(crate) struct LtPipeline {
    tokenizer: og_tokenizer::DefaultWordTokenizer,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn lt_pipeline() -> std::sync::Arc<LtPipeline> {
    static PIPE: og_core::releasable::Releasable<LtPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(LtPipeline {
            tokenizer: og_tokenizer::DefaultWordTokenizer::new_generic(),
            splitter: og_tokenizer::srx::srx_splitter("lt"),
        }))
    })
    .expect("lt pipeline")
}

impl LtPipeline {
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
    super::lt_rules_dir()
        .expect("lt rules dir")
        .join("grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("lt grammar.xml");
        let normalized = super::normalize_lt_rules_xml(&xml);
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&normalized, path.parent())
                .expect("compile lt grammar.xml"),
        ))
    })
    .expect("lt")
}

fn module_check(text: &str) -> Vec<RuleMatch> {
    let pipeline = lt_pipeline();
    let natives = super::lithuanian_native_rules();
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
fn lithuanian_grammar_examples_with_lt_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = lt_pipeline();
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
        "lt module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_lt_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 8,
        "all 8 live <example> elements of lt grammar.xml must be evaluated (the 14 raw <example> \
         lines include 6 inside the two commented-out rule drafts)"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all 8 lt examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        4,
        "lt grammar.xml compiles 4 pattern rules: BRAK_PRZECINKA_ZE, BRAK_PRZECINKA_JESLI and \
         the two KABLELIS_O_BET_TACIAU rulegroup sub-rules; the two commented-out drafts stay \
         comments"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 8,
        "the live example inventory of lt grammar.xml"
    );
}

#[test]
fn negate_tokens_compile_the_java_ascii_punct_class() {
    let rule_set = compiled_rules();
    for rule in &rule_set.rules {
        let first = &rule.pattern.tokens[0];
        assert!(
            first.negate && first.is_regexp,
            "every lt rule starts with the negated punctuation token"
        );
        let pattern = first
            .compiled_regexp
            .as_ref()
            .map(|re| re.as_str().to_string())
            .or_else(|| {
                first
                    .compiled_regexp_fancy
                    .as_ref()
                    .map(|re| re.as_str().to_string())
            })
            .unwrap_or_default();
        assert!(
            pattern.contains("[!-/:-@"),
            "the \\p{{Punct}} of lt grammar.xml must normalize to Java's ASCII punctuation class, \
             got {pattern}"
        );
    }
}

#[test]
fn srx_matches_the_java_sentence_tokenizer_test() {
    let splitter = og_tokenizer::srx::srx_splitter("lt").expect("lt SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let first = "Linux – laisvos operacinės sistemos branduolio (kernel) pavadinimas. ";
    let second = "Dažnai taip sutrumpintai vadinama ir bendrai visa Unix-tipo operacinė sistema \
naudojanti Linux branduolį kartu su sisteminėmis programomis bei bibliotekomis iš GNU projekto.";
    let sentences = splitter.split(&format!("{first}{second}"));
    let texts: Vec<&str> = sentences.iter().map(|s| s.text()).collect();
    assert_eq!(texts, vec![first, second]);
}

#[test]
fn module_check_reports_the_java_example_quartet() {
    let text = "Linux yra geresnė sistema tačiau dauguma lietuvių jos net nebandė.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![("KABLELIS_O_BET_TACIAU", "sistema tačiau")],
        "the KABLELIS_O_BET_TACIAU incorrect example flags with the 'sistema, tačiau' suggestion"
    );
    assert_eq!(
        matches[0].replacements()[0].value(),
        "sistema, tačiau",
        "the \\1, \\2 suggestion of grammar.xml"
    );
}

#[test]
fn module_check_flags_lowercase_start_and_shared_punctuation_natives() {
    let matches = module_check("linux yra geresnė sistema.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].rule().id(), "UPPERCASE_SENTENCE_START");
    assert_eq!(matches[0].offset(), 0);
    assert_eq!(matches[0].length(), "linux".len());

    let text = "Linux yra geras , bet ne.";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![("COMMA_PARENTHESIS_WHITESPACE", " ,")],
        "the comma-before-token whitespace native fires; the 'bet' sub-rule must stay quiet \
         because the token before 'bet' is punctuation"
    );

    let text = "Tai yra gerai..";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(flagged, vec![("DOUBLE_PUNCTUATION", "..")]);
}

#[test]
fn concurrency_sample_stays_clean_under_the_data_blocked_speller() {
    let text = "Vikipedijoje nėra straipsnio norimu pavadinimu.";
    let matches = module_check(text);
    assert!(
        matches.is_empty(),
        "the LithuanianConcurrencyTest sample must produce no matches; Java's own run dies on \
         the missing /lt/hunspell/lt_LT.dict, this port keeps the speller data-blocked, got {:?}",
        matches
            .iter()
            .map(|m| m.rule().id().to_string())
            .collect::<Vec<_>>()
    );
}
