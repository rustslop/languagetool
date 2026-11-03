use super::support::grammar_available;

const SIMPLIFIED_ARM: &str = r#"<token postag="UNKNOWN" regexp="yes">[a-z]+</token>"#;
const PRE_REFRESH_ARM: &str = "[a-z][a-z]?[a-z]?[a-z]?[a-z]?[a-z]?";
const FIXED_EXCEPTION_POSTAG: &str =
    r#"<exception scope="previous" postag="R pers suj 3 . s" postag_regexp="yes"/>"#;

fn strip_xml_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        rest = match after.find("-->") {
            Some(end) => &after[end + 3..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn example_accounting(xml: &str) -> (usize, usize, usize) {
    let raw = xml.matches("<example").count();
    let without_comments = strip_xml_comments(xml);
    let live = without_comments.matches("<example").count();
    let bytes = without_comments.as_bytes();
    let live_rules = without_comments
        .match_indices("<rule")
        .filter(|(at, _)| matches!(bytes.get(at + 5), Some(b' ') | Some(b'>') | Some(b'\n')))
        .count();
    (raw, live, live_rules)
}

#[test]
fn grammar_example_accounting_pins_the_4cff7008_refresh() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let (raw, live, live_rules) = example_accounting(&super::support::read_grammar());
    assert_eq!(
        raw, 16792,
        "raw <example> occurrences: the refresh left the evaluated mass unchanged"
    );
    assert_eq!(live, 16622, "live (non-comment) <example> occurrences");
    assert_eq!(live_rules, 5315, "live <rule> openings outside comments");
}

#[test]
fn style_example_accounting_pins_the_4cff7008_refresh() {
    let Some(dir) = crate::fr::test_support::fr_rules_dir() else {
        eprintln!("Skipping: fr rules dir not found");
        return;
    };
    let Ok(xml) = std::fs::read_to_string(dir.join("style.xml")) else {
        eprintln!("Skipping: fr style.xml not found");
        return;
    };
    let (raw, live, live_rules) = example_accounting(&xml);
    assert_eq!(raw, 2623, "raw <example> occurrences in style.xml");
    assert_eq!(
        live, 2613,
        "live (non-comment) <example> occurrences in style.xml"
    );
    assert_eq!(
        live_rules, 1673,
        "live <rule> openings outside comments in style.xml"
    );
}

#[test]
fn parentheses_antipattern_carries_the_simplified_regex() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let xml = super::support::read_grammar();
    assert_eq!(
        xml.matches(SIMPLIFIED_ARM).count(),
        1,
        "upstream ec2ddb171fa ('[fr] simplify regexp') rewrote the PARENTHESES[5] UNKNOWN \
         antipattern arm to [a-z]+"
    );
    assert_eq!(
        xml.matches(PRE_REFRESH_ARM).count(),
        0,
        "the pre-refresh 1..6-letter quantifier chain must be gone"
    );
}

#[test]
fn negation_que_exception_postag_has_no_leading_space() {
    if !grammar_available() {
        eprintln!("Skipping: fr grammar.xml not found");
        return;
    }
    let xml = super::support::read_grammar();
    assert_eq!(
        xml.matches(FIXED_EXCEPTION_POSTAG).count(),
        3,
        "upstream b650fda3881 ('[fr,pt] fix postag regexp') removed the leading space from \
         the NEGATION_QUE[4] c'-exception postag, joining the two s'-exceptions that already \
         carried the fixed form"
    );
    assert_eq!(
        xml.matches("postag=\" R").count(),
        0,
        "no leading-space postag regex may survive in grammar.xml: Java matches postags with \
         Pattern.matcher(...).matches() and no real postag starts with a space, so such an \
         alternative can never fire"
    );
    let Some(dir) = crate::fr::test_support::fr_rules_dir() else {
        return;
    };
    if let Ok(style) = std::fs::read_to_string(dir.join("style.xml")) {
        assert_eq!(
            style.matches("postag=\" R").count(),
            0,
            "no leading-space postag regex in style.xml either"
        );
    }
}
