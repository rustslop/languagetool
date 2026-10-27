use crate::pt::filters::{PtFilterCall, apply_engine_pt_filter, is_pt_filter};
use crate::pt::test_support::PortuguesePipeline;
use og_core::{AnalyzedSentence, AnalyzedTokenReadings, RuleMatch};
use og_rules::pattern_rule::PatternRuleEngine;
use og_rules::postponed_adjective::LangSynthesizer;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn rules_dir() -> PathBuf {
    crate::pt::filters::pt_rules_dir().expect("pt rules dir")
}

fn grammar_path() -> PathBuf {
    rules_dir().join("grammar.xml")
}

fn style_path() -> PathBuf {
    rules_dir().join("style.xml")
}

pub(crate) fn suite_engine() -> PatternRuleEngine {
    let resource_dir = crate::pt::tagger::pt_resource_dir().expect("pt resource dir");
    let core_resource_dir = crate::engine::find_lt_core_resource_dir().expect("core resource dir");
    let mut dict = og_spell::Dictionary::new();
    for name in ["hunspell/spelling.txt", "added.txt"] {
        if let Ok(words) = std::fs::read_to_string(resource_dir.join(name)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
    }
    let support = og_rules::spelling_support::SpellingSupport::new_for_language(
        Arc::new(dict),
        &core_resource_dir,
        &resource_dir,
        "pt",
    );
    let synth = suite_synthesizer().map(Arc::new);
    let mut engine = PatternRuleEngine::new().with_spelling_support(Arc::new(support));
    if let Some(synth) = synth {
        engine = engine
            .with_synthesizer(synth.clone())
            .with_suggestion_synthesizer(synth);
    }
    engine
}

pub(crate) fn suite_synthesizer() -> Option<LangSynthesizer> {
    let resource_dir = crate::pt::tagger::pt_resource_dir()?;
    LangSynthesizer::for_language(&resource_dir, "pt")
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

fn token_matches(
    token: &og_xml::compiler::CompiledPatternToken,
    atr: &AnalyzedTokenReadings,
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
    element: &og_xml::compiler::CompiledPatternElement,
    atr: &AnalyzedTokenReadings,
) -> bool {
    use og_xml::compiler::CompiledPatternElement;
    match element {
        CompiledPatternElement::Token(t) => token_matches(t, atr),
        CompiledPatternElement::OrGroup(alts) => alts.iter().any(|t| token_matches(t, atr)),
        CompiledPatternElement::AndGroup(all) => all.iter().all(|t| token_matches(t, atr)),
    }
}

fn element_optional(element: &og_xml::compiler::CompiledPatternElement) -> bool {
    use og_xml::compiler::CompiledPatternElement;
    match element {
        CompiledPatternElement::Token(t) => t.min == Some(0),
        CompiledPatternElement::OrGroup(alts) => alts.iter().any(|t| t.min == Some(0)),
        CompiledPatternElement::AndGroup(all) => all.iter().all(|t| t.min == Some(0)),
    }
}

pub(crate) fn matched_window(
    rule: &CompiledRule,
    tokens: &[&AnalyzedTokenReadings],
    rm: &RuleMatch,
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

pub(crate) fn apply_suite_filter(
    rule: &CompiledRule,
    filter: &og_xml::types::XmlFilter,
    sentence: &AnalyzedSentence,
    tokens: &[&AnalyzedTokenReadings],
    synthesizer: Option<&LangSynthesizer>,
    m: RuleMatch,
) -> Option<RuleMatch> {
    if !is_pt_filter(&filter.class) {
        return Some(m);
    }
    let (matched_positions, matched_texts) = matched_window(rule, tokens, &m);
    apply_engine_pt_filter(
        rule,
        filter,
        sentence.text(),
        m,
        PtFilterCall {
            variant: "pt-PT",
            args: &filter.args,
            matched_texts: &matched_texts,
            matched_positions: &matched_positions,
            tokens,
            regexp_groups: None,
            synthesizer,
        },
    )
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
}

type ExampleOutcome = (String, String, Option<(Outcome, String)>);

type GroupExamplePair<'a> = (&'a Vec<&'a CompiledRule>, &'a og_xml::types::XmlExample);

#[test]
#[ignore]
fn portuguese_grammar_examples_with_pt_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &grammar_path(),
        "grammar.xml",
        Some("/tmp/test_failures_pt_module.txt"),
    );
}

