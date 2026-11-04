use std::collections::HashMap;
use std::sync::Arc;

use og_core::AnalyzedSentence;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::CompiledRule;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;
use og_xml::types::XmlExampleType;

pub(crate) fn ca_resource_dir() -> Option<std::path::PathBuf> {
    crate::ca::tagger::ca_resource_dir()
}

pub(crate) fn ca_rules_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ca/src/main/resources/org/languagetool/rules/ca"))
}

pub(crate) fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = ca_rules_dir().expect("ca rules dir").join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("ca grammar.xml");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ca grammar.xml"),
        ))
    })
    .expect("ca refresh grammar")
}

pub(crate) struct RefreshPipeline {
    tokenizer: crate::ca::word_tokenizer::CatalanWordTokenizer,
    tagger: Arc<crate::ca::CatalanTagger>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl RefreshPipeline {
    pub fn shared() -> Arc<RefreshPipeline> {
        static PIPE: og_core::releasable::Releasable<RefreshPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = ca_resource_dir().expect("ca resource dir");
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .map(|xml| {
                    og_xml::parser::XmlRuleParser::expand_entities_with_base(
                        &xml,
                        Some(&resource_dir),
                    )
                })
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok())
                .and_then(|xml| {
                    crate::engine::find_lt_core_resource_dir().map(|core| {
                        og_tagger::hybrid_disambiguator("ca", &resource_dir, &core, xml)
                    })
                });
            Some(Arc::new(RefreshPipeline {
                tokenizer: crate::ca::word_tokenizer::CatalanWordTokenizer::new(),
                tagger: crate::ca::CatalanTagger::shared_arc().expect("ca tagger over added.txt"),
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("ca"),
            }))
        })
        .expect("ca refresh pipeline")
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
            let typographic_apostrophe = atr.has_typographic_apostrophe();
            let readings = atr.into_readings();
            if !readings.is_empty() {
                let primary = tokens_mut[token_idx].token().clone();
                let updated_primary =
                    AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                        .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                let mut updated =
                    AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
                if typographic_apostrophe {
                    updated.set_chunks(vec![og_core::TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string()]);
                }
                tokens_mut[token_idx] = updated;
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
        crate::ca::gv_chunker::add_gv_chunks(&mut sentence);
        sentence
    }

    pub fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        let splitter = match &self.splitter {
            Some(srx) => srx,
            None => return vec![(self.analyze_range(text, 0, text.len()), 0)],
        };
        splitter
            .split(text)
            .into_iter()
            .map(|s| (self.analyze_range(text, s.start(), s.end()), s.start()))
            .collect()
    }
}

pub(crate) fn strip_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
}

type ExampleRecord = (String, String, Option<(Outcome, String)>, bool);

const PINNED_PASSED: usize = 25189;
const PINNED_DICT_EXCLUDED: usize = 548;
const PINNED_SURFACE_RESIDUES: &[(&str, &str)] = &[
    (
        "MOTS_NO_SEPARATS",
        "El cantant Fito Luri farà una actuació.",
    ),
    ("MOTS_NO_SEPARATS", "No tinc re d'això"),
    ("MOTS_NO_SEPARATS", "Un concert en re major menor."),
    ("MOTS_NO_SEPARATS", "Una sonata en re menor."),
    (
        "MOTS_NO_SEPARATS",
        "És un exemple retro del que passa a tot arreu.",
    ),
    ("RECOLZAMENT", "Amb la vostre\u{a0}recolzament."),
    ("RECOLZAMENT", "Sense recolzament popular."),
    ("RECOLZAMENT", "Sense recolzaments parlamentaris."),
];

pub(crate) struct VetoAdapter {
    pub rule: og_xml::compiler::CompiledRule,
    pub hook: std::sync::Arc<crate::ca_filter_semantics_delta::dispatch::CatalanPatternFilterHook>,
}

