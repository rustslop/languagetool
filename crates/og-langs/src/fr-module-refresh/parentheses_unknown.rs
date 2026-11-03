use super::support::compile_grammar;
use super::support::compiled_rules;
use super::support::find_rule;
use super::support::grammar_available;
use super::support::read_grammar;
use super::support::rule_is_untagged_at;
use super::support::rule_match_spans;

const SHORT_PROBE: &str = "voici(xyz) aujourd'hui.";
const LONG_PROBE: &str = "voici(xyzwvuts) aujourd'hui.";
const RULE_EXAMPLE: &str = "Emilie( dit en souriant) : Il fait beau aujourd'hui.";
const SIMPLIFIED_ARM: &str = r#"<token postag="UNKNOWN" regexp="yes">[a-z]+</token>"#;
const PRE_REFRESH_ARM: &str =
    r#"<token postag="UNKNOWN" regexp="yes">[a-z][a-z]?[a-z]?[a-z]?[a-z]?[a-z]?</token>"#;

fn live_rule() -> og_xml::compiler::CompiledRule {
    let rules = compiled_rules();
    find_rule(&rules, "PARENTHESES", "5").clone()
}

fn pre_refresh_rule() -> og_xml::compiler::CompiledRule {
    let xml = read_grammar();
    assert_eq!(xml.matches(SIMPLIFIED_ARM).count(), 1);
    let spliced = xml.replacen(SIMPLIFIED_ARM, PRE_REFRESH_ARM, 1);
    let rules = compile_grammar(&spliced).rules;
    rules
        .into_iter()
        .find(|rule| rule.id == "PARENTHESES" && rule.sub_id.as_deref() == Some("5"))
        .expect("PARENTHESES[5] with the pre-refresh regex")
}

fn parentheses_spans(rule: &og_xml::compiler::CompiledRule, text: &str) -> Vec<(usize, usize)> {
    rule_match_spans(rule, text)
        .into_iter()
        .map(|(start, end, _)| (start, end))
        .collect()
}

#[test]
fn probes_split_into_the_four_antipattern_tokens() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let pipeline = crate::fr::test_support::FrenchPipeline::shared();
    for (text, parenthetical) in [(SHORT_PROBE, "xyz"), (LONG_PROBE, "xyzwvuts")] {
        let sentences = pipeline.analyze_text(text);
        let surfaces: Vec<&str> = sentences
            .iter()
            .flat_map(|sentence| sentence.tokens().iter())
            .filter(|token| !token.is_whitespace())
            .map(|token| token.token().token())
            .collect();
        let expected = ["voici", "(", parenthetical, ")"];
        assert!(
            expected.iter().all(|surface| surfaces.contains(surface)),
            "{text} must tokenize {expected:?} out: {surfaces:?}"
        );
    }
}

#[test]
fn parenthetical_unknowns_stay_untagged_by_the_dictionary() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    for (text, unknown) in [(SHORT_PROBE, "xyz"), (LONG_PROBE, "xyzwvuts")] {
        assert!(
            rule_is_untagged_at(text, unknown),
            "'{unknown}' must carry no dictionary reading in '{text}' so only the postag=UNKNOWN \
             antipattern arm can match it"
        );
    }
}

#[test]
fn short_unknowns_are_suppressed_under_both_the_old_and_new_regex() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    assert!(
        parentheses_spans(&live_rule(), SHORT_PROBE).is_empty(),
        "the simplified [a-z]+ arm must suppress PARENTHESES[5] on a 3-letter unknown"
    );
    assert!(
        parentheses_spans(&pre_refresh_rule(), SHORT_PROBE).is_empty(),
        "the pre-refresh 1..6-letter chain matched 'xyz' too, so the shared domain is \
         behavior-identical"
    );
}

#[test]
fn seven_letter_unknowns_only_suppress_with_the_simplified_regex() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let voici_at = LONG_PROBE.find("voici").expect("voici");
    assert!(
        parentheses_spans(&live_rule(), LONG_PROBE).is_empty(),
        "[a-z]+ now covers the 8-letter untagged 'xyzwvuts', so the antipattern fires and \
         PARENTHESES[5] must stay silent"
    );
    assert_eq!(
        parentheses_spans(&pre_refresh_rule(), LONG_PROBE),
        vec![(voici_at, voici_at + "voici(".len())],
        "the pre-refresh chain stopped at 6 letters, so 'xyzwvuts' escaped the antipattern and \
         the rule flagged 'voici(' — the simplification extends suppression to unknowns of 7+ \
         letters, the only divergence from the old regex under the full-match anchoring both \
         engines use (Java StringMatcher.java:165 calls matcher.matches(); og-xml \
         compiler.rs:130-137 anchors ^(?:...)$)"
    );
}

#[test]
fn parentheses_rule5_keeps_flagging_its_own_example() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let emilie_at = RULE_EXAMPLE.find("Emilie(").expect("Emilie(");
    let live = rule_match_spans(&live_rule(), RULE_EXAMPLE);
    assert!(
        live.iter()
            .any(|(start, end, _)| *start == emilie_at && *end == emilie_at + "Emilie(".len()),
        "PARENTHESES[5] must keep flagging its own incorrect example with the marker span \
         covered: {live:?}"
    );
}
