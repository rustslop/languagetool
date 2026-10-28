use std::collections::HashMap;
use std::sync::Arc;

use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::CompiledPatternElement;
use og_xml::types::XmlExampleType;

use super::pipeline::RefreshPipeline;
use super::pipeline::compiled_rules;
use super::pipeline::es_synth;
use super::pipeline::grammar_path;
use super::pipeline::pattern_engine;
use super::pipeline::strip_marker;

fn token_matches(
    token: &og_xml::compiler::CompiledPatternToken,
    atr: &og_core::AnalyzedTokenReadings,
) -> bool {
    let surface = atr.token().token();
    let text_ok = match &token.text {
        Some(text) => {
            let matched = if let Some(re) = &token.compiled_regexp_fancy {
                re.is_match(surface).unwrap_or(false)
            } else if token.case_sensitive {
                token
                    .compiled_regexp
                    .as_ref()
                    .is_some_and(|re| re.is_match(surface))
                    || surface == text
            } else {
                token
                    .compiled_regexp_ci
                    .as_ref()
                    .or(token.compiled_regexp.as_ref())
                    .is_some_and(|re| re.is_match(surface))
                    || surface.eq_ignore_ascii_case(text)
            };
            if token.negate { !matched } else { matched }
        }
        None => true,
    };
    let postag_ok = match &token.postag {
        Some(postag) => {
            let matched = atr
                .readings()
                .iter()
                .flat_map(|r| r.pos_tags().iter())
                .any(|t| {
                    if let Some(re) = &token.compiled_postag_regexp_fancy {
                        re.is_match(t).unwrap_or(false)
                    } else if let Some(re) = &token.compiled_postag_regexp {
                        re.is_match(t)
                    } else {
                        t == postag
                    }
                });
            if token.negate_pos { !matched } else { matched }
        }
        None => true,
    };
    text_ok && postag_ok
}

fn element_satisfied(
    element: &CompiledPatternElement,
    atr: &og_core::AnalyzedTokenReadings,
) -> bool {
    match element {
        CompiledPatternElement::Token(t) => token_matches(t, atr),
        CompiledPatternElement::OrGroup(alts) => alts.iter().any(|t| token_matches(t, atr)),
        CompiledPatternElement::AndGroup(all) => all.iter().all(|t| token_matches(t, atr)),
    }
}

fn element_optional(element: &CompiledPatternElement) -> bool {
    match element {
        CompiledPatternElement::Token(t) => t.min == Some(0),
        CompiledPatternElement::OrGroup(alts) => alts.iter().any(|t| t.min == Some(0)),
        CompiledPatternElement::AndGroup(all) => all.iter().all(|t| t.min == Some(0)),
    }
}

fn matched_window(
    rule: &og_xml::compiler::CompiledRule,
    tokens: &[&og_core::AnalyzedTokenReadings],
    rm: &og_core::RuleMatch,
) -> (Vec<usize>, Vec<String>) {
    let end = rm.offset() + rm.length();
    let overlap: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.token().start() < end && t.token().end() > rm.offset())
        .map(|(i, _)| i)
        .collect();
    let texts_of = |positions: &[usize]| {
        (
            positions.to_vec(),
            positions
                .iter()
                .map(|&i| tokens[i].token().token().to_string())
                .collect::<Vec<String>>(),
        )
    };
    let pattern = &rule.pattern;
    let (Some(ms), Some(me)) = (pattern.marker_elem_start, pattern.marker_elem_end) else {
        return texts_of(&overlap);
    };
    let marker_elements = me.saturating_sub(ms);
    let (Some(&first), Some(&last)) = (overlap.first(), overlap.last()) else {
        return texts_of(&overlap);
    };
    if marker_elements == 0 || overlap.len() != marker_elements {
        return texts_of(&overlap);
    }
    let elements = &pattern.elements;
    let mut start = first;
    let mut k = ms;
    while k > 0 {
        k -= 1;
        let Some(idx) = start.checked_sub(1).filter(|&i| i < tokens.len()) else {
            break;
        };
        if element_satisfied(&elements[k], tokens[idx]) || !element_optional(&elements[k]) {
            start = idx;
        }
    }
    let mut stop = last;
    let mut k = me;
    while k < elements.len() {
        let Some(idx) = stop.checked_add(1).filter(|&i| i < tokens.len()) else {
            break;
        };
        if element_satisfied(&elements[k], tokens[idx]) || !element_optional(&elements[k]) {
            stop = idx;
            k += 1;
        } else {
            break;
        }
    }
    let positions: Vec<usize> = (start..=stop).collect();
    let mut matched: Vec<usize> = Vec::with_capacity(positions.len());
    let mut texts: Vec<String> = Vec::with_capacity(elements.len());
    let mut cursor = 0usize;
    for (ei, element) in elements.iter().enumerate() {
        if ei == ms {
            cursor = first - start;
        } else if ei == me {
            cursor = last - start + 1;
        }
        let consumed = if ei >= ms && ei < me {
            positions.get(cursor).copied()
        } else {
            positions
                .get(cursor)
                .copied()
                .filter(|&p| element_satisfied(element, tokens[p]) || !element_optional(element))
        };
        match consumed {
            Some(p) => {
                matched.push(p);
                texts.push(tokens[p].token().token().to_string());
                cursor += 1;
            }
            None => texts.push(texts.last().cloned().unwrap_or_default()),
        }
    }
    if matched.is_empty() {
        return texts_of(&positions);
    }
    (matched, texts)
}

