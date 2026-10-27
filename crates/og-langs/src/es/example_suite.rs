use crate::es::test_support::SpanishPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::XmlCompiler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn grammar_path() -> PathBuf {
    PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/grammar.xml",
    )
}

fn style_path() -> PathBuf {
    PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/style.xml",
    )
}

fn build_pattern_engine() -> PatternRuleEngine {
    let resource_dir = crate::es::es_resource_dir().expect("es resource dir");
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
        "es",
    );
    let synth = es_synth();
    let mut engine = PatternRuleEngine::new().with_spelling_support(Arc::new(support));
    if let Some(synth) = synth {
        engine = engine
            .with_synthesizer(synth.clone())
            .with_suggestion_synthesizer(synth);
    }
    engine
}

fn es_synth() -> Option<Arc<og_rules::postponed_adjective::LangSynthesizer>> {
    static CACHE: og_core::releasable::Releasable<og_rules::postponed_adjective::LangSynthesizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        crate::es::es_resource_dir()
            .and_then(|dir| {
                og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
            })
            .map(std::sync::Arc::new)
    })
}

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
    element: &og_xml::compiler::CompiledPatternElement,
    atr: &og_core::AnalyzedTokenReadings,
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
    Passed,
    NoMatch,
    FalsePositive,
    Marker,
}

type GroupExamplePair<'a> = (
    &'a Vec<&'a og_xml::compiler::CompiledRule>,
    &'a og_xml::types::XmlExample,
);

#[test]
#[ignore]
fn spanish_grammar_examples_with_es_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &grammar_path(),
        "grammar",
        "/tmp/test_failures_es_module.txt",
    );
}

#[test]
fn dispatch_drives_the_compiled_comma_rule_end_to_end() {
    let xml = std::fs::read_to_string(grammar_path()).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, grammar_path().parent())
        .expect("compile");
    let rule = rule_set
        .rules
        .iter()
        .find(|r| {
            r.id == "COMMA_MARCADOR_DISCURSIVO"
                && r.filter
                    .as_ref()
                    .is_some_and(|f| f.class.ends_with("AddCommasFilter"))
        })
        .expect("COMMA_MARCADOR_DISCURSIVO rule carrying AddCommasFilter");
    let engine = build_pattern_engine();
    let pipeline = SpanishPipeline::shared();
    let text = "En primer lugar hablaremos de un tema importante.";
    for (sentence, _start) in pipeline.analyze_offset_text(text) {
        let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
        if let Some(m) = engine.match_rule(rule, &sentence).into_iter().next() {
            let out = apply_es_filters(rule, &tokens, sentence.text(), m).expect("match kept");
            let values: Vec<String> = out
                .replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect();
            assert_eq!(values, vec!["lugar,".to_string()]);
            assert_eq!(&text[out.offset()..out.offset() + out.length()], "lugar");
            return;
        }
    }
    panic!("no match for {text}");
}

#[test]
#[ignore]
fn spanish_style_examples_with_es_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &style_path(),
        "style",
        "/tmp/test_failures_es_style_module.txt",
    );
}