#[derive(Default)]
pub(crate) struct StyleAntipatternExamples {
    pub(crate) by_key: HashMap<(String, Option<String>), Vec<String>>,
    skipped_targets: usize,
    rule_level_count: usize,
    group_level_count: usize,
}

fn decode_xml_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        match after.find(';') {
            Some(end) if end <= 10 => {
                let entity = &after[..end];
                match entity {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    _ => {
                        if let Some(hex) = entity
                            .strip_prefix("#x")
                            .or_else(|| entity.strip_prefix("#X"))
                            && let Ok(code) = u32::from_str_radix(hex, 16)
                            && let Some(ch) = char::from_u32(code)
                        {
                            out.push(ch);
                        } else if let Some(dec) = entity.strip_prefix('#')
                            && let Ok(code) = dec.parse::<u32>()
                            && let Some(ch) = char::from_u32(code)
                        {
                            out.push(ch);
                        } else {
                            out.push('&');
                            out.push_str(entity);
                            out.push(';');
                        }
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn tag_name(tag: &str) -> &str {
    let body = tag.strip_prefix('<').unwrap_or(tag);
    let body = body.strip_prefix('/').unwrap_or(body);
    let end = body
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(body.len());
    &body[..end]
}

fn attr_value(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{attr}={quote}");
        if let Some(start) = tag.find(&needle) {
            let rest = &tag[start + needle.len()..];
            let end = rest.find(quote)?;
            return Some(decode_xml_entities(&rest[..end]));
        }
    }
    None
}

pub(crate) fn collect_style_antipattern_examples(nc: &str) -> StyleAntipatternExamples {
    let mut out = StyleAntipatternExamples::default();
    let mut group_id: Option<String> = None;
    let mut group_sub_keys: Vec<(String, Option<String>)> = Vec::new();
    let mut group_examples: Vec<String> = Vec::new();
    let mut in_rule = false;
    let mut rule_key: Option<(String, Option<String>)> = None;
    let mut in_antipattern = false;
    let mut example_capture: Option<String> = None;
    let mut rest = nc;
    while let Some(lt) = rest.find('<') {
        if let Some(cap) = example_capture.as_mut() {
            cap.push_str(&rest[..lt]);
        }
        let tag_end = rest[lt..]
            .find('>')
            .map(|e| lt + e + 1)
            .unwrap_or(rest.len());
        let tag = &rest[lt..tag_end];
        match tag_name(tag) {
            "rulegroup" if !tag.starts_with("</") => {
                group_id = attr_value(tag, "id");
                group_sub_keys.clear();
                group_examples.clear();
            }
            "rulegroup" => {
                if group_id.is_some() && !group_examples.is_empty() {
                    for key in &group_sub_keys {
                        out.by_key
                            .entry(key.clone())
                            .or_default()
                            .extend(group_examples.iter().cloned());
                    }
                    if group_sub_keys.is_empty() {
                        out.skipped_targets += group_examples.len();
                    }
                }
                group_id = None;
                group_sub_keys.clear();
                group_examples.clear();
                in_rule = false;
                rule_key = None;
                in_antipattern = false;
                example_capture = None;
            }
            "rule" if !tag.starts_with("</") => {
                in_rule = true;
                if let Some(gid) = &group_id {
                    group_sub_keys.push((
                        attr_value(tag, "id").unwrap_or_else(|| gid.clone()),
                        Some((group_sub_keys.len() + 1).to_string()),
                    ));
                    let key = group_sub_keys.last().cloned();
                    rule_key = key;
                } else {
                    rule_key = attr_value(tag, "id").map(|id| (id, None));
                }
            }
            "rule" => {
                in_rule = false;
                rule_key = None;
                in_antipattern = false;
                example_capture = None;
            }
            "antipattern" if !tag.starts_with("</") && !tag.ends_with("/>") => {
                in_antipattern = true;
            }
            "antipattern" => {
                in_antipattern = false;
            }
            "example" if in_antipattern && example_capture.is_none() && !tag.starts_with("</") => {
                example_capture = Some(String::new());
            }
            "example" if example_capture.is_some() => {
                if let Some(text) = example_capture.take() {
                    let clean = decode_xml_entities(&text)
                        .replace("<marker>", "")
                        .replace("</marker>", "")
                        .trim()
                        .to_string();
                    if in_rule {
                        if let Some(key) = &rule_key {
                            out.rule_level_count += 1;
                            out.by_key.entry(key.clone()).or_default().push(clean);
                        } else {
                            out.skipped_targets += 1;
                        }
                    } else if group_id.is_some() {
                        out.group_level_count += 1;
                        group_examples.push(clean);
                    } else {
                        out.skipped_targets += 1;
                    }
                }
            }
            _ => {}
        }
        rest = &rest[tag_end..];
    }
    out
}

#[test]
#[ignore]
fn portuguese_style_examples_with_pt_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_style_examples_with_antipatterns(&style_path(), "/tmp/test_failures_pt_style.txt");
}

fn evaluate_style_examples_with_antipatterns(rules_path: &Path, artifact: &str) {
    let xml = std::fs::read_to_string(rules_path).expect("rules xml");
    let without_comments = strip_xml_comments(&xml);
    let antipattern = collect_style_antipattern_examples(&without_comments);
    let antipattern_example_count: usize = antipattern.by_key.values().map(Vec::len).sum();

    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, rules_path.parent())
        .expect("compile rules xml");
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

    let engine = suite_engine();
    let synthesizer = suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut fail_lines: Vec<String> = Vec::new();
    let started = std::time::Instant::now();

    for group in &groups {
        let Some(rule) = group.first() else { continue };
        for example in &rule.examples {
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
            let failure: Option<String> = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some(format!(
                            "Expected match on incorrect example but got none: '{clean_text}'"
                        ))
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_start = first.offset();
                        let match_end = match_start + first.length();
                        if match_start <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some(format!(
                                "Match {match_start}..{match_end} does not contain marker {m_start}..{m_end}: '{clean_text}'"
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
                        Some(format!(
                            "Expected no match on correct example but got {}: '{clean_text}'",
                            matches.len()
                        ))
                    }
                }
                og_xml::types::XmlExampleType::TriggersError => None,
            };
            match failure {
                None => passed += 1,
                Some(message) => {
                    failed += 1;
                    fail_lines.push(format!("FAIL: [{}] {}", rule.id, message));
                }
            }
        }
    }

    let mut antipattern_failures = 0usize;
    let mut antipattern_target_misses = 0usize;
    for ((id, sub_id), texts) in &antipattern.by_key {
        let targets: Vec<&CompiledRule> = groups
            .iter()
            .flatten()
            .copied()
            .filter(|r| &r.id == id && &r.sub_id == sub_id)
            .collect();
        if targets.is_empty() {
            antipattern_target_misses += texts.len();
            continue;
        }
        for text in texts {
            let mut matches = 0usize;
            for aux in &targets {
                for (sentence, _) in pipeline.analyze_offset_text(text) {
                    matches += engine.match_rule(aux, &sentence).len();
                }
            }
            if matches == 0 {
                passed += 1;
            } else {
                failed += 1;
                antipattern_failures += 1;
                fail_lines.push(format!(
                    "FAIL: [{id}/{} ANTIPATTERN]: Expected no match (antipattern example) but got {matches}: '{text}'",
                    sub_id.as_deref().unwrap_or("-")
                ));
            }
        }
    }

    eprintln!(
        "pt module pipeline [style.xml + antipatterns]: {}/{} passed ({:.1}%) - antipattern examples: {} evaluated, {} failed, {} target-missed, {} skipped [{:.1}s]",
        passed,
        passed + failed,
        100.0 * passed as f64 / (passed + failed).max(1) as f64,
        antipattern_example_count,
        antipattern_failures,
        antipattern_target_misses,
        antipattern.skipped_targets,
        started.elapsed().as_secs_f64(),
    );
    let _ = std::fs::write(artifact, fail_lines.join("\n") + "\n");
    assert!(passed + failed > 0, "suite must evaluate examples");
}

