use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let xml_path = std::env::args().nth(1).expect("xml path");
    let text = std::env::args().nth(2).expect("text");
    let xml = std::fs::read_to_string(&xml_path).expect("xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(&xml_path).parent())
        .expect("compile");
    let runner = TestRunner::for_language("de");
    for rule in &rule_set.rules {
        let (_, _, matches) = runner.match_rule_on_text(rule, &text);
        println!(
            "rule {} sub={:?}: {} matches",
            rule.id,
            rule.sub_id,
            matches.len()
        );
        for m in &matches {
            println!("  {} len {} msg {:?}", m.offset(), m.length(), m.message());
        }
    }
}
