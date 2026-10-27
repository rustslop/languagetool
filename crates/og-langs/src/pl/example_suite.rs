use crate::pl::test_support::PolishPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn rules_dir() -> PathBuf {
    crate::pl::pl_rules_dir().expect("pl rules dir")
}

fn grammar_path() -> PathBuf {
    rules_dir().join("grammar.xml")
}

pub(crate) fn suite_engine() -> PatternRuleEngine {
    let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
    let core_resource_dir = crate::engine::find_lt_core_resource_dir().expect("core resource dir");
    let dictionary = og_spell::Dictionary::from_shared_words(std::sync::Arc::new(
        og_spell::morfologik_view::MorfologikView::new(
            og_tagger::generic_tagger::shared_dictionary(&resource_dir, "polish")
                .expect("polish tagger dict"),
        ),
    ));
    let support = og_rules::spelling_support::SpellingSupport::new_for_language(
        std::sync::Arc::new(dictionary),
        &core_resource_dir,
        &resource_dir,
        "pl",
    );
    PatternRuleEngine::new().with_spelling_support(std::sync::Arc::new(support))
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

fn element_texts(
    rule: &CompiledRule,
    tokens: &[&og_core::AnalyzedTokenReadings],
    m: &og_core::RuleMatch,
) -> Vec<Option<String>> {
    let end = m.offset() + m.length();
    let overlap: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.token().start() < end && t.token().end() > m.offset())
        .map(|(i, _)| i)
        .collect();
    let mut texts: Vec<Option<String>> = vec![None; rule.pattern.elements.len()];
    let Some(ms) = rule.pattern.marker_elem_start else {
        return overlap
            .iter()
            .map(|&i| Some(tokens[i].token().token().to_string()))
            .collect();
    };
    let me = rule
        .pattern
        .marker_elem_end
        .unwrap_or(rule.pattern.elements.len());
    for (k, &token_idx) in overlap.iter().enumerate() {
        if ms + k < me {
            texts[ms + k] = Some(tokens[token_idx].token().token().to_string());
        }
    }
    let Some(&first) = overlap.first() else {
        return texts;
    };
    let mut left = first;
    let mut k = ms;
    while k > 0 {
        k -= 1;
        let Some(next) = tokens[..left].iter().rposition(|t| !t.is_whitespace()) else {
            break;
        };
        left = next;
        if !matches!(
            &rule.pattern.elements[k],
            og_xml::compiler::CompiledPatternElement::Token(t) if t.min == Some(0)
        ) {
            texts[k] = Some(tokens[left].token().token().to_string());
        }
    }
    texts
}

