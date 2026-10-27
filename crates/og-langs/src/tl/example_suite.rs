use og_core::checker::Tagger as _;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::rule::Rule as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckRequest, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct TlPipeline {
    tokenizer: super::TagalogWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn tl_pipeline() -> std::sync::Arc<TlPipeline> {
    static PIPE: og_core::releasable::Releasable<TlPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        Some(std::sync::Arc::new(TlPipeline {
            tokenizer: super::TagalogWordTokenizer::new(),
            tagger: super::tagalog_tagger().expect("tagalog.dict tagger"),
            splitter: og_tokenizer::srx::srx_splitter("tl"),
        }))
    })
    .expect("tl pipeline")
}

impl TlPipeline {
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

struct TlSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for TlSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("tl") {
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

struct TlWordTokenizerBridge;

impl og_core::checker::WordTokenizer for TlWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        super::TagalogWordTokenizer::new()
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
        let tagger = super::tagalog_tagger().expect("tagalog.dict tagger");
        let natives = super::tagalog_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = natives.rules.clone();
        for compiled in compiled_rules().rules.clone() {
            if compiled.default_on && !compiled.deprecated {
                rules.push(Arc::new(crate::engine::XmlPatternRule::new(compiled)));
            }
        }
        Some(std::sync::Arc::new(
            og_core::checker::Checker::new()
                .with_sentence_tokenizer(Arc::new(TlSentenceTokenizer))
                .with_word_tokenizer(Arc::new(TlWordTokenizerBridge))
                .with_tagger(tagger)
                .set_rules(rules)
                .set_text_level_rules(natives.text_level_rules.clone()),
        ))
    })
    .expect("tl checker")
}

fn grammar_path() -> std::path::PathBuf {
    super::tl_rules_dir()
        .expect("tl rules dir")
        .join("grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("tl grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile tl grammar.xml"),
        ))
    })
    .expect("tl grammar")
}

fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = tl_pipeline();
    let natives = super::tagalog_native_rules();
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
fn tagalog_grammar_examples_with_tl_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = tl_pipeline();

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
        "tl module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_tl_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 99,
        "all live <example> elements of tl grammar.xml must be evaluated"
    );
    assert_eq!(
        all_failures,
        Vec::<String>::new(),
        "grammar.xml:125 asks for <match no=\"1\" case_conversion=\"startupper\" \
         regexp_match=\"Nag(.)(.*)\" regexp_replace=\"$1um$2\"/>; the shared engine applies the \
         replace first and the case conversion last exactly like Java's MatchState.java:113-118 \
         and :287, so 'Nagbunot' offers 'Bumunot' and the module suite has no residue"
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        44,
        "tl grammar.xml compiles 44 pattern rules (one <rule> each, no <rulegroup>, no \
         commented-out drafts)"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 99,
        "the live example inventory of tl grammar.xml"
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
fn srx_resolves_the_java_generic_ruleset() {
    use og_tokenizer::SentenceTokenizer as _;
    let pipeline = tl_pipeline();
    let splitter = pipeline
        .splitter
        .as_ref()
        .expect("the (TL|tl).* -> Generic languagemap must resolve for tl");
    let text = "Ang Linux ay isang operating system kernel para sa mga operating system na humahalintulad sa Unix. Isa ang Linux sa mga pinaka-prominenteng halimbawa ng malayang software at pagsasagawa ng open source; madalas, malayang mapapalitan, gamitin, at maipamahagi ninuman ang lahat ng pinag-ugatang source code (pinagmulang kodigo).";
    let texts: Vec<String> = splitter
        .split(text)
        .into_iter()
        .map(|s| s.text().to_string())
        .collect();
    assert_eq!(
        texts,
        vec![
            "Ang Linux ay isang operating system kernel para sa mga operating system na humahalintulad sa Unix. ",
            "Isa ang Linux sa mga pinaka-prominenteng halimbawa ng malayang software at pagsasagawa ng open source; madalas, malayang mapapalitan, gamitin, at maipamahagi ninuman ang lahat ng pinag-ugatang source code (pinagmulang kodigo).",
        ],
        "TagalogSRXSentenceTokenizerTest.testTokenize: segment.srx maps (TL|tl).* to the Generic \
         ruleset, which breaks after '. ' but keeps '; ' and ', ' inside the second sentence"
    );
}

#[test]
fn tagger_dictionary_tags_tagalog_words() {
    let pipeline = tl_pipeline();
    let text = "Maganda si Maria.";
    let sentence = pipeline.analyze_range(text, 0, text.len());
    let readings: Vec<(&str, Vec<String>)> = sentence
        .tokens()
        .iter()
        .map(|t| {
            (
                t.token().token(),
                t.readings()
                    .iter()
                    .flat_map(|r| r.pos_tags().iter().cloned())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    eprintln!("tl tagger readings: {readings:?}");
    assert!(
        sentence.tokens()[1]
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("ADMO"))),
        "tagalog.dict must give 'Maganda' an ADMO reading (the ADJECTIVE-* rules depend on it)"
    );
    assert!(
        sentence.tokens()[5]
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("NPRO"))),
        "'Maria' carries NPRO (the PROPER_NOUN/SI_* rules depend on it)"
    );
    assert!(
        sentence.tokens()[6]
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("PSNP"))),
        "tagalog.dict tags the full stop PSNP (tagset.txt:206), which is what the \
         *_LAST_WORD_MISSING rules match on: {readings:?}"
    );
}

