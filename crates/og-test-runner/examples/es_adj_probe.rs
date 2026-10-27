use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let text = std::env::args().nth(1).unwrap_or_default();
    if text.is_empty() {
        return;
    }
    let runner = TestRunner::for_language("es");
    let path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/grammar.xml";
    let xml = std::fs::read_to_string(path).unwrap();
    let rules = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(path).parent())
        .unwrap();
    og_rules::pattern_rule::set_test_date_hack(true);
    let sentence = runner.debug_sentence(&text);
    let toks: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
    for rule in &rules.rules {
        if rule.id != "AGREEMENT_POSTPONED_ADJ" {
            continue;
        }
        let (_, _, matches) = runner.match_rule_on_text(rule, &text);
        for m in &matches {
            let s = m.offset();
            let e = s + m.length();
            println!(
                "sub {:?} match {}..{} '{}' repl {:?}",
                rule.sub_id,
                s,
                e,
                &text[s.min(text.len())..e.min(text.len())],
                m.replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
    let _ = &toks;
}
