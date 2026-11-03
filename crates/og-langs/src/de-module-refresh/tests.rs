use crate::engine::{LanguageEngine, find_lt_resource_dir};
use og_core::RuleMatch;
use std::sync::Arc;

fn shared_engine() -> Arc<LanguageEngine> {
    LanguageEngine::shared_for_code("de").expect("german engine")
}

fn sub_id(m: &RuleMatch) -> Option<String> {
    serde_json::to_value(m).ok().and_then(|v| {
        v["rule"]
            .get("subId")
            .and_then(|s| s.as_str())
            .map(str::to_string)
    })
}

fn leerzeichen_matches(
    engine: &LanguageEngine,
    text: &str,
) -> Vec<(Option<String>, usize, String)> {
    engine
        .check(text)
        .matches
        .iter()
        .filter(|m| m.rule().id() == "LEERZEICHEN_VOR_KLAMMER")
        .map(|m| {
            (
                sub_id(m),
                m.offset(),
                m.replacements()
                    .first()
                    .map(|r| r.value().to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn rule_matches(engine: &LanguageEngine, text: &str, id: &str) -> Vec<(usize, String)> {
    engine
        .check(text)
        .matches
        .iter()
        .filter(|m| m.rule().id() == id)
        .map(|m| {
            (
                m.offset(),
                m.replacements()
                    .first()
                    .map(|r| r.value().to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn compiled_de_rules() -> Vec<og_xml::compiler::CompiledRule> {
    let lt_root = find_lt_resource_dir().expect("languagetool resource dir");
    let path = lt_root.join("de/src/main/resources/org/languagetool/rules/de/grammar.xml");
    let xml = std::fs::read_to_string(&path).expect("de grammar.xml readable");
    og_xml::XmlCompiler::new()
        .compile_file(&xml)
        .expect("de grammar.xml compiles")
        .rules
}

#[test]
fn leerzeichen_rulegroup_compiles_into_two_sub_rules_with_their_examples() {
    let rules = compiled_de_rules();
    let subs: Vec<(&og_xml::compiler::CompiledRule, &str)> = rules
        .iter()
        .filter(|r| r.id == "LEERZEICHEN_VOR_KLAMMER")
        .filter_map(|r| r.sub_id.as_deref().map(|s| (r, s)))
        .collect();
    assert_eq!(
        subs.len(),
        2,
        "the rewritten rulegroup must compile to exactly its two sub-rules"
    );
    let (general, mw) = (subs[0].0, subs[1].0);
    assert_eq!(subs[0].1, "1", "general case keeps the first sub-rule slot");
    assert_eq!(subs[1].1, "2", "the job-ad rule is the second sub-rule");
    assert_eq!(
        general.examples.len(),
        1,
        "general case carries the old single-rule example"
    );
    assert_eq!(
        mw.examples.len(),
        2,
        "the job-ad sub-rule adds one incorrect and one correct example"
    );
    assert!(
        matches!(
            mw.examples[0].example_type,
            og_xml::types::XmlExampleType::Incorrect
        ) && matches!(
            mw.examples[1].example_type,
            og_xml::types::XmlExampleType::Correct
        ),
        "job-ad examples must stay incorrect-then-correct as in grammar.xml"
    );
    assert_eq!(
        mw.examples[0].corrections,
        vec!["("],
        "the job-ad incorrect example corrects the '(' to ' (' (XML attribute \
         normalization trims the suggestion like in Java)"
    );
}

#[test]
fn leerzeichen_general_case_keeps_its_match_shape() {
    let engine = shared_engine();
    let matches = leerzeichen_matches(&engine, "Ich komme aus Altona(Hamburg).");
    assert_eq!(
        matches,
        vec![(Some("1".to_string()), 20, " (".to_string())],
        "the general-case sub-rule must flag the missing space with its old suggestion"
    );
    assert!(
        leerzeichen_matches(&engine, "Ich komme aus Altona (Hamburg).").is_empty(),
        "the spaced form must stay clean"
    );
}

#[test]
fn leerzeichen_job_ad_brackets_flag_on_the_second_sub_rule() {
    let engine = shared_engine();
    for text in [
        "Wir suchen Lehrer(m/w/d) für eine Schule.",
        "Wir suchen Lehrer(m/w) für eine Schule.",
        "Wir suchen Pförtner(m/w/x) für eine Schule.",
    ] {
        let bracket = text.find('(').expect("test text contains '('");
        let matches = leerzeichen_matches(&engine, text);
        assert_eq!(
            matches,
            vec![(Some("2".to_string()), bracket, " (".to_string())],
            "{text}: the job-ad sub-rule must own the match with subId 2"
        );
    }
    for text in [
        "Wir suchen Lehrer (m/w/d) für eine Schule.",
        "Wir suchen Lehrer (m/w) für eine Schule.",
    ] {
        assert!(
            leerzeichen_matches(&engine, text).is_empty(),
            "{text}: spaced job-ad brackets must stay clean"
        );
    }
}

#[test]
fn leerzeichen_job_ad_four_segment_markers_stay_silent_like_java() {
    let engine = shared_engine();
    for text in [
        "Wir suchen Lehrer(m/w/d/x) für eine Schule.",
        "Wir suchen Lehrer(m/f/d/x) für eine Schule.",
    ] {
        assert!(
            leerzeichen_matches(&engine, text).is_empty(),
            "{text}: Java does not flag 4-segment gender markers either, despite the rule \
             name '(m/w), (m/w/d), (m/w/d/x) in job ads'. For a minOccurrence==0 element \
             Java scans the later elements at the same position and skips exclusively — \
             minOccurSkip++ then break, never exploring the consume branch even when the \
             optional token itself matches (AbstractPatternRulePerformer.java:125-143); \
             the element loop breaks on the first non-match with no backtracking and only \
             reports at matchingTokens == patternSize (:167-171). The skip cascade drops \
             both trailing optional '/[mwdfx]' pairs onto the second slash, so the ')' \
             element dies at the third slash. pattern_rule.rs's min=0 fast path plus \
             later_element_matches_at reproduces this exactly; adding consume-branch \
             exploration here would diverge from Java, not fix a divergence"
        );
    }
}

#[test]
fn gruende_postag_alternative_compiles_without_the_trailing_space() {
    let rules = compiled_de_rules();
    let gruende: Vec<&og_xml::compiler::CompiledRule> =
        rules.iter().filter(|r| r.id == "GRUENDE").collect();
    assert!(!gruende.is_empty(), "GRUENDE must stay in the live grammar");
    for rule in gruende {
        let postag_tokens: Vec<&og_xml::compiler::CompiledPatternToken> = rule
            .pattern
            .tokens
            .iter()
            .filter(|t| t.postag.is_some())
            .collect();
        assert!(
            postag_tokens
                .iter()
                .all(|t| !t.postag.as_deref().unwrap_or("").ends_with(' ')),
            "the old 'KON:UNT|ADV:INR ' trailing space made the ADV:INR alternative \
             unmatchable; it must stay fixed"
        );
        assert!(
            postag_tokens
                .iter()
                .any(|t| t.postag.as_deref() == Some("KON:UNT|ADV:INR")),
            "the GRUENDE postag regex must be exactly the fixed 'KON:UNT|ADV:INR'"
        );
    }
}

#[test]
fn gruende_flags_the_gruende_interrogative_continuation() {
    let engine = shared_engine();
    let matches = rule_matches(&engine, "Was sind die gründe, warum das so ist?", "GRUENDE");
    assert_eq!(
        matches,
        vec![(13, "Gründe".to_string())],
        "the ADV:INR alternative is live after the trailing-space fix; 'warum' carries the \
         ADV:INR reading, so this continuation only matches with the fixed postag regex"
    );
}

#[test]
fn gruende_still_flags_the_subordinate_conjunction_continuation() {
    let engine = shared_engine();
    let matches = rule_matches(
        &engine,
        "Was sind die gründe dafür, dass du das getan hast?",
        "GRUENDE",
    );
    assert_eq!(
        matches,
        vec![(13, "Gründe".to_string())],
        "the KON:UNT continuation from the grammar.xml example must keep matching"
    );
}

#[test]
fn case_rule_exceptions_accept_the_new_regex_entries() {
    let engine = shared_engine();
    for text in [
        "Sie engagierte sich in der Neuen Frauenbewegung.",
        "Das Sozialistische Büro publizierte Flugblätter.",
        "Die Eigene Stelle wurde geschaffen.",
        "Unter den Gästen waren Adelige aus Bayern.",
        "Der Adelige vermachte sein Vermögen.",
    ] {
        assert!(
            rule_matches(&engine, text, "DE_CASE").is_empty(),
            "{text}: the case_rule_exceptions.txt entry must suppress DE_CASE"
        );
    }
}

#[test]
fn case_rule_exceptions_do_not_bleed_into_unlisted_phrases() {
    let engine = shared_engine();
    let matches = rule_matches(
        &engine,
        "Sie engagierte sich in der Neuen Arbeiterbewegung.",
        "DE_CASE",
    );
    assert_eq!(
        matches,
        vec![(27, "neuen".to_string())],
        "'Neue[nr]? Frauenbewegung(en)?' must not whitelist other movements"
    );
    let matches = rule_matches(
        &engine,
        "Das Sozialistische Amt publizierte Flugblätter.",
        "DE_CASE",
    );
    assert_eq!(
        matches,
        vec![(4, "sozialistische".to_string())],
        "'Sozialistische[smn]? Büro(s)?' must not whitelist other institutions"
    );
    let matches = rule_matches(&engine, "Die Eigene Abteilung wurde geschaffen.", "DE_CASE");
    assert_eq!(
        matches,
        vec![(4, "eigene".to_string())],
        "'Eigene[nrsm]? Stelle(n)?' must not whitelist other departments"
    );
}