#[cfg(test)]
mod antipattern_tests {
    use super::*;

    #[test]
    fn style_antipattern_extraction_counts_match_an_independent_parse() {
        let xml = std::fs::read_to_string(style_path()).expect("style.xml");
        let without_comments = strip_xml_comments(&xml);
        let antipattern = collect_style_antipattern_examples(&without_comments);
        let total: usize = antipattern.by_key.values().map(Vec::len).sum();
        assert_eq!(
            total, 1477,
            "910 rule-level + 273 group-level antipattern examples; the 273 group-level ones expand to every sub-rule key like Java's prepareRule (PatternRuleHandler.java:858-866), so the evaluated total is 1477 (independent count via python re over comment-stripped XML, round 2)"
        );
        assert_eq!(antipattern.rule_level_count, 910);
        assert_eq!(antipattern.group_level_count, 273);
        assert_eq!(
            antipattern.skipped_targets, 0,
            "every antipattern example must attach to a rule or group key"
        );
        assert_eq!(
            antipattern
                .by_key
                .get(&("USAR_UTILIZAR".to_string(), None))
                .map(Vec::len),
            Some(29),
            "USAR_UTILIZAR is a top-level rule carrying 29 antipattern examples under key (id, None)"
        );
        assert!(
            antipattern
                .by_key
                .contains_key(&("INTERNET_ABBREVIATIONS".to_string(), Some("1".to_string()))),
            "anonymous sub-rules of rulegroups must key as (group_id, Some(pos)) so the compiled (id, sub_id) lookup resolves"
        );
        assert_eq!(antipattern.rule_level_count, 910);
        assert_eq!(antipattern.group_level_count, 273);
    }
}

