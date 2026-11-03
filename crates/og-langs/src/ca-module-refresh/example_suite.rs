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

const PINNED_PASSED: usize = 15723;
const PINNED_DICT_EXCLUDED: usize = 9968;
const PINNED_SURFACE_RESIDUES: &[(&str, &str)] = &[
    ("AL_ANDALUS", "Un estudi sobre l'al-Àndalus."),
    ("APOSTROF_RECTE", "L’home"),
    ("APOSTROF_RECTE", "Se’ns va explicar."),
    ("APOSTROF_TIPOGRAFIC", "L’home"),
    (
        "AVANTATJA_AVANTATGE",
        "L'avantatja decisiva que tenen és molt gran.",
    ),
    ("AVANTATJA_AVANTATGE", "La verdadera avantatja competitiva."),
    (
        "AVANTATJA_AVANTATGE",
        "Va ser una gran avantatja poder usar aquestes plataformes.",
    ),
    ("COLLE", "Els col·les ho fan."),
    ("COLLE", "Me'n vaig amb el col·le."),
    ("COMMA_LOCUTION", "Això en canvi no és correcte."),
    (
        "COMMA_LOCUTION",
        "Ara farem per exemple un exercici de fraccions.",
    ),
    (
        "COMMA_LOCUTION",
        "Finalment doncs, les ajudes públiques es van aprovar.",
    ),
    (
        "COMMA_LOCUTION",
        "M'agradava aquesta, això no obstant m'enduguí l'altra.",
    ),
    (
        "COMMA_LOCUTION",
        "M'agradava aquesta, ara bé m'enduguí l'altra.",
    ),
    ("COMMA_LOCUTION", "O per exemple, el mes que ve."),
    (
        "COMMA_LOCUTION",
        "Va dir, no obstant això que era necessari.",
    ),
    ("CONCORDANCES_NUMERALS_DOS", "Dues-centes xiquetes."),
    (
        "CONCORDANCES_NUMERALS_DOS",
        "Han vingut vint-i-dues xiquetes.",
    ),
    ("CONCORDANCES_NUMERALS_DOS", "Trenta-dues xiquetes."),
    ("CONCORDANCES_NUMERALS_DUES", "Dos-centes."),
    ("COSE", "Tinc una cose per a tu."),
    ("DEJA_VU", "Un déjà-vu."),
    ("ENTENIMENT_ENTESA", "Cal un enteniment entre generacions"),
    (
        "ENTENIMENT_ENTESA",
        "cal un enteniment mutu entre generacions",
    ),
    (
        "ENTENIMENT_ENTESA",
        "cal un enteniment recíproc entre generacions",
    ),
    ("LIJA", "Ho vam polir amb lija fina."),
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
    (
        "MUNICIPIS_CATALANS_FORMA_NORMATIVA_NO_OFICIAL",
        "Ajuntament del Figaró-Montmany.",
    ),
    (
        "MUNICIPIS_CATALANS_FORMA_NORMATIVA_NO_OFICIAL",
        "Allò era Figaró-Montmany.",
    ),
    ("NORD_AMERICA", "A Sud-àfrica"),
    ("PROVA_DEL_COTO", "Serà la prova del cotó-en-pèl."),
    ("RECOLZAMENT", "Amb la vostre\u{a0}recolzament."),
    ("RECOLZAMENT", "Sense recolzament popular."),
    ("RECOLZAMENT", "Sense recolzaments parlamentaris."),
    ("RODALIES", "Per la ciutat i les seves rodalies."),
    ("TINGLADO", "Els tinglados."),
    ("TINGLADO", "Era un tinglado."),
];

fn evaluate_all() -> Vec<ExampleRecord> {
    use rayon::prelude::*;
    let rule_set = compiled_rules();
    let engine = PatternRuleEngine::new();
    let pipeline = RefreshPipeline::shared();
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
        evaluated, 25732,
        "the full live example set must be evaluated"
    );
    assert_eq!(
        passed, PINNED_PASSED,
        "passed count pinned at the 4cff7008 refresh"
    );
    assert_eq!(
        dict_excluded, PINNED_DICT_EXCLUDED,
        "dict/synthesis-dependent failures are the accepted-excluded mass (missing ca-ES.dict)"
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
