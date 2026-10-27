use og_core::checker::Disambiguator as _;
use og_core::checker::Tagger as _;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter as _, clean_overlapping_filter,
};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckRequest, Language};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct KmPipeline {
    tokenizer: super::KhmerWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn km_resource_dir() -> std::path::PathBuf {
    super::km_resource_dir().expect("km resource dir")
}

pub(crate) fn km_pipeline() -> std::sync::Arc<KmPipeline> {
    static PIPE: og_core::releasable::Releasable<KmPipeline> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&PIPE, || {
        let resource_dir = km_resource_dir();
        Some(std::sync::Arc::new(KmPipeline {
            tokenizer: super::KhmerWordTokenizer::new(),
            tagger: super::khmer_tagger().expect("khmer.dict tagger"),
            disambiguator: super::build_km_disambiguator(&resource_dir),
            splitter: og_tokenizer::srx::srx_splitter("km"),
        }))
    })
    .expect("km")
}

impl KmPipeline {
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

struct KmSentenceTokenizer;

impl og_core::checker::SentenceTokenizer for KmSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<og_core::checker::SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        match og_tokenizer::srx::srx_splitter("km") {
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

struct KmWordTokenizerBridge;

impl og_core::checker::WordTokenizer for KmWordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        super::KhmerWordTokenizer::new()
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
        let tagger = super::khmer_tagger().expect("khmer.dict tagger");
        let natives = super::khmer_native_rules();
        let mut rules: Vec<Arc<dyn og_core::rule::Rule>> = natives.rules.clone();
        for compiled in compiled_rules().rules.clone() {
            if compiled.default_on && !compiled.deprecated {
                rules.push(Arc::new(crate::engine::XmlPatternRule::new(compiled)));
            }
        }
        let mut checker = og_core::checker::Checker::new()
            .with_sentence_tokenizer(Arc::new(KmSentenceTokenizer))
            .with_word_tokenizer(Arc::new(KmWordTokenizerBridge))
            .with_tagger(tagger)
            .set_rules(rules)
            .set_text_level_rules(natives.text_level_rules.clone());
        if let Some(resource_dir) = super::km_resource_dir()
            && let Some(disambiguator) = super::build_km_disambiguator(&resource_dir)
        {
            checker = checker.with_disambiguator(Arc::new(disambiguator));
        }
        Some(std::sync::Arc::new(checker))
    })
    .expect("km")
}

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("km/src/main/resources/org/languagetool/rules/km/grammar.xml")
}

fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("km grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile km grammar.xml"),
        ))
    })
    .expect("km")
}

fn module_check(text: &str) -> Vec<og_core::RuleMatch> {
    let pipeline = km_pipeline();
    let natives = super::khmer_native_rules();
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
    pipeline: &KmPipeline,
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
fn khmer_grammar_examples_with_km_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = km_pipeline();
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
        "km module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines.clone();
    all_failures.extend(correction_failures.clone());
    let _ = std::fs::write(
        "/tmp/test_failures_km_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(
        total, 66,
        "all 66 <example> elements of km grammar.xml must be evaluated"
    );
    assert!(
        all_failures.is_empty(),
        "expected Java parity on all 66 km examples, got {} failures: {all_failures:?}",
        all_failures.len()
    );
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        33,
        "km grammar.xml compiles 33 pattern rules: 31 standalone <rule id=...> elements plus the \
         two Classifier_PEOPLE rulegroup sub-rules"
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(
        example_count, 66,
        "the live example inventory of km grammar.xml"
    );
}

#[test]
fn srx_languagemap_resolves_the_generalimportant_cascade() {
    let splitter =
        og_tokenizer::srx::srx_splitter("km").expect("the .* languagemap must resolve for km");
    use og_tokenizer::SentenceTokenizer as _;
    let sentence_count = |text: &str| splitter.split(text).len();
    assert_eq!(sentence_count("ឃ្លា\u{200B}មួយ។ ឃ្លា\u{200B}ពីរ។"), 1);
    assert_eq!(
        sentence_count("ឃ្លា\u{200B}មួយ។ ឃ្លា"),
        1,
        "km has no dedicated SRX block, so only the .* -> GeneralImportant cascade applies and \
         the Khmer sign khan does not break sentences"
    );
}

#[test]
fn khmer_tagger_and_disambiguator_load() {
    let resource_dir = km_resource_dir();
    assert!(
        super::build_km_disambiguator(&resource_dir).is_some(),
        "disambiguation.xml (29 rules, no DOCTYPE entities) must parse"
    );
    let pipeline = km_pipeline();
    let sentence = pipeline.analyze_range("ការ\u{200B}ពិនិត្យ", 0, "ការ\u{200B}ពិនិត្យ".len());
    assert!(
        sentence
            .tokens()
            .iter()
            .any(|t| !t.readings().is_empty() && t.readings()[0].lemma().is_some()),
        "khmer.dict must tag at least one reading with a lemma"
    );
}

#[test]
fn module_checker_runs_the_demo_text() {
    let checker = module_checker();
    let text = "ឃ្លា\u{200B}នេះ\u{200B}បង្ហាញ\u{200B}ពី\u{200B}ពី\u{200B}កំហុស\u{200B}វេយ្យាករណ៍ ដើម្បី\u{200B}បញ្ជាក់\u{200B}ពី\u{200B}ប្រសិទ្ធភាព\u{200B}របស់\u{200B}កម្មវិធី LanguageTool សំរាប់\u{200B}ភាសាខ្មែរ។";
    let result = checker.check(&CheckRequest::new(text.to_string(), Language::Khmer));
    let flagged: Vec<(&str, &str)> = result
        .matches
        .iter()
        .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
        .collect();
    assert_eq!(
        flagged,
        vec![
            ("KM_WORD_REPEAT_RULE", "ពី\u{200B}ពី"),
            ("HUNSPELL_RULE", "LanguageTool"),
            ("KM_SIMPLE_REPLACE", "សំរាប់"),
            ("HUNSPELL_RULE", "ភាសាខ្មែរ"),
        ],
        "KhmerTest.testDemoText expects [KM_WORD_REPEAT_RULE, HUNSPELL_RULE x5, KM_SIMPLE_REPLACE] \
         over the real km_KH.dic hunspell; this port's module speller is the khmer.dict wordlist \
         (the engine convention for every hunspell language here, cf. gl/da/sv), so it misses \
         Latin 'LanguageTool' and the compound ភាសាខ្មែរ where Java's hunspell misses five \
         other words"
    );
}

#[test]
fn module_check_reports_unpaired_brackets_and_space_before() {
    let text = "គាត់\u{200B}បាន\u{200B}ទៅ\u{200B}ដើម្បី\u{200B}ទិញ\u{200B}ម្ហូប («\u{200B}ខ្ញុំ។";
    let matches = module_check(text);
    let flagged: Vec<(&str, usize)> = matches
        .iter()
        .map(|m| (m.rule().id(), m.offset()))
        .collect();
    eprintln!("km module_check: {flagged:?}");
    assert!(
        flagged
            .iter()
            .any(|(id, _)| *id == "KM_SPACE_BEFORE_CONJUNCTION"),
        "KhmerSpaceBeforeRule fires on the glued ដើម្បី"
    );
    assert!(
        flagged.iter().any(|(id, _)| *id == "KM_UNPAIRED_BRACKETS"),
        "the unclosed « flags"
    );
}