fn apply_pl_filters(
    rule: &CompiledRule,
    tokens: &[&og_core::AnalyzedTokenReadings],
    m: og_core::RuleMatch,
) -> Option<og_core::RuleMatch> {
    let Some(filter) = rule
        .filter
        .as_ref()
        .filter(|f| crate::pl::filters::is_undispatched_pl_filter(&f.class))
    else {
        return Some(m);
    };
    let texts = element_texts(rule, tokens, &m);
    crate::pl::filters::apply_undispatched_pl_filter(&filter.class, &filter.args, &texts, m)
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
fn polish_grammar_examples_with_pl_pipeline() {
    let _release_guard = crate::release_guard();
    evaluate_examples_file(
        &grammar_path(),
        "grammar.xml",
        "/tmp/test_failures_pl_module.txt",
    );
}

fn evaluate_examples_file(rules_path: &Path, file_label: &str, artifact: &str) {
    let xml = std::fs::read_to_string(rules_path).expect("rules xml");
    let xml = crate::pl::normalize_pl_rules_xml(&xml);
    let xml = xml.as_ref();
    let raw_example_count = xml.matches("<example").count();
    let without_comments = strip_xml_comments(xml);
    let comment_example_count = raw_example_count - without_comments.matches("<example").count();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(xml, rules_path.parent())
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
    let pipeline = PolishPipeline::shared();
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
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                        let tokens = sentence.non_whitespace_tokens();
                        for m in engine.match_rule(aux, &sentence) {
                            let Some(mut m) = apply_pl_filters(aux, &tokens, m) else {
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
        "pl module pipeline [{file_label}]: {}/{} passed ({:.1}%) - no_match: {}, fp: {}, marker: {} [{:.1}s]",
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

    fn compiled_rules_from(path: PathBuf) -> std::sync::Arc<CompiledRuleSet> {
        static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&GRAMMAR, || {
            let xml = std::fs::read_to_string(&path).unwrap();
            let xml = crate::pl::normalize_pl_rules_xml(&xml);
            Some(std::sync::Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(xml.as_ref(), path.parent())
                    .expect("compile rules xml"),
            ))
        })
        .expect("pl rule set")
    }

    fn rule_matches(rules_path: PathBuf, rule_id: &str, text: &str) -> Vec<(usize, usize)> {
        let rules = compiled_rules_from(rules_path);
        let engine = suite_engine();
        let pipeline = PolishPipeline::shared();
        let mut out = Vec::new();
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            let tokens = sentence.non_whitespace_tokens();
            for rule in rules.rules.iter().filter(|r| r.id == rule_id) {
                for m in engine.match_rule(rule, &sentence) {
                    if let Some(m) = apply_pl_filters(rule, &tokens, m) {
                        out.push((m.offset() + sentence_start, m.length()));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn suite_engine_resolves_the_polish_spelling_support() {
        let engine = suite_engine();
        let _ = engine;
    }

    fn rule_matches_of(rules_path: PathBuf, rule_id: &str, text: &str) -> Vec<(usize, usize)> {
        rule_matches(rules_path, rule_id, text)
    }

    fn filtered_rule_matches(
        rule_id: &str,
        filter_class: &str,
        text: &str,
    ) -> Vec<(usize, usize, Vec<String>)> {
        let rules = compiled_rules_from(grammar_path());
        let engine = suite_engine();
        let pipeline = PolishPipeline::shared();
        let mut out = Vec::new();
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            let tokens = sentence.non_whitespace_tokens();
            for rule in rules.rules.iter().filter(|r| {
                r.id == rule_id && r.filter.as_ref().is_some_and(|f| f.class == filter_class)
            }) {
                for m in engine.match_rule(rule, &sentence) {
                    if let Some(m) = apply_pl_filters(rule, &tokens, m) {
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
            }
        }
        out
    }

    #[test]
    fn grammar_carries_the_java_filter_class_inventory() {
        let xml = std::fs::read_to_string(grammar_path()).expect("grammar.xml");
        assert_eq!(
            xml.matches("org.languagetool.rules.pl.DecadeSpellingFilter")
                .count(),
            3
        );
        assert_eq!(
            xml.matches("org.languagetool.rules.pl.DateCheckFilter")
                .count(),
            3
        );
        assert_eq!(
            xml.matches("org.languagetool.rules.DateRangeChecker")
                .count(),
            3
        );
        assert_eq!(
            xml.matches("org.languagetool.rules.ShortenedYearRangeChecker")
                .count(),
            1
        );
        assert_eq!(xml.matches("<filter class=").count(), 10);
    }

    #[test]
    fn data_dekady_suggestions_carry_the_java_decade_and_century() {
        for (text, expected) in [
            (
                "Na pocz\u{105}tku lat 1840. ulica Zachodnia zosta\u{142}a po\u{142}\u{105}czona na p\u{f3}\u{142}nocy z ulic\u{105} Stodolnian\u{105} na Starym Mie\u{15b}cie.",
                "40. XIX w.",
            ),
            ("Wspominam pi\u{119}kne lata 1990.", "90. XX w."),
            (
                "W latach 1770. Smith osiedli\u{142} si\u{119} w miejscowo\u{15b}ci wypoczynkowej Bath.",
                "70. XVIII w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1660.",
                "60. XVII w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1560.",
                "60. XVI w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1460.",
                "60. XV w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1360.",
                "60. XIV w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1260.",
                "60. XIII w.",
            ),
            (
                "Szampan jest wi\u{119}c szybko butelkowany od lat 1160.",
                "60. XII w.",
            ),
            (
                "Lata 1900. charakteryzowa\u{142}y si\u{119} historycznymi zmianami.",
                "pierwsza dekada XX w.",
            ),
            (
                "Lata 1910. charakteryzowa\u{142}y si\u{119} historycznymi zmianami.",
                "druga dekada XX w.",
            ),
        ] {
            let matches = filtered_rule_matches(
                "DATA_DEKADY",
                "org.languagetool.rules.pl.DecadeSpellingFilter",
                text,
            );
            assert_eq!(matches.len(), 1, "{text}");
            assert_eq!(matches[0].2, vec![expected.to_string()], "{text}");
        }
    }

    #[test]
    fn shortened_year_range_drops_the_ranges_java_accepts() {
        let class = "org.languagetool.rules.ShortenedYearRangeChecker";
        assert_eq!(
            filtered_rule_matches(
                "DATA_NIEMOZLIWA",
                class,
                "By\u{142}o to w latach 1855\u{2013}40."
            )
            .len(),
            1,
            "1855 >= 1840 keeps the match (grammar.xml example)"
        );
        assert_eq!(
            filtered_rule_matches("DATA_NIEMOZLIWA", class, "By\u{142}o to w latach 1915-15.")
                .len(),
            1,
            "1915 >= 1915 keeps the match (grammar.xml example)"
        );
        assert!(
            filtered_rule_matches(
                "DATA_NIEMOZLIWA",
                class,
                "By\u{142}o to w latach 1855\u{2013}60."
            )
            .is_empty(),
            "1855 < 1860 is a valid range, Java's ShortenedYearRangeChecker drops it"
        );
    }

    #[test]
    fn polishtest_comma_rules_fire_through_the_pl_disambiguation() {
        let text = "Kupi\u{142}em wino chcia\u{142}em te\u{17c} pi\u{107} wod\u{119}.";
        assert!(
            !rule_matches_of(grammar_path(), "ZDANIA_ZLOZONE", text).is_empty(),
            "the missing-comma example must fire once interp:comma is comma-only: {text}"
        );
        let text = "Kupi\u{142}em wino, chcia\u{142}em te\u{17c} pi\u{107} wod\u{119}.";
        assert!(
            rule_matches_of(grammar_path(), "ZDANIA_ZLOZONE", text).is_empty(),
            "the correct comma form stays clean: {text}"
        );
    }

    #[test]
    fn polishtest_agreement_rules_fire_through_the_pipeline() {
        let text = "Odr\u{f3}\u{17c}ni\u{142} je od poj\u{119}cia solipsyzm metodologicznego.";
        assert!(
            !rule_matches(grammar_path(), "SUBST_ADJ_UNIFY", text).is_empty(),
            "the agreement pattern fires on the corpus example: {text}"
        );
    }

    #[test]
    fn polishtest_correct_agreement_example_stays_clean() {
        let text = "Lubi\u{119} pi\u{142}k\u{119} no\u{17c}n\u{105}.";
        assert!(
            rule_matches(grammar_path(), "ADJ_SUBST_ADJ_UNIFY", text).is_empty(),
            "the agreed example stays clean: {text}"
        );
    }

    #[test]
    fn brak_kropki_punctuation_terminated_sentences_stay_clean() {
        for text in [
            "Po\u{17c}ycz mi sto z\u{142}otych.",
            "To jest zdanie z \u{201e}cytatem na ko\u{144}cu i przypisem\u{201d}.",
            "Po\u{17c}ycz mi sto z\u{142}otych\u{2026}",
        ] {
            assert!(
                rule_matches(grammar_path(), "BRAK_KROPKI", text).is_empty(),
                "the restored \\02 class exception must suppress: {text}"
            );
        }
        assert!(
            !rule_matches(
                grammar_path(),
                "BRAK_KROPKI",
                "Po\u{17c}ycz mi sto z\u{142}otych"
            )
            .is_empty(),
            "the unterminated sentence still fires"
        );
    }

    #[test]
    fn dni_tygodnia_lowercase_days_fire_and_holidays_stay_clean() {
        assert!(
            !rule_matches(
                grammar_path(),
                "DNI_TYGODNIA",
                "To si\u{119} sta\u{142}o w wielki pi\u{105}tek."
            )
            .is_empty(),
            "the lowercase holiday no longer matches the case-sensitive antipattern"
        );
        for text in [
            "To si\u{119} sta\u{142}o w Wielki Pi\u{105}tek.",
            "Poniedzia\u{142}ek to idealny pocz\u{105}tek tygodnia.",
            "To si\u{119} zdarzy\u{142}o w \u{15a}rod\u{119} Popielcow\u{105} albo Niedziel\u{119} Palmow\u{105}, sam nie pami\u{119}tam.",
            "Niebawem b\u{119}dzie Niedziela Mi\u{142}osierdzia Bo\u{17c}ego.",
        ] {
            assert!(
                rule_matches(grammar_path(), "DNI_TYGODNIA", text).is_empty(),
                "the antipatterns must keep protecting: {text}"
            );
        }
        assert!(
            !rule_matches(
                grammar_path(),
                "DNI_TYGODNIA",
                "Robimy dorsza co Pi\u{105}tek."
            )
            .is_empty(),
            "the capitalized weekday still fires"
        );
    }
}

#[cfg(test)]
mod synthetic_evaluator_tests {
    use super::*;

    fn synthetic_rules() -> String {
        [
            "<rules lang=\"pl\">",
            "<rule id=\"SYN_MATCH\" name=\"syn match\">",
            "<pattern><token>gronz</token></pattern>",
            "<message>s\u{142}owo z\u{142}e</message>",
            "<example type=\"incorrect\">To jest <marker>gronz</marker> tutaj.</example>",
            "<example type=\"correct\">To jest dobrze.</example>",
            "</rule>",
            "<rule id=\"SYN_NOMATCH\" name=\"syn nomatch\">",
            "<pattern><token>zzzqqq</token></pattern>",
            "<message>s\u{142}owo z\u{142}e</message>",
            "<example type=\"incorrect\">Nic tu nie ma.</example>",
            "</rule>",
            "<rule id=\"SYN_FP\" name=\"syn fp\">",
            "<pattern><token>To</token></pattern>",
            "<message>s\u{142}owo z\u{142}e</message>",
            "<example type=\"correct\">To jest dobrze.</example>",
            "</rule>",
            "<rule id=\"SYN_MARKER_MISS\" name=\"syn marker miss\">",
            "<pattern><token>przypadek</token></pattern>",
            "<message>s\u{142}owo z\u{142}e</message>",
            "<example type=\"incorrect\">To jest <marker>gronz</marker> przypadek.</example>",
            "</rule>",
            "<!-- <example type=\"correct\">commented</example> -->",
            "</rules>",
        ]
        .join("\n")
    }

    #[test]
    fn evaluate_examples_file_classifies_synthetic_outcomes_into_the_artifact() {
        let path = std::env::temp_dir().join(format!(
            "og_pl_synthetic_grammar_{}.xml",
            std::process::id()
        ));
        std::fs::write(&path, synthetic_rules()).expect("write synthetic rules");
        let artifact_path = std::env::temp_dir().join(format!(
            "og_pl_synthetic_artifact_{}.txt",
            std::process::id()
        ));
        evaluate_examples_file(&path, "synthetic", artifact_path.to_str().unwrap());
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
