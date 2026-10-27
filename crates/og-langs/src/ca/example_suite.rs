use og_core::rule::{Rule as _, TextLevelRule as _};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) struct CatalanPipeline {
    tokenizer: super::word_tokenizer::CatalanWordTokenizer,
    tagger: Arc<super::tagger::CatalanTagger>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

pub(crate) fn ca_rules_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ca/src/main/resources/org/languagetool/rules/ca"))
}

impl CatalanPipeline {
    pub fn shared() -> std::sync::Arc<CatalanPipeline> {
        static PIPE: og_core::releasable::Releasable<CatalanPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = super::tagger::ca_resource_dir().expect("ca resource dir");
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok())
                .and_then(|xml| {
                    crate::engine::find_lt_core_resource_dir().map(|core| {
                        og_tagger::hybrid_disambiguator("ca", &resource_dir, &core, xml)
                    })
                });
            Some(std::sync::Arc::new(CatalanPipeline {
                tokenizer: super::word_tokenizer::CatalanWordTokenizer::new(),
                tagger: super::tagger::CatalanTagger::shared_arc()
                    .expect("ca tagger over added.txt"),
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("ca"),
            }))
        })
        .expect("ca pipeline")
    }

    pub fn sentence_ranges(&self, text: &str) -> Vec<(usize, usize)> {
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

    pub fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(
            self.tokenizer.tokenize(sentence_text).into_iter().map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
            }),
        );
        sentence.set_tokens(tokens);
        let token_texts: Vec<String> = sentence
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token().to_string())
            .collect();
        let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
        let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &refs);
        let tokens_mut = sentence.tokens_mut();
        for (i, atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens_mut.len() {
                continue;
            }
            let readings = atr.into_readings();
            if !readings.is_empty() {
                let primary = tokens_mut[token_idx].token().clone();
                let updated_primary =
                    AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                        .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                tokens_mut[token_idx] =
                    AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
            }
        }
        for token in sentence.tokens_mut().iter_mut().rev() {
            if !token.is_whitespace()
                && !token.token().pos_tags().contains(&"SENT_START".to_string())
            {
                if !token.has_pos_tag("SENT_END") {
                    token.add_sent_end();
                }
                break;
            }
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        super::gv_chunker::add_gv_chunks(&mut sentence);
        sentence
    }

    pub fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        self.sentence_ranges(text)
            .into_iter()
            .map(|(s, e)| (self.analyze_range(text, s, e), s))
            .collect()
    }
}

fn grammar_path() -> std::path::PathBuf {
    ca_rules_dir().expect("ca rules dir").join("grammar.xml")
}

fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("ca grammar.xml");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ca grammar.xml"),
        ))
    })
    .expect("ca")
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

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
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

fn rule_needs_tags(rule: &CompiledRule) -> bool {
    super::trace::rule_needs_readings(rule)
}

type ExampleOutcome = (String, String, Option<(Outcome, String)>, bool);

type GroupExamplePair<'a> = (
    &'a Vec<&'a og_xml::compiler::CompiledRule>,
    &'a og_xml::types::XmlExample,
);

