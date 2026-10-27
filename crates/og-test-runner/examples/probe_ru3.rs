use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let text = std::env::args().nth(1).unwrap();
    let rule_id_prefix = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "NN_N_pril_prich".into());
    let path = "languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/rules/ru/grammar.xml";
    let xml = std::fs::read_to_string(path).unwrap();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(path).parent())
        .unwrap();
    let runner = TestRunner::for_language("ru");
    for rule in &rule_set.rules {
        if !rule.id.contains(&rule_id_prefix) {
            continue;
        }
        let (clean, _m, matches) = runner.match_rule_on_text(rule, &text);
        if !matches.is_empty() {
            for m in &matches {
                let s = m.offset();
                let e = s + m.length();
                println!(
                    "{} [{}] sub={:?}: {}..{} => {:?}",
                    rule.id,
                    rule.sub_id.clone().unwrap_or_default(),
                    rule.name,
                    s,
                    e,
                    &clean[s.min(clean.len())..e.min(clean.len())]
                );
            }
        }
    }
}
