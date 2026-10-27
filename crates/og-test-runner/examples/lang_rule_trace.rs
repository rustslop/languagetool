use og_rules::pattern_trace;
use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let code = std::env::args().nth(1).expect("lang code");
    let rule_id = std::env::args().nth(2).expect("rule id");
    let text = std::env::args().nth(3).expect("text");
    let grammar_path = format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
    );
    let xml = std::fs::read_to_string(&grammar_path).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(grammar_path.as_str()).parent())
        .expect("compile");
    let runner = TestRunner::for_language(&code);
    for (n, rule) in rule_set
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.id == rule_id)
    {
        pattern_trace::set_enabled(true);
        pattern_trace::clear();
        let (clean, _mr, matches) = runner.match_rule_on_text(rule, &text);
        println!(
            "=== [#{n}] sub={:?} text={clean:?} matches={}",
            rule.sub_id,
            matches.len()
        );
        let sentence = runner.debug_sentence(&clean);
        for (i, tok) in sentence.tokens().iter().enumerate() {
            println!(
                "  tok{i} {:?}: {:?}",
                tok.token(),
                tok.readings()
                    .iter()
                    .map(|r| (r.lemma(), r.pos_tags()))
                    .collect::<Vec<_>>()
            );
        }
        for e in pattern_trace::take() {
            println!(
                "  trace elem#{} tok={} tokidx={} {}",
                e.pattern_idx, e.token, e.token_idx, e.cause
            );
        }
        pattern_trace::set_enabled(false);
    }
}
