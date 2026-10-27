fn main() {
    let path = std::env::args().nth(1).unwrap();
    let rid = std::env::args().nth(2).unwrap();
    let xml = std::fs::read_to_string(&path).unwrap();
    let parser = og_xml::parser::XmlRuleParser::new();
    let result = parser.parse(&xml).unwrap();
    for rule in result.rules.iter().filter(|r| r.id == rid) {
        println!("rule {} pattern tokens:", rule.id);
        for (i, t) in rule.pattern.tokens.iter().enumerate() {
            println!(
                "  [{i}] text={:?} postag={:?} postag_re={:?} negate_pos={} regexp={:?} exc={} match_no={:?} mpostag={:?} mre={} mrep={:?} setpos={}",
                t.text,
                t.postag,
                t.postag_regexp,
                t.negate_pos,
                t.regexp,
                t.exceptions.len(),
                t.match_no,
                t.match_no_postag,
                t.match_no_postag_regexp,
                t.match_no_postag_replace,
                t.match_no_setpos
            );
        }
    }
}
