use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let rule_id = std::env::args().nth(1).expect("rule id");
    let text = std::env::args().nth(2).expect("text");
    let grammar_path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/grammar.xml";
    let xml = std::fs::read_to_string(grammar_path).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(grammar_path).parent())
        .expect("compile");
    let runner = TestRunner::for_language("de");
    let sentence = runner.debug_sentence(&text);
    for (i, t) in sentence.tokens().iter().enumerate() {
        let readings = t
            .readings()
            .iter()
            .map(|r| format!("{}:{}", r.lemma().unwrap_or("?"), r.pos_tags().join("|")))
            .collect::<Vec<_>>()
            .join(" ");
        println!("{i:3} {:?} [{readings}]", t.token().token());
    }
    for (n, rule) in rule_set
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.id == rule_id)
    {
        let matches = runner.match_rule_on_text(rule, &text).2;
        println!(
            "=== [#{n}] sub={:?} antipatterns={} matches={}",
            rule.sub_id,
            rule.antipatterns.len(),
            matches.len()
        );
        let mut stripped = rule.clone();
        stripped.antipatterns = Vec::new();
        let matches_no_anti = runner.match_rule_on_text(&stripped, &text).2;
        println!("   without antipatterns: {}", matches_no_anti.len());
        let mut no_filter = rule.clone();
        no_filter.antipatterns = Vec::new();
        no_filter.filter = None;
        let matches_no_filter = runner.match_rule_on_text(&no_filter, &text).2;
        println!(
            "   without antipatterns+filter: {}",
            matches_no_filter.len()
        );
        if matches.len() != matches_no_anti.len() {
            for (ai, anti) in rule.antipatterns.iter().enumerate() {
                let mut one = rule.clone();
                one.antipatterns = vec![anti.clone()];
                let m = runner.match_rule_on_text(&one, &text).2;
                println!(
                    "   antipattern[{ai}] tokens={} -> matches {}",
                    anti.tokens.len(),
                    m.len()
                );
            }
        }
    }
}