impl og_core::Rule for VetoAdapter {
    fn id(&self) -> &str {
        &self.rule.id
    }
    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<og_core::RuleMatch> {
        let concrete = std::sync::Arc::clone(&self.hook);
        let hook: std::sync::Arc<dyn og_rules::language_filter::LanguageFilterHook> = concrete;
        PatternRuleEngine::new()
            .with_language_filter_hook(hook)
            .match_rule(&self.rule, sentence)
    }
}

pub(crate) struct CaSuppressSupport<F>
where
    F: Fn(&str) -> Vec<AnalyzedSentence> + Send + Sync,
{
    pub rules: std::sync::Arc<CompiledRuleSet>,
    pub hook: std::sync::Arc<crate::ca_filter_semantics_delta::dispatch::CatalanPatternFilterHook>,
    pub analyze: F,
}

impl<F> og_rules::suppress_if_any::SuppressIfAnySupport for CaSuppressSupport<F>
where
    F: Fn(&str) -> Vec<AnalyzedSentence> + Send + Sync,
{
    fn analyze_text(&self, text: &str) -> Vec<AnalyzedSentence> {
        (self.analyze)(text)
    }
    fn rules_for_ids(&self, rule_ids: &[&str]) -> Vec<std::sync::Arc<dyn og_core::Rule>> {
        self.rules
            .rules
            .iter()
            .filter(|r| rule_ids.contains(&r.id.as_str()))
            .map(|r| {
                let adapter: std::sync::Arc<dyn og_core::Rule> = std::sync::Arc::new(VetoAdapter {
                    rule: r.clone(),
                    hook: std::sync::Arc::clone(&self.hook),
                });
                adapter
            })
            .collect()
    }
}

pub(crate) fn ca_suite_engine(
    rule_set: &std::sync::Arc<CompiledRuleSet>,
    resource_dir: &std::path::Path,
    analyze: impl Fn(&str) -> Vec<AnalyzedSentence> + Send + Sync + 'static,
) -> PatternRuleEngine {
    let hook = std::sync::Arc::new(
        crate::ca_filter_semantics_delta::dispatch::CatalanPatternFilterHook::new(
            crate::ca::tagger::CatalanTagger::shared_arc(),
            og_rules::postponed_adjective::LangSynthesizer::for_language(resource_dir, "ca")
                .map(std::sync::Arc::new),
        ),
    );
    let tagger = crate::ca::tagger::CatalanTagger::shared_arc()
        .map(|tagger| tagger as std::sync::Arc<dyn og_tagger::Tagger>);
    let hook_concrete = std::sync::Arc::clone(&hook);
    let hook_dyn: std::sync::Arc<dyn og_rules::language_filter::LanguageFilterHook> = hook_concrete;
    PatternRuleEngine::new()
        .with_language_filter_hook(hook_dyn)
        .with_suppress_if_any_support(
            std::sync::Arc::new(CaSuppressSupport {
                rules: std::sync::Arc::clone(rule_set),
                hook,
                analyze,
            }),
            tagger,
        )
}