fn evaluate_examples_file(rules_path: &Path, file_label: &str, artifact: Option<&str>) {
    let xml = std::fs::read_to_string(rules_path).expect("rules xml");
    let raw_example_count = xml.matches("<example").count();
    let without_comments = strip_xml_comments(&xml);
    let comment_example_count = raw_example_count - without_comments.matches("<example").count();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, rules_path.parent())
        .expect("compile rules xml");

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

    let engine = suite_engine();
    let synthesizer = suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
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
                                        "Match {}..{} does not contain marker {}..{}",
                                        match_start, match_end, m_start, m_end,
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
                fail_lines.push(format!("FAIL: [{}] {}: '{}'", rule_id, message, clean_text));
                *fail_by_rule.entry(rule_id).or_default() += 1;
            }
        }
    }

    eprintln!(
        "pt module pipeline [{file_label}]: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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
    if file_label == "grammar.xml" {
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
    }
    if let Some(artifact) = artifact {
        let _ = std::fs::write(artifact, fail_lines.join("\n") + "\n");
    }
    assert!(passed + failed > 0, "suite must evaluate examples");
}

pub(crate) fn strip_xml_comments_pub(xml: &str) -> String {
    strip_xml_comments(xml)
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

    fn compiled_rules_from(path: PathBuf) -> std::sync::Arc<CompiledRuleSet> {
        static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        static STYLE: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        let cell = if path.ends_with("style.xml") {
            &STYLE
        } else {
            &GRAMMAR
        };
        og_core::releasable::shared_get_or_build(cell, || {
            let xml = std::fs::read_to_string(&path).unwrap();
            Some(std::sync::Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(&xml, path.parent())
                    .expect("compile rules xml"),
            ))
        })
        .expect("pt rule set")
    }

    fn filtered_rule_matches(
        rules_path: PathBuf,
        rule_id: &str,
        text: &str,
    ) -> Vec<(usize, usize, Vec<String>)> {
        let rules = compiled_rules_from(rules_path);
        let rule = rules
            .rules
            .iter()
            .find(|r| r.id == rule_id)
            .unwrap_or_else(|| panic!("rule {rule_id} not compiled"));
        let engine = suite_engine();
        let synthesizer = suite_synthesizer();
        let pipeline = PortuguesePipeline::shared();
        let mut out = Vec::new();
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
            for m in engine.match_rule(rule, &sentence) {
                let Some(m) = (match rule.filter.as_ref() {
                    Some(filter) => apply_suite_filter(
                        rule,
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
                out.push((
                    m.offset() + sentence_start,
                    m.length(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                ));
            }
        }
        out
    }

    fn pt_filter_rule_matches(
        rules_path: PathBuf,
        rule_id: &str,
        text: &str,
    ) -> (usize, Vec<(usize, usize, Vec<String>)>) {
        let engine = suite_engine();
        let synthesizer = suite_synthesizer();
        let pipeline = PortuguesePipeline::shared();
        let mut raw = 0usize;
        let mut filtered = Vec::new();
        for rule in compiled_rules_from(rules_path).rules.iter() {
            if rule.id != rule_id {
                continue;
            }
            let Some(filter) = rule.filter.as_ref().filter(|f| is_pt_filter(&f.class)) else {
                continue;
            };
            let bare = {
                let mut b = rule.clone();
                b.filter = None;
                b
            };
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                raw += engine.match_rule(&bare, &sentence).len();
                let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
                for m in engine.match_rule(rule, &sentence) {
                    let Some(m) = apply_suite_filter(
                        rule,
                        filter,
                        &sentence,
                        &tokens,
                        synthesizer.as_ref(),
                        m,
                    ) else {
                        continue;
                    };
                    filtered.push((
                        m.offset() + sentence_start,
                        m.length(),
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect(),
                    ));
                }
            }
        }
        (raw, filtered)
    }

    #[test]
    fn suite_filter_drives_the_style_synthesizer_rule_end_to_end() {
        let matches = filtered_rule_matches(
            style_path(),
            "TER_PARTICIPIO-PASSADO_V2",
            "Eu tenho pesquisado o assunto há décadas.",
        );
        assert!(
            matches
                .iter()
                .any(|(_, _, reps)| reps.iter().any(|r| r == "pesquiso")),
            "lemmaFrom:2 (pesquisado→pesquisar) with postagFrom:1 (tenho→VMIP1S0) must synthesize 'pesquiso': {matches:?}"
        );
    }

    #[test]
    fn suite_filter_pairs_regexp_groups_for_the_toponym_rule() {
        let matches = filtered_rule_matches(
            grammar_path(),
            "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PARENTESES",
            "Nasceu em Niterói (RJ).",
        );
        assert_eq!(
            matches,
            vec![(18, 5, vec!["\u{2013}RJ".to_string()])],
            "the marked group 2 covers the optional space plus '(RJ)'"
        );
    }

    #[test]
    fn suite_filter_nulls_the_ten_diacritics_runner_false_positives() {
        for text in [
            "Alguém fabrica aves que voam por entre as labaredas de azul.",
            "Ninguém nos deu uma oportunidade.",
            "Ninguém sabia disso a não ser o Tom.",
            "Por isso a alma dele sofre.",
            "Qualquer um seria melhor do que ninguém.",
            "Enfim, na iminência da derrota total, teve ainda o desplante de declarar que nada ficaria na mesma depois do Apito, pois tinha conseguido incomodar e assustar... os inocentes.",
            "Mas pq diabos uma figura dessas seria agora plasmada à imagem de um (ex)juiz em manifestações de rua?",
            "Ninguém sabia nada sobre isso.",
            "Uma pedra nos servia de assento.",
            "Você nos incomoda com sua música.",
        ] {
            let (raw, filtered) = pt_filter_rule_matches(grammar_path(), "DIACRITICS", text);
            assert!(raw > 0, "the engine must fire raw on '{text}'");
            assert!(
                filtered.is_empty(),
                "ConfusionCheckFilter must null the correct example '{text}': {filtered:?}"
            );
        }
    }

    #[test]
    fn suite_filter_nulls_the_eight_separadores_runner_false_positives() {
        let cases = [
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PARENTESES",
                "O candidato a prefeito Fernando Gabeira (RJ).",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Adaptador 135W AC com cabo de 3 pinos.",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Autônomo de Saneamento de Pelotas -PROCAP",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Cidade Alerta GO.",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nasceu em Niterói–RJ.",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nativo de Ooga Booga do Foo Bar/RJ.",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nativo de Oogabooga/RJ.",
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Foram 157 km2 de mata derrubados em nove Estados (AC, AM, AP, MA, MT, PA, TO, RO e RR).",
            ),
        ];
        for (id, text) in cases {
            let (raw, filtered) = pt_filter_rule_matches(grammar_path(), id, text);
            assert!(raw > 0, "the engine must fire raw on '{text}' ({id})");
            assert!(
                filtered.is_empty(),
                "BrazilianToponymFilter must null the correct example '{text}': {filtered:?}"
            );
        }
    }

    #[test]
    fn suite_filter_nulls_the_two_irregular_participle_runner_false_positives() {
        for text in [
            "Depois de ter discretamente limpado o carro, foi desculpado.",
            "No totalitarismo, as liberdades do indivíduo eram suprimidas e o povo era subordinado ao poder sem limites do …",
        ] {
            let (raw, filtered) =
                pt_filter_rule_matches(grammar_path(), "IRREGULAR_PAST_PARTICIPLES", text);
            assert!(raw > 0, "the engine must fire raw on '{text}'");
            assert!(
                filtered.is_empty(),
                "RegularIrregularParticipleFilter must null the correct example '{text}': {filtered:?}"
            );
        }
    }

    #[test]
    fn url_inicial_matches_through_the_global_disambiguation_stage() {
        let matches = filtered_rule_matches(
            grammar_path(),
            "URL_INICIAL",
            "google.com.br é um excelente recurso.",
        );
        assert!(
            matches
                .iter()
                .any(|(start, len, _)| *start == 0 && *len == 13),
            "disambiguation-global.xml must add _IS_URL so URL_INICIAL fires on the sentence-initial URL: {matches:?}"
        );
    }
}