#[test]
#[ignore]
fn catalan_grammar_examples_with_ca_pipeline() {
    use rayon::prelude::*;
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = CatalanPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let started = std::time::Instant::now();

    let pairs: Vec<GroupExamplePair> = groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    let per_example: Vec<ExampleOutcome> = pairs
        .par_iter()
        .map(|(group, example)| {
            let rule = group[0];
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            // Java tests ca incorrect examples unsplit (PatternRuleTest.java:617-620)
            let unsplit_incorrect =
                example.example_type == og_xml::types::XmlExampleType::Incorrect;
            for aux in group.iter() {
                if unsplit_incorrect {
                    let sentence = pipeline.analyze_range(&clean_text, 0, clean_text.len());
                    matches.extend(engine.match_rule(aux, &sentence));
                } else {
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                        for mut m in engine.match_rule(aux, &sentence) {
                            m.set_offset_length(m.offset() + sentence_start, m.length());
                            matches.push(m);
                        }
                    }
                }
            }
            let failure: Option<(Outcome, String)> = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some((
                            Outcome::NoMatch,
                            "Expected match on incorrect example but got none".to_string(),
                        ))
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_start = first.offset();
                        let match_end = match_start + first.length();
                        if match_start <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some((
                                Outcome::Marker,
                                format!(
                                    "Match {match_start}..{match_end} does not contain marker {m_start}..{m_end}",
                                ),
                            ))
                        }
                    } else {
                        None
                    }
                }
                og_xml::types::XmlExampleType::Correct => {
                    if matches.is_empty() {
                        None
                    } else {
                        Some((
                            Outcome::FalsePositive,
                            format!(
                                "Expected no match on correct example but got {}",
                                matches.len()
                            ),
                        ))
                    }
                }
                og_xml::types::XmlExampleType::TriggersError => None,
            };
            (rule.id.clone(), clean_text, failure, rule_needs_tags(rule))
        })
        .collect();

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut tag_starved: usize = 0usize;
    let mut engine_side: usize = 0usize;
    let mut fail_lines: Vec<String> = Vec::new();
    let mut fail_by_rule: HashMap<String, usize> = HashMap::new();
    let mut evaluated = 0usize;

    for (rule, clean_text, failure, needs_tags) in per_example {
        evaluated += 1;
        if evaluated.is_multiple_of(2000) {
            eprintln!(
                "ca suite progress: {evaluated} examples, {:.1}s",
                started.elapsed().as_secs_f64()
            );
        }
        match failure {
            None => passed += 1,
            Some((outcome, message)) => {
                failed += 1;
                *outcome_counts.entry(outcome).or_default() += 1;
                if needs_tags {
                    tag_starved += 1;
                } else {
                    engine_side += 1;
                }
                fail_lines.push(format!("FAIL: [{}] {}: '{}'", rule, message, clean_text));
                *fail_by_rule.entry(rule).or_default() += 1;
            }
        }
    }

    eprintln!(
        "ca module pipeline: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
        passed,
        passed + failed,
        100.0 * passed as f64 / (passed + failed).max(1) as f64,
        outcome_counts.get(&Outcome::NoMatch).copied().unwrap_or(0),
        outcome_counts
            .get(&Outcome::FalsePositive)
            .copied()
            .unwrap_or(0),
        outcome_counts.get(&Outcome::Marker).copied().unwrap_or(0),
        started.elapsed().as_secs_f64(),
    );
    eprintln!(
        "residue attribution: {} failures on dict-dependent rules (postag or inflected tokens, \
         token exceptions or antipattern tokens - need the missing ca-ES.dict), {} on surface-only rules",
        tag_starved, engine_side
    );
    let mut top_rules: Vec<(String, usize)> = fail_by_rule.into_iter().collect();
    top_rules.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    eprintln!(
        "top failing rules: {}",
        top_rules
            .iter()
            .take(12)
            .map(|(id, n)| format!("{id}({n})"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let _ = std::fs::write(
        "/tmp/test_failures_ca_module.txt",
        fail_lines.join("\n") + "\n",
    );
    assert!(passed + failed > 0, "suite must evaluate examples");
}

#[test]
fn suite_covers_every_live_grammar_example() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        9610,
        "ca grammar.xml carries exactly 9610 <rule> elements (1742 rulegroups + standalone) and \
         the compiler must keep them all"
    );
    let total_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    assert_eq!(
        total_examples, 25598,
        "the suite denominator must stay the full live example set: of the 27098 '<example' \
         occurrences in grammar.xml, 410 sit inside XML comments and 1090 inside <antipattern> \
         elements (Java routes those to antiPatternExample, PatternRuleHandler.java:342-348, \
         and never tests them as rule examples), leaving 25598 live rule examples - any drop \
         here is a silent og-xml parser/compiler regression"
    );
    let mut keys = std::collections::HashSet::with_capacity(rule_set.rules.len());
    for rule in &rule_set.rules {
        assert!(
            keys.insert((rule.id.clone(), rule.sub_id.clone())),
            "duplicate (id, sub_id) key {:?}/{:?}: rule_groups keeps only the first rule's \
             examples and the rest would never be evaluated",
            rule.id,
            rule.sub_id
        );
        assert!(
            !rule.examples.is_empty(),
            "rule {}/{} lost all its examples",
            rule.id,
            rule.sub_id.clone().unwrap_or_default()
        );
    }
}

