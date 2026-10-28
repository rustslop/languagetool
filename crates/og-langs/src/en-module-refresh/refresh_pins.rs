use super::pipeline::compiled_rules;
use super::pipeline::grammar_available;
use super::pipeline::grammar_path;

#[test]
fn grammar_totals_pin_the_refreshed_file() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        5556,
        "the 4cff7008 en grammar.xml must compile exactly its 5556 live <rule> elements"
    );
    let total_examples: usize = rule_set.rules.iter().map(|rule| rule.examples.len()).sum();
    assert_eq!(
        total_examples, 21314,
        "22592 live <example> tags minus the 1278 living inside <antipattern> blocks"
    );
}

#[test]
fn example_tag_accounting_matches_the_flagship_suite() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let path = grammar_path().expect("en grammar.xml path");
    let xml = std::fs::read_to_string(&path).expect("en grammar.xml");
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
        raw, 22707,
        "raw <example> occurrences, the count the refresh left unchanged"
    );
    assert_eq!(live, 22592, "live (non-comment) <example> occurrences");
    assert_eq!(
        in_antipattern, 1278,
        "live examples sitting inside <antipattern> blocks"
    );
    assert_eq!(
        live - in_antipattern,
        21314,
        "the evaluated mass the flagship suite runs green at 4cff7008"
    );
    let bytes = without_comments.as_bytes();
    let live_rules = without_comments
        .match_indices("<rule")
        .filter(|(at, _)| matches!(bytes.get(at + 5), Some(b' ') | Some(b'>') | Some(b'\n')))
        .count();
    assert_eq!(live_rules, 5556, "live <rule> openings outside comments");
}

#[test]
fn refresh_leaves_no_trailing_space_in_the_unknown_alternative() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let path = grammar_path().expect("en grammar.xml path");
    let xml = std::fs::read_to_string(&path).expect("en grammar.xml");
    assert_eq!(
        xml.matches("NN.*|UNKNOWN \"").count(),
        0,
        "the 4cff7008 refresh (upstream 59d80b8fadb) removed the trailing space from the \
         HAVE_PART_AGREEMENT antipattern postag; a trailing-space UNKNOWN alternative can never \
         match a real postag"
    );
    assert_eq!(
        xml.matches("NN.*|UNKNOWN\"").count(),
        38,
        "occurrences of the fixed NN.*|UNKNOWN alternation"
    );
}