#[test]
fn module_check_flags_the_java_demo_text_with_the_pinned_rule_order() {
    let checker = module_checker();
    let text = "Ang LanguageTool ay maganda gamit sa araw-araw. Ang talatang ito ay nagpapakita ng ng kakayahan ng LanguageTool at hinahalimbawa kung paano ito gamitin. Litaw rin sa talatang ito na may mga bagaybagay na hindii pa kayang itama nng LanguageTool.";
    let result = checker.check(&CheckRequest::new(text.to_string(), Language::Tagalog));
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    eprintln!("tl demo text ids: {ids:?}");
    assert_eq!(
        ids,
        vec![
            "ADJECTIVE-V_COMMON_NOUN",
            "NG_NG",
            "MORFOLOGIK_RULE_TL",
            "R_WORDS",
            "MORFOLOGIK_RULE_TL",
            "MORFOLOGIK_RULE_TL",
            "MORFOLOGIK_RULE_TL",
        ],
        "TagalogTest.java:33-37 testDemoText pins exactly these seven matches in this order"
    );
}

#[test]
fn nag_ss_pins_the_shared_engine_case_conversion_order() {
    let pipeline = tl_pipeline();
    let rules = compiled_rules();
    let engine = PatternRuleEngine::new();
    let rule = rules
        .rules
        .iter()
        .find(|r| r.id == "NAG-_SS")
        .expect("NAG-_SS compiles");
    let text = "Nagbunot ng ngipin ang dentista.";
    let mut matches = Vec::new();
    for sentence in pipeline.analyze_text(text) {
        matches.extend(engine.match_rule(rule, &sentence));
    }
    let offered: Vec<&str> = matches[0]
        .replacements()
        .iter()
        .map(|r| r.value())
        .collect();
    assert_eq!(
        offered,
        vec!["Bumunot"],
        "Java offers 'Bumunot' here: MatchState.java:113-118 applies \
         regexp_match/regexp_replace FIRST and case_conversion='startupper' LAST - the shared \
         engine now follows that order in og-rules/src/pattern_rule.rs, so 'Nagbunot' -> \
         'bumunot' via regexp_replace=\"$1um$2\" then startupper lifts the first char"
    );
}

#[test]
fn module_check_reports_shared_punctuation_natives() {
    let text = "Kumain siya , ng mabuti..";
    let matches = module_check(text);
    let flagged: Vec<(&str, &str)> = matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    eprintln!("tl natives: {flagged:?}");
    assert!(
        flagged.contains(&("COMMA_PARENTHESIS_WHITESPACE", " ,")),
        "CommaWhitespaceRule is Tagalog.java:86"
    );
    assert!(
        flagged.contains(&("DOUBLE_PUNCTUATION", "..")),
        "DoublePunctuationRule is Tagalog.java:87"
    );
}

#[test]
fn speller_rule_pins_the_java_test_offsets() {
    let rule = super::tagalog_speller_rule();
    let pipeline = tl_pipeline();
    let clean = "Ang talatang ito ay nagpapakita ng ng kakayahan ng LanguageTool at halimbawa kung paano ito gamitin.";
    let sentence = pipeline.analyze_range(clean, 0, clean.len());
    assert!(
        rule.match_sentence(&sentence).is_empty(),
        "MorfologikTagalogSpellerRuleTest.java:40: every word of the correct sentence is known"
    );
    let typo = "Ang talatang ito ay nagpapakita ng ng kakayahan ng LanguageTool at hinahalimbawa kung paano ito gamitin.";
    let sentence = pipeline.analyze_range(typo, 0, typo.len());
    let matches = rule.match_sentence(&sentence);
    assert_eq!(
        matches.len(),
        1,
        "MorfologikTagalogSpellerRuleTest.java:42-43: exactly one word is flagged"
    );
    assert_eq!(matches[0].offset(), 67);
    assert_eq!(matches[0].offset() + matches[0].length(), 80);
    assert!(
        matches[0].replacements().is_empty(),
        "documented divergence: Java's first suggestion is the compound split 'hina halimbawa' \
         (MorfologikTagalogSpellerRuleTest.java:46) because MorfologikSpeller tries dictionary \
         word splits, while the shared Rust SuggestionEngine only proposes single dictionary \
         words within edit distance 3 of 'hinahalimbawa' and none qualifies"
    );
}
