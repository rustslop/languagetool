use crate::pt::example_suite::{apply_suite_filter, suite_engine, suite_synthesizer};
use crate::pt::filters::{is_pt_filter, pt_rules_dir};
use crate::pt::test_support::PortuguesePipeline;
use og_core::AnalyzedTokenReadings;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, XmlCompiler};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
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

fn pt_pt_grammar_path() -> PathBuf {
    pt_rules_dir()
        .expect("pt rules dir")
        .join("pt-PT/grammar.xml")
}

type ExampleOutcome = (String, String, Option<(Outcome, String)>);

#[test]
#[ignore]
fn pt_pt_grammar_examples_with_pt_pipeline() {
    let _release_guard = crate::release_guard();
    let path = pt_pt_grammar_path();
    let xml = std::fs::read_to_string(&path).expect("pt-PT grammar.xml");
    let raw_example_count = xml.matches("<example").count();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, path.parent())
        .expect("compile pt-PT grammar.xml");

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
    let compiled_example_count: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    let evaluated_example_count: usize = groups
        .iter()
        .map(|g| g.first().map(|r| r.examples.len()).unwrap_or(0))
        .sum();

    let engine: PatternRuleEngine = suite_engine();
    let synthesizer = suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let started = std::time::Instant::now();

    type GroupExamplePair<'a> = (&'a Vec<&'a CompiledRule>, &'a og_xml::types::XmlExample);
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
                    let pt_filter = aux.filter.as_ref().filter(|f| is_pt_filter(&f.class));
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                        let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
                        for m in engine.match_rule(aux, &sentence) {
                            let Some(mut m) = (match pt_filter {
                                Some(filter) => apply_suite_filter(
                                    aux,
                                    filter,
                                    &sentence,
                                    &tokens,
                                    synthesizer.as_ref(),
                                    m,
                                ),
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
                                        "Match {match_start}..{match_end} does not contain marker {m_start}..{m_end}"
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
    let mut fail_by_rule: HashMap<String, usize> = HashMap::new();
    for (rule_id, clean_text, failure) in per_example {
        match failure {
            None => passed += 1,
            Some((outcome, message)) => {
                failed += 1;
                *outcome_counts.entry(outcome).or_default() += 1;
                fail_lines.push(format!("FAIL: [{rule_id}] {message}: '{clean_text}'"));
                *fail_by_rule.entry(rule_id).or_default() += 1;
            }
        }
    }

    eprintln!(
        "pt module pipeline [pt-PT/grammar.xml]: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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
        "accepted-excluded residue [pt-PT/grammar.xml]: {raw_example_count} <example> tags, {compiled_example_count} on compiled rules, {} on non-first rules of duplicate (id,sub_id) groups, {evaluated_example_count} evaluated",
        compiled_example_count - evaluated_example_count,
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
        "/tmp/test_failures_pt_pt_module.txt",
        fail_lines.join("\n") + "\n",
    );
    assert!(passed + failed > 0, "suite must evaluate examples");
}

#[test]
fn pt_pt_grammar_file_pins_the_refreshed_delta() {
    let path = pt_pt_grammar_path();
    assert!(path.exists(), "pt-PT/grammar.xml must resolve: {path:?}");
    let xml = std::fs::read_to_string(path).expect("pt-PT grammar.xml");
    assert_eq!(xml.matches("<example").count(), 1088);
    assert!(
        xml.contains("BRASILEIRISMO_TER_EXISTENCIAL_AQUI"),
        "the delta rule must be present in the refreshed pt-PT/grammar.xml"
    );
}