#[test]
fn srx_languagemap_resolves_the_catalan_ruleset() {
    let splitter = og_tokenizer::srx::srx_splitter("ca").expect("ca SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("Aix\u{f2} \u{e9}s una frase. Aquesta \u{e9}s una altra frase.");
    assert_eq!(sentences.len(), 2);
    let sentences = splitter.split("S\u{f3}n del s. III dC. S\u{f3}n importants les pintures.");
    assert_eq!(
        sentences.len(),
        2,
        "'s. III' abbreviation must not break the sentence"
    );
    let sentences = splitter.split("A l'atenci\u{f3} d'A. Comes.");
    assert_eq!(sentences.len(), 1, "initial 'A.' must not split");
    let sentences = splitter.split("Va ser obra de Felip V. Ara ho sabem.");
    assert_eq!(
        sentences.len(),
        2,
        "Roman numeral 'V.' followed by capital splits"
    );
}

#[test]
fn grammar_rules_compile_the_java_scale() {
    let rule_set = compiled_rules();
    assert!(
        rule_set.rules.len() >= 2400,
        "ca grammar.xml must compile the real rule set, got {}",
        rule_set.rules.len()
    );
}

#[test]
fn pipeline_tags_from_the_added_lists_and_runs_the_hybrid_disambiguator() {
    let pipeline = CatalanPipeline::shared();
    let sentences = pipeline.analyze_offset_text("AICLE \u{e9}s un acr\u{f2}nim.");
    assert_eq!(sentences.len(), 1);
    let aicle = sentences[0]
        .0
        .tokens()
        .iter()
        .find(|t| t.token().token() == "AICLE")
        .expect("AICLE token");
    assert!(
        aicle.has_pos_tag("NCMS000"),
        "added.txt entry must reach the readings: {:?}",
        aicle.readings()
    );
}

#[test]
fn surface_only_grammar_rules_fire_through_the_ca_pipeline() {
    let engine = PatternRuleEngine::new();
    let rule_set = compiled_rules();
    let pipeline = CatalanPipeline::shared();
    let mut fired = 0usize;
    for rule in &rule_set.rules {
        if rule_needs_tags(rule) || rule.examples.is_empty() {
            continue;
        }
        let example = &rule.examples[0];
        if example.example_type != og_xml::types::XmlExampleType::Incorrect {
            continue;
        }
        let (clean_text, _) = extract_marker(&example.text);
        let mut any = false;
        for (sentence, _) in pipeline.analyze_offset_text(&clean_text) {
            any |= !engine.match_rule(rule, &sentence).is_empty();
        }
        if any {
            fired += 1;
        }
    }
    assert!(
        fired > 50,
        "surface-only (text/regexp-token) rules must fire without the dictionary, got {fired}"
    );
}

#[test]
fn catalan_word_repeat_fires_on_the_java_demo_text() {
    let rule = super::native_rules::CatalanWordRepeatRule::new();
    let pipeline = CatalanPipeline::shared();
    let text = "Introdu\u{ef}u aqu\u{ed} el vostre text. o feu servir aquest texts com a a exemple per a alguns errades que LanguageTool hi pot detectat.";
    let mut count = 0usize;
    for (sentence, _) in pipeline.analyze_offset_text(text) {
        count += rule.match_sentence(&sentence).len();
    }
    assert!(
        count >= 1,
        "the demo text 'a a' must fire CATALAN_WORD_REPEAT_RULE: {count}"
    );
}

#[test]
fn unpaired_question_mark_rule_fires_through_the_pipeline() {
    use og_core::rule::TextLevelRule as _;
    let rule = super::native_rules::catalan_unpaired_question_marks_rule();
    let pipeline = CatalanPipeline::shared();
    let sentences: Vec<AnalyzedSentence> = pipeline
        .analyze_offset_text("Com est\u{e0}s avui?")
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert_eq!(rule.match_text("", &sentences).len(), 1);
}

#[test]
fn wrong_word_in_context_fires_through_the_pipeline() {
    let rule = super::native_rules::CatalanWrongWordInContextRule::new();
    let pipeline = CatalanPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Vam comprar xocolate de m\u{e0}nec.");
    assert_eq!(
        rule.match_sentence(&sentences[0].0).len(),
        1,
        "the xocolate/mànec confusion is surface-based and must fire"
    );
}

#[test]
fn demo_text_residue_is_dict_starved() {
    let engine = PatternRuleEngine::new();
    let rule_set = compiled_rules();
    let pipeline = CatalanPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let text = "Introdu\u{ef}u aqu\u{ed} el vostre text. o feu servir aquest texts com a a exemple per a alguns errades que LanguageTool hi pot detectat.";
    let sentences = pipeline.analyze_offset_text(text);
    let mut ids: Vec<String> = Vec::new();
    for rule in &rule_set.rules {
        for (sentence, sentence_start) in &sentences {
            for m in engine.match_rule(rule, sentence) {
                ids.push(format!("{}@{}", rule.id, m.offset() + sentence_start));
            }
        }
    }
    eprintln!("ca demo grammar ids: {ids:?}");
    assert!(
        !ids.is_empty(),
        "some grammar rules must fire even with added-lists-only tagging"
    );
}

#[test]
fn check_case_fires_over_the_pipeline_tokens() {
    let rule = super::native_rules::CheckCaseRule::new();
    let pipeline = CatalanPipeline::shared();
    let sentences = pipeline.analyze_offset_text("El lliure Da Vinci \u{e9}s bonic.");
    assert_eq!(rule.match_sentence(&sentences[0].0).len(), 1);
}

#[test]
fn sentence_with_readings_helper_is_exported() {
    let sentence =
        super::native_rules::sentence_with_readings(&[("AICLE", &[("AICLE", "NCMS000")])]);
    assert!(sentence.tokens()[1].has_pos_tag("NCMS000"));
}

#[test]
fn pipeline_gv_chunking_stays_dict_starved_on_our_data() {
    let pipeline = CatalanPipeline::shared();
    let sentences = pipeline.analyze_offset_text("S'ha de fer.");
    let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentences[0]
        .0
        .tokens()
        .iter()
        .filter(|t| !t.is_whitespace() && !t.is_sentence_start())
        .collect();
    let surfaces: Vec<&str> = tokens.iter().map(|t| t.token().token()).collect();
    assert_eq!(surfaces, vec!["S'", "ha", "de", "fer", "."]);
    assert!(
        tokens.iter().all(|t| !t.has_chunk("GV")),
        "added.txt carries no haver/de/fer verb readings, so the GV chunker cannot fire on our data"
    );
}

#[test]
fn pronom_feble_duplicate_stays_dict_starved_through_the_pipeline() {
    let rule = super::pronom_feble_duplicate::PronomFebleDuplicateRule::new();
    let pipeline = CatalanPipeline::shared();
    let text = "S'ha de fer-se";
    let sentences = pipeline.analyze_offset_text(text);
    let matches = rule.match_sentence(&sentences[0].0);
    assert!(
        matches.is_empty(),
        "our-data outcome: without ca-ES.dict the tagger emits no pronom-feble or verb readings, so PRONOMS_FEBLES_DUPLICATS stays dormant (algorithm-level pins live in pronom_feble_duplicate::tests)"
    );
}

#[test]
fn compound_rule_pipeline_outcome_on_a_two_word_compound() {
    let rule = super::compound_rule::CompoundRule::new();
    let pipeline = CatalanPipeline::shared();
    let sentences = pipeline.analyze_offset_text("El blau grana \u{e9}s bonic.");
    let matches = rule.match_sentence(&sentences[0].0);
    assert_eq!(
        matches.len(),
        1,
        "our-data outcome: 'blau grana' must fire CA_COMPOUNDS with the tagged merge 'blaugrana'"
    );
    assert!(
        matches[0]
            .replacements()
            .iter()
            .any(|r| r.value() == "blaugrana")
    );
}

#[test]
fn unpaired_brackets_pipeline_spans_match_the_text() {
    let rule = super::catalan_unpaired_brackets_rule();
    let pipeline = CatalanPipeline::shared();
    let text = "Aix\u{f2} \u{e9}s (impossible. ";
    let sentences: Vec<AnalyzedSentence> = pipeline
        .analyze_offset_text(text)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    let matches = rule.match_text(text, &sentences);
    assert_eq!(matches.len(), 1);
    assert_eq!(
        &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
        "("
    );
}

#[test]
fn word_repeat_beginning_pipeline_fires_on_the_java_example() {
    let rule = super::word_repeat_beginning::CatalanWordRepeatBeginningRule::new();
    let pipeline = CatalanPipeline::shared();
    let text = "Per\u{f2} el carrer \u{e9}s tot modernista. Per\u{f2} t\u{e9} nom de poeta.";
    let sentences: Vec<AnalyzedSentence> = pipeline
        .analyze_offset_text(text)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    let matches = rule.match_text(text, &sentences);
    assert_eq!(matches.len(), 1);
    assert_eq!(
        &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
        "Per\u{f2}"
    );
}

#[test]
fn remote_rules_stay_silent_through_the_pipeline() {
    let pipeline = CatalanPipeline::shared();
    let text = "Aix\u{f2} \u{e9}s una frase. I aquesta \u{e9}s una altra.";
    let sentences: Vec<AnalyzedSentence> = pipeline
        .analyze_offset_text(text)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert!(
        super::remote_rules::CatalanRemoteRule::new()
            .match_text(text, &sentences)
            .is_empty()
    );
    assert!(
        super::remote_rules::CatalanSplitLongSentenceRule::new(60)
            .match_text(text, &sentences)
            .is_empty()
    );
}