fn evaluate_examples_file(path: &std::path::Path, label: &str, artifact: &str) {
    let xml = std::fs::read_to_string(path).expect("rules file");
    let raw_example_count = xml.matches("<example").count();
    let stripped = strip_xml_comments(&xml);
    let commented_example_count = raw_example_count - stripped.matches("<example").count();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, path.parent())
        .expect("compile rules file");

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

    let compiled_example_count: usize = groups
        .iter()
        .map(|g| g.first().map(|r| r.examples.len()).unwrap_or(0))
        .sum();

    let engine = build_pattern_engine();
    let pipeline = SpanishPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let started = std::time::Instant::now();

    let pairs: Vec<GroupExamplePair> = groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    let per_example: Vec<(String, String, bool, Outcome, String)> = {
        use rayon::prelude::*;
        pairs
            .par_iter()
            .map(|(group, example)| {
                let rule = group[0];
                let (clean_text, marker_range) = extract_marker(&example.text);
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
                let (ok, outcome, message) = match example.example_type {
                    og_xml::types::XmlExampleType::Incorrect => {
                        if matches.is_empty() {
                            (
                                false,
                                Outcome::NoMatch,
                                "Expected match on incorrect example but got none".to_string(),
                            )
                        } else if let Some((m_start, m_end)) = marker_range {
                            let first = &matches[0];
                            let match_start = first.offset();
                            let match_end = match_start + first.length();
                            if match_start <= m_start && match_end >= m_end {
                                (true, Outcome::Passed, String::new())
                            } else {
                                (
                                    false,
                                    Outcome::Marker,
                                    format!(
                                        "Match {}..{} does not contain marker {}..{}",
                                        match_start, match_end, m_start, m_end
                                    ),
                                )
                            }
                        } else {
                            (true, Outcome::Passed, String::new())
                        }
                    }
                    og_xml::types::XmlExampleType::Correct => {
                        if matches.is_empty() {
                            (true, Outcome::Passed, String::new())
                        } else {
                            (
                                false,
                                Outcome::FalsePositive,
                                format!(
                                    "Expected no match on correct example but got {}",
                                    matches.len()
                                ),
                            )
                        }
                    }
                    og_xml::types::XmlExampleType::TriggersError => {
                        (true, Outcome::Passed, String::new())
                    }
                };
                (rule.id.clone(), clean_text, ok, outcome, message)
            })
            .collect()
    };

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut fail_lines: Vec<String> = Vec::new();

    for (rule_id, clean_text, ok, outcome, message) in per_example {
        if ok {
            passed += 1;
        } else {
            failed += 1;
            *outcome_counts.entry(outcome).or_default() += 1;
            fail_lines.push(format!("FAIL: [{}] {}: '{}'", rule_id, message, clean_text));
        }
    }

    eprintln!(
        "es module {label} pipeline: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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
    let all_compiled_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    let non_first_examples = all_compiled_examples - compiled_example_count;
    eprintln!(
        "accepted-excluded residue: {raw_example_count} <example> tags in {label} rules ({commented_example_count} inside XML comments), {all_compiled_examples} on compiled rules, {non_first_examples} on non-first rules of duplicate (id,sub_id) groups, {compiled_example_count} evaluated",
    );
    let _ = std::fs::write(artifact, fail_lines.join("\n") + "\n");
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
mod synthetic_evaluator_tests {
    use super::*;

    fn write_temp(name: &str, xml: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("og_es_synthetic_{name}_{}.xml", std::process::id()));
        std::fs::write(&path, xml).expect("write synthetic rules");
        path
    }

    fn synthetic_rules() -> String {
        [
            "<rules lang=\"es\">",
            "<rule id=\"SYN_MATCH\" name=\"syn match\">",
            "<pattern><token>gronz</token></pattern>",
            "<message>palabra mala</message>",
            "<example type=\"incorrect\">Esto es un <marker>gronz</marker> aqu\u{ed}.</example>",
            "<example type=\"correct\">Esto es correcto.</example>",
            "</rule>",
            "<rule id=\"SYN_NOMATCH\" name=\"syn nomatch\">",
            "<pattern><token>zzzqqq</token></pattern>",
            "<message>palabra mala</message>",
            "<example type=\"incorrect\">Nada que ver.</example>",
            "</rule>",
            "<rule id=\"SYN_FP\" name=\"syn fp\">",
            "<pattern><token>Esto</token></pattern>",
            "<message>palabra mala</message>",
            "<example type=\"correct\">Esto es correcto.</example>",
            "</rule>",
            "<rule id=\"SYN_MARKER_MISS\" name=\"syn marker miss\">",
            "<pattern><token>caso</token></pattern>",
            "<message>palabra mala</message>",
            "<example type=\"incorrect\">Esto es <marker>gronz</marker> un caso.</example>",
            "</rule>",
            "<!-- <example type=\"correct\">commented out</example> -->",
            "</rules>",
        ]
        .join("\n")
    }

    #[test]
    fn evaluate_examples_file_classifies_synthetic_outcomes_into_the_artifact() {
        let path = write_temp("classify", &synthetic_rules());
        let artifact_path = std::env::temp_dir().join(format!(
            "og_es_synthetic_artifact_{}.txt",
            std::process::id()
        ));
        evaluate_examples_file(&path, "synthetic", artifact_path.to_str().unwrap());
        let artifact = std::fs::read_to_string(&artifact_path).unwrap_or_default();

        assert!(
            artifact.contains(
                "FAIL: [SYN_NOMATCH] Expected match on incorrect example but got none: 'Nada que ver.'"
            ),
            "the unmatched incorrect example must be recorded as a NoMatch failure: {artifact}"
        );
        assert!(
            artifact.contains("FAIL: [SYN_FP] Expected no match on correct example but got"),
            "the firing rule on a correct example must be recorded as a false positive: {artifact}"
        );
        assert!(
            artifact.contains("FAIL: [SYN_MARKER_MISS] Match ")
                && artifact.contains("does not contain marker"),
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
    fn extract_marker_reports_byte_offsets_and_strips_the_tags() {
        let (clean, range) = extract_marker("Esto es un <marker>gronz</marker> aqu\u{ed}.");
        assert_eq!(clean, "Esto es un gronz aqu\u{ed}.");
        let (start, end) = range.expect("marker range");
        assert_eq!(&clean[start..end], "gronz");

        let (clean, range) = extract_marker("sin marker");
        assert_eq!(clean, "sin marker");
        assert_eq!(range, None);

        let (clean, range) = extract_marker("only <marker>open");
        assert_eq!(clean, "only <marker>open");
        assert_eq!(range, None);
    }

    #[test]
    fn strip_xml_comments_drops_full_comments_and_keeps_a_trailing_rest() {
        let xml = "a<!-- one -->b<!-- two -->c";
        assert_eq!(strip_xml_comments(xml), "abc");
        let unterminated = "x<!-- never closed";
        assert_eq!(strip_xml_comments(unterminated), "x");
        let none = "plain";
        assert_eq!(strip_xml_comments(none), "plain");
    }
}
