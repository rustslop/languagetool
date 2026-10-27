use crate::fr::test_support::FrenchPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::XmlCompiler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn grammar_path() -> PathBuf {
    crate::fr::test_support::fr_rules_dir()
        .expect("fr rules dir")
        .join("grammar.xml")
}

pub(crate) fn build_pattern_engine() -> PatternRuleEngine {
    let resource_dir = crate::fr::test_support::fr_resource_dir().expect("fr resource dir");
    let core_resource_dir = crate::engine::find_lt_core_resource_dir().expect("core resource dir");
    let dict = crate::fr::suppress_misspelled::shared_french_spelling_dictionary()
        .unwrap_or_else(|| Arc::new(og_spell::Dictionary::new()));
    let support = og_rules::spelling_support::SpellingSupport::new_for_language(
        dict,
        &core_resource_dir,
        &resource_dir,
        "fr",
    );
    let synth = og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "fr")
        .map(Arc::new);
    let mut engine = PatternRuleEngine::new().with_spelling_support(Arc::new(support));
    if let Some(synth) = synth {
        engine = engine
            .with_synthesizer(synth.clone())
            .with_suggestion_synthesizer(synth);
    }
    engine
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

type ExampleOutcome = (String, String, Option<(Outcome, String)>);

type GroupExamplePair<'a> = (
    &'a Vec<&'a og_xml::compiler::CompiledRule>,
    &'a og_xml::types::XmlExample,
);

#[test]
#[ignore]
fn french_grammar_examples_with_fr_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &grammar_path(),
        "grammar.xml",
        Some("/tmp/test_failures_fr_module.txt"),
    );
}

#[test]
#[ignore]
fn french_style_examples_with_fr_pipeline() {
    let _release_guard = crate::release_guard();
    let style_path = crate::fr::test_support::fr_rules_dir()
        .expect("fr rules dir")
        .join("style.xml");
    evaluate_examples_file(
        &style_path,
        "style.xml",
        Some("/tmp/test_failures_fr_style.txt"),
    );
}