fn evaluate_all() -> Vec<ExampleRecord> {
    use rayon::prelude::*;
    let rule_set = compiled_rules();
    let resource_dir = ca_resource_dir().expect("ca resource dir");
    let pipeline = RefreshPipeline::shared();
    let pipeline_for_engine = std::sync::Arc::clone(&pipeline);
    let engine = ca_suite_engine(&rule_set, &resource_dir, move |text| {
        pipeline_for_engine
            .analyze_offset_text(text)
            .into_iter()
            .map(|(sentence, _)| sentence)
            .collect()
    });
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut grouped: Vec<Vec<&CompiledRule>> = Vec::new();
    for rule in &rule_set.rules {
        let key = (rule.id.clone(), rule.sub_id.clone());
        let next = grouped.len();
        let idx = *group_index.entry(key).or_insert(next);
        if idx == next {
            grouped.push(vec![rule]);
        } else {
            grouped[idx].push(rule);
        }
    }

    let pairs: Vec<(&Vec<&CompiledRule>, &og_xml::types::XmlExample)> = grouped
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    pairs
        .par_iter()
        .map(|(group, example)| {
            let rule = group[0];
            let (clean_text, marker_range) = strip_marker(&example.text);
            let mut matches = Vec::new();
            let unsplit_incorrect = example.example_type == XmlExampleType::Incorrect;
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
                XmlExampleType::Incorrect => {
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
                                    "Match {match_start}..{match_end} does not contain marker {m_start}..{m_end}"
                                ),
                            ))
                        }
                    } else {
                        None
                    }
                }
                XmlExampleType::Correct => {
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
                XmlExampleType::TriggersError => None,
            };
            (
                rule.id.clone(),
                clean_text,
                failure,
                crate::ca::trace::rule_needs_readings(rule),
            )
        })
        .collect()
}

#[test]
#[ignore]
fn catalan_refresh_example_suite_counts_accepted_exclusions() {
    let _release_guard = crate::release_guard();
    let started = std::time::Instant::now();
    let per_example = evaluate_all();

    let mut passed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut dict_excluded = 0usize;
    let mut surface: Vec<(String, String)> = Vec::new();
    let mut fail_by_rule: HashMap<String, usize> = HashMap::new();
    let mut evaluated = 0usize;

    for (rule, clean_text, failure, needs_tags) in per_example {
        evaluated += 1;
        let Some((outcome, message)) = failure else {
            passed += 1;
            continue;
        };
        *outcome_counts.entry(outcome).or_default() += 1;
        *fail_by_rule.entry(rule.clone()).or_default() += 1;
        if needs_tags {
            dict_excluded += 1;
        } else {
            eprintln!("SURFACE: [{rule}] {outcome:?} {message}: '{clean_text}'");
            surface.push((rule, clean_text));
        }
    }
    surface.sort_unstable();

    let failed = evaluated - passed;
    eprintln!(
        "ca refresh suite: {passed}/{evaluated} passed, {failed} failed ({:.1}s) - \
         dict-excluded: {dict_excluded}, surface: {}",
        started.elapsed().as_secs_f64(),
        surface.len()
    );
    eprintln!(
        "outcomes: no_match: {}, fp: {}, marker: {}",
        outcome_counts.get(&Outcome::NoMatch).copied().unwrap_or(0),
        outcome_counts
            .get(&Outcome::FalsePositive)
            .copied()
            .unwrap_or(0),
        outcome_counts.get(&Outcome::Marker).copied().unwrap_or(0),
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

    assert_eq!(
        evaluated, 25745,
        "the full live example set must be evaluated"
    );
    assert_eq!(
        passed, PINNED_PASSED,
        "passed count pinned at the 2c9b51e refresh with ca-ES.dict loaded"
    );
    assert_eq!(
        dict_excluded, PINNED_DICT_EXCLUDED,
        "reading-consulting rules are the accepted-excluded mass (agreement, filter and \
         synthesis divergences against Java with the loaded ca-ES.dict)"
    );
    let actual: Vec<(&str, &str)> = surface
        .iter()
        .map(|(r, t)| (r.as_str(), t.as_str()))
        .collect();
    assert_eq!(
        actual, PINNED_SURFACE_RESIDUES,
        "every surface-only failure must be an explicitly accepted residue"
    );
}

#[test]
fn pipeline_tokens_keep_the_typographic_apostrophe_flag() {
    let pipeline = RefreshPipeline::shared();
    let sentence = pipeline.analyze_range("L\u{2019}home", 0, 7);
    let flagged = sentence
        .tokens()
        .iter()
        .filter(|t| t.has_typographic_apostrophe())
        .count();
    assert_eq!(
        flagged, 1,
        "the rebuilt suite tokens must carry the tagger chunk like Java setTypographicApostrophe"
    );
}
