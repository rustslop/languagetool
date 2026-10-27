use crate::nl::test_support::DutchPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_rules::postponed_adjective::LangSynthesizer;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn rules_dir() -> PathBuf {
    crate::nl::nl_rules_dir().expect("nl rules dir")
}

fn grammar_path() -> PathBuf {
    rules_dir().join("grammar.xml")
}

fn style_path() -> PathBuf {
    rules_dir().join("style.xml")
}

fn variant_grammar_path() -> PathBuf {
    rules_dir().join("nl-NL/grammar.xml")
}

pub(crate) fn suite_engine() -> PatternRuleEngine {
    let resource_dir = crate::nl::tagger::nl_resource_dir().expect("nl resource dir");
    let core_resource_dir = crate::engine::find_lt_core_resource_dir().expect("core resource dir");
    let dictionary = crate::nl::speller::shared_speller_dictionary()
        .unwrap_or_else(|| Arc::new(og_spell::Dictionary::new()));
    let support = og_rules::spelling_support::SpellingSupport::new_for_language(
        dictionary,
        &core_resource_dir,
        &resource_dir,
        "nl",
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
    let resource_dir = crate::nl::tagger::nl_resource_dir()?;
    LangSynthesizer::for_language(&resource_dir, "nl")
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
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(decode_xml_entities(&rest[..end]))
}

#[derive(Default)]
struct AntipatternExamples {
    rule_level: HashMap<(String, usize), Vec<String>>,
    group_level: HashMap<String, Vec<String>>,
}

fn collect_antipattern_examples(nc: &str) -> AntipatternExamples {
    let mut out = AntipatternExamples::default();
    let mut group: Option<(String, usize)> = None;
    let mut in_rule = false;
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
                group = attr_value(tag, "id").map(|id| (id, 0));
            }
            "rulegroup" => {
                group = None;
                in_rule = false;
                in_antipattern = false;
                example_capture = None;
            }
            "rule" if !tag.starts_with("</") => {
                if let Some((_, count)) = group.as_mut() {
                    *count += 1;
                }
                in_rule = true;
            }
            "rule" => {
                in_rule = false;
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
                    match (&group, in_rule) {
                        (Some((gid, pos)), true) => {
                            out.rule_level
                                .entry((gid.clone(), *pos))
                                .or_default()
                                .push(clean);
                        }
                        (Some((gid, _)), false) => {
                            out.group_level.entry(gid.clone()).or_default().push(clean);
                        }
                        _ => {}
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
fn dutch_grammar_examples_with_nl_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &grammar_path(),
        "grammar.xml",
        "/tmp/test_failures_nl_module.txt",
    );
}

#[test]
#[ignore]
fn dutch_style_examples_with_nl_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &style_path(),
        "style.xml",
        "/tmp/test_failures_nl_style.txt",
    );
}

#[test]
#[ignore]
fn dutch_variant_grammar_examples_with_nl_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &variant_grammar_path(),
        "nl-NL/grammar.xml",
        "/tmp/test_failures_nl_variant.txt",
    );
}