fn evaluate_examples_file(rules_path: &std::path::Path, file_label: &str, artifact: Option<&str>) {
    let xml =
        crate::fr::normalize_fr_rules_xml(&std::fs::read_to_string(rules_path).expect("rules xml"))
            .into_owned();
    let raw_example_count = xml.matches("<example").count();
    let without_comments = strip_xml_comments(&xml);
    let comment_example_count = raw_example_count - without_comments.matches("<example").count();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, rules_path.parent())
        .expect("compile rules xml");

    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&og_xml::compiler::CompiledRule>> = Vec::new();
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

    let compiled_example_count: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    let evaluated_example_count: usize = groups
        .iter()
        .map(|g| g.first().map(|r| r.examples.len()).unwrap_or(0))
        .sum();

    let engine = build_pattern_engine();
    let pipeline = FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let started = std::time::Instant::now();

    let pairs: Vec<GroupExamplePair> = groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    let per_example: Vec<ExampleOutcome> = {
        use rayon::prelude::*;
        pairs
            .par_iter()
            .map(|(group, example)| {
                let rule = group[0];
                let (clean_text, marker_range) = extract_marker(&example.text);
                let mut matches = Vec::new();
                for aux in group.iter() {
                    let fr_filter = aux
                        .filter
                        .as_ref()
                        .filter(|f| {
                            f.class
                                == crate::fr::suppress_misspelled::FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS
                        });
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                        for m in engine.match_rule(aux, &sentence) {
                            let Some(mut m) = (match fr_filter {
                                Some(filter) => {
                                    crate::fr::suppress_misspelled::accept_rule_match(
                                        m,
                                        &filter.args,
                                        &[],
                                    )
                                }
                                None => Some(m),
                            }) else {
                                continue;
                            };
                            m.set_offset_length(m.offset() + sentence_start, m.length());
                            matches.push(m);
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
                                        "Match {}..{} does not contain marker {}..{}",
                                        match_start, match_end, m_start, m_end
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
                (rule.id.clone(), clean_text, failure)
            })
            .collect()
    };

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut fail_lines: Vec<String> = Vec::new();

    for (rule_id, clean_text, failure) in per_example {
        match failure {
            None => passed += 1,
            Some((outcome, message)) => {
                failed += 1;
                *outcome_counts.entry(outcome).or_default() += 1;
                fail_lines.push(format!("FAIL: [{}] {}: '{}'", rule_id, message, clean_text));
            }
        }
    }

    eprintln!(
        "fr module pipeline [{file_label}]: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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
        "accepted-excluded residue [{file_label}]: {raw_example_count} <example> tags ({comment_example_count} inside XML comments), {compiled_example_count} on compiled rules, {} on non-first rules of duplicate (id,sub_id) groups, {evaluated_example_count} evaluated",
        compiled_example_count - evaluated_example_count,
    );
    if let Some(artifact) = artifact {
        let _ = std::fs::write(artifact, fail_lines.join("\n") + "\n");
    }
    assert!(passed + failed > 0, "suite must evaluate examples");
}

fn strip_xml_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        match after.find("-->") {
            Some(end) => {
                rest = &after[end + 3..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filtered_matches(rule_id: &str, text: &str) -> Vec<(usize, usize, Vec<String>)> {
        let xml = crate::fr::normalize_fr_rules_xml(
            &std::fs::read_to_string(grammar_path()).expect("grammar.xml"),
        )
        .into_owned();
        let rule_set = XmlCompiler::new()
            .compile_file_with_base(&xml, grammar_path().parent())
            .expect("compile grammar.xml");
        let engine = build_pattern_engine();
        let pipeline = FrenchPipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);
        let mut out = Vec::new();
        for rule in &rule_set.rules {
            if rule.id != rule_id {
                continue;
            }
            let fr_filter = rule.filter.as_ref().filter(|f| {
                f.class == crate::fr::suppress_misspelled::FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS
            });
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                for m in engine.match_rule(rule, &sentence) {
                    let Some(m) = (match fr_filter {
                        Some(filter) => {
                            crate::fr::suppress_misspelled::accept_rule_match(m, &filter.args, &[])
                        }
                        None => Some(m),
                    }) else {
                        continue;
                    };
                    let start = m.offset() + sentence_start;
                    out.push((
                        start,
                        start + m.length(),
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect(),
                    ));
                }
            }
        }
        out
    }

    #[test]
    fn la_la2_no_longer_false_positives_now_that_the_noop_flag_group_is_normalized() {
        let matches = filtered_matches("LA_LA2", "Mais la plupart travaillaient dans la pêcherie.");
        assert!(
            matches.is_empty(),
            "the exception list (plupart|fois|...) must suppress 'la plupart': {matches:?}"
        );
        let matches = filtered_matches("LA_LA2", "La personne photographiée est à la fois :");
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn faire_plus_no_longer_false_positives_now_that_alternatives_are_joined() {
        let matches = filtered_matches("FAIRE_PLUS_NOM_INCORRECT", "Il fait frais.");
        assert!(
            matches.is_empty(),
            "'frais' must be covered by the exception list: {matches:?}"
        );
        let matches =
            filtered_matches("REFAIRE_PLUS_NOM_INCORRECT", "Il refait usage de son bras.");
        assert!(
            matches.is_empty(),
            "'usage' must be covered by the exception list: {matches:?}"
        );
    }

    #[test]
    fn suppress_misspelled_filter_removes_split_words_false_positive() {
        let matches = filtered_matches("FR_SPLIT_WORDS_HYPHEN", "The pia- nist");
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn suppress_misspelled_filter_keeps_valid_split_words_correction() {
        let matches = filtered_matches("FR_SPLIT_WORDS_HYPHEN", "Le pia- niste joue très bien.");
        assert!(
            matches
                .iter()
                .any(|(_, _, replacements)| replacements.contains(&"pianiste".to_string())),
            "{matches:?}"
        );
    }

    #[test]
    fn suppress_misspelled_filter_removes_apos_espace_false_positive() {
        let matches = filtered_matches("APOS_ESPACE", "La surface du mur ne s'abîme.");
        assert!(matches.is_empty(), "{matches:?}");
    }
}

#[cfg(test)]
mod synthetic_evaluator_tests {
    use super::*;

    fn synthetic_rules() -> String {
        [
            "<rules lang=\"fr\">",
            "<rule id=\"SYN_MATCH\" name=\"syn match\">",
            "<pattern><token>gronz</token></pattern>",
            "<message>mot mauvais</message>",
            "<example type=\"incorrect\">Ceci est un <marker>gronz</marker> ici.</example>",
            "<example type=\"correct\">Ceci est correct.</example>",
            "</rule>",
            "<rule id=\"SYN_NOMATCH\" name=\"syn nomatch\">",
            "<pattern><token>zzzqqq</token></pattern>",
            "<message>mot mauvais</message>",
            "<example type=\"incorrect\">Rien \u{e0} voir.</example>",
            "</rule>",
            "<rule id=\"SYN_FP\" name=\"syn fp\">",
            "<pattern><token>Ceci</token></pattern>",
            "<message>mot mauvais</message>",
            "<example type=\"correct\">Ceci est correct.</example>",
            "</rule>",
            "<rule id=\"SYN_MARKER_MISS\" name=\"syn marker miss\">",
            "<pattern><token>cas</token></pattern>",
            "<message>mot mauvais</message>",
            "<example type=\"incorrect\">Ceci est <marker>gronz</marker> un cas.</example>",
            "</rule>",
            "<!-- <example type=\"correct\">commented</example> -->",
            "</rules>",
        ]
        .join("\n")
    }

    #[test]
    fn evaluate_examples_file_classifies_synthetic_outcomes_into_the_artifact() {
        let path = std::env::temp_dir().join(format!(
            "og_fr_synthetic_grammar_{}.xml",
            std::process::id()
        ));
        std::fs::write(&path, synthetic_rules()).expect("write synthetic rules");
        let artifact_path = std::env::temp_dir().join(format!(
            "og_fr_synthetic_artifact_{}.txt",
            std::process::id()
        ));
        evaluate_examples_file(&path, "synthetic", Some(artifact_path.to_str().unwrap()));
        let artifact = std::fs::read_to_string(&artifact_path).unwrap_or_default();

        assert!(
            artifact
                .contains("FAIL: [SYN_NOMATCH] Expected match on incorrect example but got none"),
            "the unmatched incorrect example must be recorded as a NoMatch failure: {artifact}"
        );
        assert!(
            artifact.contains("FAIL: [SYN_FP] Expected no match on correct example but got"),
            "the firing rule on a correct example must be recorded as a false positive: {artifact}"
        );
        assert!(
            artifact.contains("does not contain marker"),
            "a match outside the marker span must be recorded as a marker failure: {artifact}"
        );
        assert!(
            !artifact.contains("SYN_MATCH"),
            "the rule whose match covers the marker and whose correct example stays clean must pass: {artifact}"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&artifact_path);
    }
}
