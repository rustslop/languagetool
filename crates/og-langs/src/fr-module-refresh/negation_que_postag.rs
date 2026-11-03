use super::support::compile_grammar;
use super::support::compiled_rules;
use super::support::find_rule;
use super::support::grammar_available;
use super::support::read_grammar;
use super::support::rule_match_spans;

const PROBE: &str = "Il c'est que toi.";
const INCORRECT_EXAMPLE: &str = "C'est qu'une fille.";
const CORRECT_QUEXAMPLE: &str = "Non mais, dites donc, qu'est-ce que c'est que ce genre ?";
const CORRECT_CE_EXAMPLE: &str = "Je sais ce que c'est que cela.";
const FIXED_EXCEPTION: &str =
    r#"<exception scope="previous" postag="R pers suj 3 . s" postag_regexp="yes"/>"#;
const PRE_REFRESH_EXCEPTION: &str =
    r#"<exception scope="previous" postag=" R pers suj 3 . s" postag_regexp="yes"/>"#;

fn live_rule() -> og_xml::compiler::CompiledRule {
    let rules = compiled_rules();
    find_rule(&rules, "NEGATION_QUE", "4").clone()
}

fn splice_c_prime_exception(xml: &str, replacement: &str) -> String {
    let mut at = 0usize;
    let mut sites = Vec::new();
    while let Some(hit) = xml[at..].find(FIXED_EXCEPTION) {
        let start = at + hit;
        if xml[start.saturating_sub(200)..start].contains("<token>c'") {
            sites.push(start);
        }
        at = start + FIXED_EXCEPTION.len();
    }
    assert_eq!(
        sites.len(),
        1,
        "exactly the NEGATION_QUE[4] c'-exception carries this postag behind a c' token; the \
         other two occurrences belong to s' tokens"
    );
    let mut out = xml.to_string();
    out.replace_range(sites[0]..sites[0] + FIXED_EXCEPTION.len(), replacement);
    out
}

fn spliced_rule(exception_xml: &str) -> og_xml::compiler::CompiledRule {
    let xml = read_grammar();
    let spliced = splice_c_prime_exception(&xml, exception_xml);
    let rules = compile_grammar(&spliced).rules;
    rules
        .into_iter()
        .find(|rule| rule.id == "NEGATION_QUE" && rule.sub_id.as_deref() == Some("4"))
        .expect("NEGATION_QUE[4] with the spliced exception")
}

fn negation_spans(rule: &og_xml::compiler::CompiledRule, text: &str) -> Vec<(usize, usize)> {
    rule_match_spans(rule, text)
        .into_iter()
        .map(|(start, end, _)| (start, end))
        .collect()
}

#[test]
fn the_fixed_postag_matches_gendered_third_person_subject_readings_only() {
    let re = regex::Regex::new(r"^(?:R pers suj 3 . s)$").expect("anchored postag regex");
    assert!(re.is_match("R pers suj 3 m s"));
    assert!(re.is_match("R pers suj 3 f s"));
    assert!(
        !re.is_match("R pers suj 3 s"),
        "the genderless shape (je/tu/il impersonal variants) is 14 chars, the regex needs the \
         gender slot"
    );
    assert!(
        !re.is_match("R pers suj 3 m p"),
        "plural escapes the final s anchor"
    );
}

#[test]
fn il_keeps_its_gendered_subject_reading_before_c() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let pipeline = crate::fr::test_support::FrenchPipeline::shared();
    let sentence = &pipeline.analyze_text(PROBE)[0];
    let il = sentence
        .tokens()
        .iter()
        .find(|token| token.token().token() == "Il")
        .expect("'Il' token in the probe");
    assert!(
        il.has_pos_tag("R pers suj 3 m s"),
        "the exception needs this reading to fire: {:?}",
        il.readings()
    );
}

#[test]
fn the_fixed_exception_suppresses_the_rule_after_a_subject_pronoun() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    assert!(
        negation_spans(&live_rule(), PROBE).is_empty(),
        "upstream b650fda3881 makes the c'-exception live: the scope=previous postag \
         R pers suj 3 . s matches 'Il' (R pers suj 3 m s) and NEGATION_QUE[4] must stay silent"
    );
}

#[test]
fn removing_the_exception_restores_the_match() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let c_est_at = PROBE.find("c'est").expect("c'est");
    let without_exception = spliced_rule("");
    assert_eq!(
        negation_spans(&without_exception, PROBE),
        vec![(c_est_at, c_est_at + "c'est".len())],
        "with the exception spliced out the rule must flag 'c'est' — the suppression above is \
         carried by the refreshed exception alone"
    );
}

#[test]
fn the_pre_refresh_leading_space_is_inert_in_the_rust_engine() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let pre_refresh = spliced_rule(PRE_REFRESH_EXCEPTION);
    assert!(
        negation_spans(&pre_refresh, PROBE).is_empty(),
        "og-xml compiler.rs:130-137 anchor_regex trims postag patterns before anchoring, so the \
         pre-refresh leading space compiled to the same ^(?:R pers suj 3 . s)$ here and the \
         exception was already firing in Rust; the refresh is a Java-side fix (Java matches \
         postags with Pattern.matcher(...).matches(), where the leading space could never match \
         a real postag) that the data now states explicitly"
    );
}

#[test]
fn negation_que_rule4_keeps_its_three_examples() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let rule = live_rule();
    let c_est_at = INCORRECT_EXAMPLE.find("C'est").expect("C'est");
    assert_eq!(
        negation_spans(&rule, INCORRECT_EXAMPLE),
        vec![(c_est_at, c_est_at + "C'est".len())],
        "sentence-initial c' has no previous token, so the exception cannot veto and the \
         incorrect example must keep firing"
    );
    assert!(
        negation_spans(&rule, CORRECT_QUEXAMPLE).is_empty(),
        "'qu'est-ce que c'est que' stays suppressed by the first antipattern"
    );
    assert!(
        negation_spans(&rule, CORRECT_CE_EXAMPLE).is_empty(),
        "'ce que c'est que' stays suppressed by the second antipattern"
    );
}