fn evaluate_examples_file(rules_path: &Path, file_label: &str, artifact: &str) {
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

    let antipattern = collect_antipattern_examples(&without_comments);
    let antipattern_rule_level_count: usize = antipattern.rule_level.values().map(Vec::len).sum();
    let antipattern_group_level_count: usize = antipattern.group_level.values().map(Vec::len).sum();

    let engine = suite_engine();
    let pipeline = DutchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut fail_lines: Vec<String> = Vec::new();
    let mut fail_by_rule: HashMap<String, usize> = HashMap::new();
    let started = std::time::Instant::now();

    let collect_matches = |targets: &[&CompiledRule], text: &str| -> usize {
        let mut matches = 0usize;
        for aux in targets {
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                matches += engine.match_rule(aux, &sentence).len();
            }
        }
        matches
    };

    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                    for mut m in engine.match_rule(aux, &sentence) {
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
                                matches.len(),
                            ),
                        ))
                    }
                }
                og_xml::types::XmlExampleType::TriggersError => None,
            };
            match failure {
                None => passed += 1,
                Some((outcome, message)) => {
                    failed += 1;
                    *outcome_counts.entry(outcome).or_default() += 1;
                    *fail_by_rule.entry(rule.id.clone()).or_default() += 1;
                    fail_lines.push(format!("FAIL: [{}] {}: '{}'", rule.id, message, clean_text));
                }
            }
        }
    }

    let mut antipattern_targets: HashMap<(String, Option<String>), &Vec<&CompiledRule>> =
        HashMap::new();
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        let key = (rule.id.clone(), rule.sub_id.clone());
        antipattern_targets.insert(key, group);
    }

    let mut antipattern_failures: Vec<String> = Vec::new();
    for ((gid, pos), texts) in &antipattern.rule_level {
        let Some(targets) = antipattern_targets.get(&(gid.clone(), Some(pos.to_string()))) else {
            antipattern_failures.push(format!(
                "FAIL: [{gid}/{pos} ANTIPATTERN]: target sub-rule missing from compiled set: '{gid}'"
            ));
            continue;
        };
        for text in texts {
            let matches = collect_matches(targets, text);
            if matches == 0 {
                passed += 1;
            } else {
                failed += 1;
                *outcome_counts.entry(Outcome::FalsePositive).or_default() += 1;
                *fail_by_rule.entry(gid.clone()).or_default() += 1;
                antipattern_failures.push(format!(
                    "FAIL: [{gid}/{pos} ANTIPATTERN]: Expected no match (antipattern example) but got {matches}: '{text}'"
                ));
            }
        }
    }
    for (gid, texts) in &antipattern.group_level {
        let sub_groups: Vec<&Vec<&CompiledRule>> = antipattern_targets
            .iter()
            .filter(|((id, _), _)| id == gid)
            .map(|(_, g)| *g)
            .collect();
        if sub_groups.is_empty() {
            antipattern_failures.push(format!(
                "FAIL: [{gid} ANTIPATTERN]: target group missing from compiled set: '{gid}'"
            ));
            continue;
        }
        for text in texts {
            let mut matches = 0usize;
            for group in &sub_groups {
                matches += collect_matches(group, text);
            }
            if matches == 0 {
                passed += 1;
            } else {
                failed += 1;
                *outcome_counts.entry(Outcome::FalsePositive).or_default() += 1;
                *fail_by_rule.entry(gid.clone()).or_default() += 1;
                antipattern_failures.push(format!(
                    "FAIL: [{gid} ANTIPATTERN]: Expected no match (group antipattern example) but got {matches}: '{text}'"
                ));
            }
        }
    }
    fail_lines.extend(antipattern_failures);

    eprintln!(
        "nl module pipeline [{file_label}]: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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
        "accepted-excluded residue [{file_label}]: {raw_example_count} <example> tags ({comment_example_count} inside XML comments), {compiled_example_count} on compiled rules, {} on non-first rules of duplicate (id,sub_id) groups, {evaluated_example_count} evaluated, {antipattern_rule_level_count} rule-level + {antipattern_group_level_count} group-level antipattern examples evaluated as Java correctExamples (PatternRuleHandler.java:858-866)",
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
    let _ = std::fs::write(artifact, fail_lines.join("\n") + "\n");
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
    use og_core::rule::Rule as _;

    fn compiled_rules_from(path: PathBuf) -> std::sync::Arc<CompiledRuleSet> {
        static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        static STYLE: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        static VARIANT: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        let cell = if path.ends_with("style.xml") {
            &STYLE
        } else if path.ends_with("nl-NL/grammar.xml") {
            &VARIANT
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
        .expect("nl rule set")
    }

    fn rule_matches(rules_path: PathBuf, rule_id: &str, text: &str) -> Vec<(usize, usize)> {
        let rules = compiled_rules_from(rules_path);
        let rule = rules
            .rules
            .iter()
            .find(|r| r.id == rule_id)
            .unwrap_or_else(|| panic!("rule {rule_id} not compiled"));
        let engine = suite_engine();
        let pipeline = DutchPipeline::shared();
        let mut out = Vec::new();
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for m in engine.match_rule(rule, &sentence) {
                out.push((m.offset() + sentence_start, m.length()));
            }
        }
        out
    }

    #[test]
    fn antipattern_example_extraction_matches_the_java_parse() {
        let grammar = std::fs::read_to_string(grammar_path()).unwrap();
        let grammar = collect_antipattern_examples(&strip_xml_comments(&grammar));
        let rule_level: usize = grammar.rule_level.values().map(Vec::len).sum();
        let group_level: usize = grammar.group_level.values().map(Vec::len).sum();
        assert_eq!(
            rule_level, 193,
            "grammar.xml rule-level antipattern examples"
        );
        assert_eq!(
            group_level, 20,
            "grammar.xml group-level antipattern examples"
        );
        let style = std::fs::read_to_string(style_path()).unwrap();
        let style = collect_antipattern_examples(&strip_xml_comments(&style));
        let rule_level: usize = style.rule_level.values().map(Vec::len).sum();
        let group_level: usize = style.group_level.values().map(Vec::len).sum();
        assert_eq!(rule_level, 55, "style.xml rule-level antipattern examples");
        assert_eq!(
            group_level, 56,
            "style.xml group-level antipattern examples"
        );
    }

    #[test]
    fn antipattern_example_extraction_decodes_entities_and_markers() {
        let nc = "<rules><rulegroup id=\"G\"><rule><pattern><token/></pattern>\
<antipattern><token>a</token><example>Zo &amp; zo &lt;marker&gt;x&lt;/marker&gt;</example></antipattern>\
<example>plain</example></rule></rulegroup></rules>";
        let out = collect_antipattern_examples(nc);
        assert_eq!(
            out.rule_level.get(&("G".to_string(), 1)).unwrap(),
            &vec!["Zo & zo x".to_string()],
            "the plain <example> after </antipattern> must not be captured"
        );
        assert!(out.group_level.is_empty());
        let nc = "<rules><rulegroup id=\"G\"><antipattern><example>groep</example></antipattern>\
<rule><pattern><token/></pattern></rule></rulegroup></rules>";
        let out = collect_antipattern_examples(nc);
        assert_eq!(
            out.group_level.get("G").unwrap(),
            &vec!["groep".to_string()],
            "antipattern before any sub-rule attaches at group level"
        );
    }

    #[test]
    fn dutchtest_demo_text_rules_fire_through_the_pipeline() {
        let text = "Bij voorbeeld om een collega te vertellen wat er vrijdag gebeurd is.";
        assert!(
            !rule_matches(grammar_path(), "BIJ_VOORBEELD", text).is_empty(),
            "the demo text must trigger BIJ_VOORBEELD"
        );
        let text = "Dit is fout !";
        assert!(
            !rule_matches(grammar_path(), "SPATIE_LEESTEKEN", text).is_empty(),
            "the pinned example must trigger SPATIE_LEESTEKEN on 'fout !'"
        );
        let text = "LanguageTool laat stijlwesties zo optimaal mogelijk zien in het blauw.";
        assert!(
            !rule_matches(style_path(), "ZO_OPTIMAAL_MOGELIJK", text).is_empty(),
            "the demo text must trigger ZO_OPTIMAAL_MOGELIJK"
        );

        let pipeline = DutchPipeline::shared();
        let uppercase = og_rules::native_rules::UppercaseSentenceStartRule::new();
        let demo = "wist u al dat u synoniemen kunt oproepen met een dubbelklik op een woord ?";
        let sentence = pipeline.analyze_range(demo, 0, demo.len());
        assert!(
            !uppercase.match_sentence(&sentence).is_empty(),
            "the demo text must trigger UPPERCASE_SENTENCE_START on 'wist'"
        );

        let simple_replace = crate::nl::simple_replace::dutch_simple_replace_rule();
        let text = "LanguageTool is een absolute musthave.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        let matches = simple_replace.match_sentence(&sentence);
        assert!(
            matches.len() == 1
                && matches[0]
                    .rule()
                    .id()
                    .contains("NL_SIMPLE_REPLACE_MUSTHAVE")
                && matches[0].replacements()[0].value() == "must-have",
            "{matches:?}"
        );
    }
}