#[cfg(test)]
mod synthetic_evaluator_tests {
    use super::*;

    fn write_temp(name: &str, xml: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("og_pt_synthetic_{name}_{}.xml", std::process::id()));
        std::fs::write(&path, xml).expect("write synthetic rules");
        path
    }

    fn synthetic_grammar() -> String {
        [
            "<rules lang=\"pt\">",
            "<rule id=\"SYN_MATCH\" name=\"syn match\">",
            "<pattern><token>gronz</token></pattern>",
            "<message>palavra ruim</message>",
            "<example type=\"incorrect\">Isto \u{e9} um <marker>gronz</marker> aqui.</example>",
            "<example type=\"correct\">Isto \u{e9} correto.</example>",
            "</rule>",
            "<rule id=\"SYN_NOMATCH\" name=\"syn nomatch\">",
            "<pattern><token>zzzqqq</token></pattern>",
            "<message>palavra ruim</message>",
            "<example type=\"incorrect\">Nada aqui.</example>",
            "</rule>",
            "<rule id=\"SYN_FP\" name=\"syn fp\">",
            "<pattern><token>Isto</token></pattern>",
            "<message>palavra ruim</message>",
            "<example type=\"correct\">Isto \u{e9} correto.</example>",
            "</rule>",
            "<rule id=\"SYN_MARKER_MISS\" name=\"syn marker miss\">",
            "<pattern><token>caso</token></pattern>",
            "<message>palavra ruim</message>",
            "<example type=\"incorrect\">Isto \u{e9} <marker>gronz</marker> um caso.</example>",
            "</rule>",
            "<!-- <example type=\"correct\">commented</example> -->",
            "</rules>",
        ]
        .join("\n")
    }

    fn synthetic_style() -> String {
        [
            "<rules lang=\"pt\">",
            "<rule id=\"SYN_STYLE_MATCH\" name=\"syn style match\">",
            "<antipattern>",
            "<example type=\"correct\">Palavra flurb proibida aqui.</example>",
            "</antipattern>",
            "<pattern><token>flurb</token></pattern>",
            "<message>ruim</message>",
            "<example type=\"incorrect\">Aqui vem <marker>flurb</marker> mesmo.</example>",
            "</rule>",
            "<rulegroup id=\"SYN_STYLE_GROUP\">",
            "<antipattern>",
            "<example type=\"correct\">Grupo blorp caso.</example>",
            "</antipattern>",
            "<rule name=\"g1\"><pattern><token>blorp</token></pattern><message>ruim</message>",
            "<example type=\"incorrect\">Grupo <marker>blorp</marker> caso.</example></rule>",
            "</rulegroup>",
            "<rule id=\"SYN_STYLE_CLEAN\" name=\"syn style clean\">",
            "<antipattern>",
            "<example type=\"correct\">Este texto nenhuma palavra da regra tem.</example>",
            "</antipattern>",
            "<pattern><token>querqu</token></pattern>",
            "<message>ruim</message>",
            "<example type=\"incorrect\">Aqui vem <marker>querqu</marker>.</example>",
            "</rule>",
            "</rules>",
        ]
        .join("\n")
    }

    #[test]
    fn evaluate_examples_file_classifies_synthetic_outcomes_into_the_artifact() {
        let path = write_temp("grammar", &synthetic_grammar());
        let artifact_path = std::env::temp_dir().join(format!(
            "og_pt_synthetic_grammar_{}.txt",
            std::process::id()
        ));
        evaluate_examples_file(
            &path,
            "synthetic-grammar",
            Some(artifact_path.to_str().unwrap()),
        );
        let artifact = std::fs::read_to_string(&artifact_path).unwrap_or_default();

        assert!(
            artifact.contains(
                "FAIL: [SYN_NOMATCH] Expected match on incorrect example but got none: 'Nada aqui.'"
            ),
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

    #[test]
    fn evaluate_style_examples_reports_rule_and_group_antipattern_fires() {
        let path = write_temp("style", &synthetic_style());
        let artifact_path =
            std::env::temp_dir().join(format!("og_pt_synthetic_style_{}.txt", std::process::id()));
        evaluate_style_examples_with_antipatterns(&path, artifact_path.to_str().unwrap());
        let artifact = std::fs::read_to_string(&artifact_path).unwrap_or_default();

        assert!(
            artifact.contains(
                "FAIL: [SYN_STYLE_MATCH/- ANTIPATTERN]: Expected no match (antipattern example)"
            ),
            "a rule-level antipattern example that still fires must be recorded: {artifact}"
        );
        assert!(
            artifact.contains(
                "FAIL: [SYN_STYLE_GROUP/1 ANTIPATTERN]: Expected no match (antipattern example)"
            ),
            "group-level antipatterns expand to the (group id, sub id) keys of their sub-rules: {artifact}"
        );
        assert!(
            !artifact.contains("SYN_STYLE_CLEAN"),
            "a non-firing rule with correct examples and a clean antipattern must pass: {artifact}"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[test]
    fn collect_style_antipattern_examples_keys_synthetic_rules_and_groups() {
        let nc = strip_xml_comments(&synthetic_style());
        let antipattern = collect_style_antipattern_examples(&nc);
        assert_eq!(
            antipattern
                .by_key
                .get(&("SYN_STYLE_MATCH".to_string(), None))
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            antipattern
                .by_key
                .get(&("SYN_STYLE_GROUP".to_string(), Some("1".to_string())))
                .map(Vec::len),
            Some(1),
            "the group-level antipattern must attach to the anonymous sub-rule key (group id, position)"
        );
        assert_eq!(antipattern.rule_level_count, 2);
        assert_eq!(antipattern.group_level_count, 1);
    }

    #[test]
    fn decode_xml_entities_decodes_named_and_numeric_references() {
        assert_eq!(decode_xml_entities("sem ampersand"), "sem ampersand");
        assert_eq!(
            decode_xml_entities("a &amp; b &lt;c&gt; &quot;d&quot; &apos;"),
            "a & b <c> \"d\" '"
        );
        assert_eq!(decode_xml_entities("&#65;&#x42;"), "AB");
        assert_eq!(decode_xml_entities("a &unknownent; b"), "a &unknownent; b");
        assert_eq!(decode_xml_entities("a & unfinished"), "a & unfinished");
        assert_eq!(
            decode_xml_entities("&verylongentityname;"),
            "&verylongentityname;"
        );
    }

    #[test]
    fn tag_name_and_attr_value_parse_single_quoted_and_missing_attributes() {
        assert_eq!(tag_name("<rule id=\"x\">"), "rule");
        assert_eq!(tag_name("</rule>"), "rule");
        assert_eq!(tag_name("<rule/>"), "rule");
        assert_eq!(tag_name("<token inflected='yes'>"), "token");
        assert_eq!(
            attr_value("<rule id=\"x\" off=\"yes\">", "id").as_deref(),
            Some("x")
        );
        assert_eq!(
            attr_value("<rule id='single'>", "id").as_deref(),
            Some("single")
        );
        assert_eq!(attr_value("<rule>", "id"), None);
        assert_eq!(
            attr_value("<rule id=\"a&amp;b\">", "id").as_deref(),
            Some("a&b")
        );
    }

    #[test]
    fn strip_xml_comments_drops_full_comments_and_an_unterminated_tail() {
        assert_eq!(strip_xml_comments("a<!-- x -->b<!-- y -->c"), "abc");
        assert_eq!(strip_xml_comments("x<!-- never closed"), "x");
        assert_eq!(strip_xml_comments("plain"), "plain");
    }
}