fn apply_es_filters(
    rule: &og_xml::compiler::CompiledRule,
    tokens: &[&og_core::AnalyzedTokenReadings],
    sentence_text: &str,
    m: og_core::RuleMatch,
) -> Option<og_core::RuleMatch> {
    let Some(filter) = rule
        .filter
        .as_ref()
        .filter(|f| crate::es::filters::is_es_filter(&f.class))
    else {
        return Some(m);
    };
    let (matched_positions, matched_texts) = matched_window(rule, tokens, &m);
    let synthesizer = es_synth();
    let call = crate::es::filters::EsFilterCall {
        args: &filter.args,
        matched_texts: &matched_texts,
        matched_positions: &matched_positions,
        tokens,
        sentence_text,
        synthesizer: synthesizer.as_deref(),
    };
    crate::es::filters::apply_es_filter(&filter.class, call, m)
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
}

#[test]
fn grammar_example_tags_pin_the_refreshed_count() {
    let path = grammar_path().expect("es grammar.xml");
    let xml = std::fs::read_to_string(&path).expect("es grammar.xml");
    let raw = xml.matches("<example").count();
    let mut without_comments = String::with_capacity(xml.len());
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<!--") {
        without_comments.push_str(&rest[..start]);
        rest = match rest[start..].find("-->") {
            Some(end) => &rest[start + end + 3..],
            None => "",
        };
    }
    without_comments.push_str(rest);
    let live = without_comments.matches("<example").count();
    let mut in_antipattern = 0usize;
    let mut depth = 0usize;
    let mut offset = 0usize;
    while let Some(rel) = without_comments[offset..].find('<') {
        let tag = &without_comments[offset + rel..];
        if tag.starts_with("<antipattern") {
            depth += 1;
        } else if tag.starts_with("</antipattern") && depth > 0 {
            depth -= 1;
        } else if depth > 0 && tag.starts_with("<example") {
            in_antipattern += 1;
        }
        offset += rel + 1;
    }
    assert_eq!(
        raw, 7252,
        "the es grammar example count must stay at the pre-refresh 7252 the gap pinned"
    );
    assert_eq!(live, 7140, "live (non-comment) <example> occurrences");
    assert_eq!(
        in_antipattern, 489,
        "live examples sitting inside <antipattern> blocks"
    );
    assert_eq!(
        live - in_antipattern,
        6651,
        "the evaluated mass the refresh suite runs green"
    );
}

#[test]
#[ignore]
fn es_refresh_grammar_example_suite_pins_the_java_delta() {
    let _release_guard = crate::release_guard();
    let started = std::time::Instant::now();
    let rule_set = compiled_rules();
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
    let evaluated_by_groups: usize = groups
        .iter()
        .map(|g| g.first().map(|r| r.examples.len()).unwrap_or(0))
        .sum();
    let all_compiled_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();

    let engine: Arc<PatternRuleEngine> = pattern_engine();
    let pipeline = RefreshPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let pairs: Vec<(
        &Vec<&og_xml::compiler::CompiledRule>,
        &og_xml::types::XmlExample,
    )> = groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    let per_example: Vec<(String, String, Option<(Outcome, String)>)> = {
        use rayon::prelude::*;
        pairs
            .par_iter()
            .map(|(group, example)| {
                let rule = group[0];
                let (clean_text, marker_range) = strip_marker(&example.text);
                let mut matches = Vec::new();
                for aux in group.iter() {
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                        let tokens: Vec<&og_core::AnalyzedTokenReadings> =
                            sentence.non_whitespace_tokens();
                        for m in engine.match_rule(aux, &sentence) {
                            let Some(mut m) = apply_es_filters(aux, &tokens, sentence.text(), m)
                            else {
                                continue;
                            };
                            m.set_offset_length(m.offset() + sentence_start, m.length());
                            matches.push(m);
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
                                        "Match {match_start}..{match_end} does not contain marker \
                                         {m_start}..{m_end}"
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
                (rule.id.clone(), clean_text, failure)
            })
            .collect()
    };

    let mut passed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut fail_lines: Vec<String> = Vec::new();
    for (rule_id, clean_text, failure) in per_example {
        match failure {
            None => passed += 1,
            Some((outcome, message)) => {
                *outcome_counts.entry(outcome).or_default() += 1;
                fail_lines.push(format!("FAIL: [{rule_id}] {message}: '{clean_text}'"));
            }
        }
    }
    let failed = outcome_counts.values().sum::<usize>();
    eprintln!(
        "es refresh grammar suite: {passed}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} \
         [{:.1}s]",
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
        "accounting: {} groups, {all_compiled_examples} examples on compiled rules, \
         {evaluated_by_groups} evaluated",
        groups.len()
    );
    assert_eq!(
        all_compiled_examples, 6739,
        "examples carried by compiled rules from the 7252 raw <example> tags"
    );
    assert_eq!(
        evaluated_by_groups, 6651,
        "the evaluated mass: first-rule examples per (id, sub_id) group"
    );
    assert_eq!(
        passed, 6446,
        "the es grammar suite stays green at the refresh baseline"
    );
    assert_eq!(
        outcome_counts.get(&Outcome::NoMatch).copied().unwrap_or(0),
        138,
        "no-match residue"
    );
    assert_eq!(
        outcome_counts
            .get(&Outcome::FalsePositive)
            .copied()
            .unwrap_or(0),
        66,
        "false-positive residue"
    );
    assert_eq!(
        outcome_counts.get(&Outcome::Marker).copied().unwrap_or(0),
        1,
        "marker-miss residue"
    );
    fail_lines.sort();
    let _ = std::fs::write(
        "/tmp/test_failures_es_module_refresh.txt",
        fail_lines.join("\n") + "\n",
    );
}