#[cfg(test)]
mod synthetic_evaluator_tests {
    use super::*;

    fn write_temp(name: &str, xml: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("og_nl_synthetic_{name}_{}.xml", std::process::id()));
        std::fs::write(&path, xml).expect("write synthetic rules");
        path
    }

    fn synthetic_grammar() -> String {
        [
            "<rules lang=\"nl\">",
            "<rule id=\"SYN_MATCH\" name=\"syn match\">",
            "<pattern><token>gronz</token></pattern>",
            "<message>slecht</message>",
            "<example type=\"incorrect\">Dit is een <marker>gronz</marker> hier.</example>",
            "<example type=\"correct\">Dit is goed.</example>",
            "</rule>",
            "<rule id=\"SYN_NOMATCH\" name=\"syn nomatch\">",
            "<pattern><token>zzzqqq</token></pattern>",
            "<message>slecht</message>",
            "<example type=\"incorrect\">Niets te zien.</example>",
            "</rule>",
            "<rule id=\"SYN_FP\" name=\"syn fp\">",
            "<pattern><token>Dit</token></pattern>",
            "<message>slecht</message>",
            "<example type=\"correct\">Dit is goed.</example>",
            "</rule>",
            "<rule id=\"SYN_MARKER_MISS\" name=\"syn marker miss\">",
            "<pattern><token>geval</token></pattern>",
            "<message>slecht</message>",
            "<example type=\"incorrect\">Dit is <marker>gronz</marker> een geval.</example>",
            "</rule>",
            "<rulegroup id=\"SYN_GROUP\">",
            "<antipattern>",
            "<example type=\"correct\">Groep blorp geval.</example>",
            "</antipattern>",
            "<rule name=\"g1\">",
            "<antipattern>",
            "<example type=\"correct\">Geen enkel woord hiervan.</example>",
            "</antipattern>",
            "<pattern><token>blorp</token></pattern>",
            "<message>slecht</message>",
            "<example type=\"incorrect\">Groep <marker>blorp</marker> geval.</example>",
            "</rule>",
            "</rulegroup>",
            "<!-- <example type=\"correct\">commented</example> -->",
            "</rules>",
        ]
        .join("\n")
    }

    #[test]
    fn evaluate_examples_file_classifies_synthetic_outcomes_and_group_antipatterns() {
        let path = write_temp("grammar", &synthetic_grammar());
        let artifact_path = std::env::temp_dir().join(format!(
            "og_nl_synthetic_grammar_{}.txt",
            std::process::id()
        ));
        evaluate_examples_file(&path, "synthetic", artifact_path.to_str().unwrap());
        let artifact = std::fs::read_to_string(&artifact_path).unwrap_or_default();

        assert!(
            artifact.contains(
                "FAIL: [SYN_NOMATCH] Expected match on incorrect example but got none: 'Niets te zien.'"
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
            artifact.contains(
                "FAIL: [SYN_GROUP ANTIPATTERN]: Expected no match (group antipattern example) but got"
            ),
            "a group-level antipattern example on which a sub-rule still fires must be recorded: {artifact}"
        );
        assert!(
            !artifact.contains("SYN_MATCH"),
            "the rule whose match covers the marker and whose correct example stays clean must pass: {artifact}"
        );
        assert!(
            !artifact.contains("SYN_GROUP/1 ANTIPATTERN"),
            "the rule-level antipattern that stays clean must pass: {artifact}"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&artifact_path);
    }

    #[test]
    fn collect_antipattern_examples_splits_rule_and_group_levels() {
        let nc = strip_xml_comments(&synthetic_grammar());
        let antipattern = collect_antipattern_examples(&nc);
        assert_eq!(
            antipattern
                .rule_level
                .get(&("SYN_GROUP".to_string(), 1))
                .map(Vec::len),
            Some(1),
            "the antipattern inside the sub-rule keys as (group id, position)"
        );
        assert_eq!(
            antipattern.group_level.get("SYN_GROUP").map(Vec::len),
            Some(1),
            "the antipattern before any sub-rule keys as group-level"
        );
    }
}
