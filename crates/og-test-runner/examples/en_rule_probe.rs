use og_rules::postponed_adjective::LangSynthesizer;
use og_test_runner::runner::TestRunner;

fn main() {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "There are 400 College and university chapter.".into());
    let rule_xml = std::env::args().nth(2).unwrap_or_else(|| "CD_NN".into());
    let runner = TestRunner::for_language("en");
    let sentence = runner.debug_sentence(&text);
    let grammar = std::fs::read_to_string("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml").unwrap();
    let start = grammar
        .find(&format!("id=\"{rule_xml}\""))
        .expect("rule id");
    let rule_start = grammar[..start].rfind('<').unwrap_or(start);
    let mut depth = 0i32;
    let mut end = rule_start;
    let bytes = grammar.as_bytes();
    let mut i = rule_start;
    while i < bytes.len() {
        if grammar.is_char_boundary(i) && grammar[i..].starts_with("<rule") {
            depth += 1;
        } else if grammar.is_char_boundary(i) && grammar[i..].starts_with("</rule") {
            depth -= 1;
            if depth == 0 {
                end = grammar[i..].find('>').map(|t| i + t + 1).unwrap_or(i + 7);
                break;
            }
        }
        i += 1;
    }
    let rule_xml_str = &grammar[rule_start..end];
    let compiler = og_xml::compiler::XmlCompiler::new();
    let wrapped = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"en\">\n{rule_xml_str}\n</rules>\n"
    );
    let rule_set = compiler.compile_file(&wrapped).expect("compile");
    println!("compiled {} rules", rule_set.rules.len());
    let en_dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let synth = LangSynthesizer::for_language(&en_dir, "en").expect("synth");
    for (name, engine) in [
        (
            "with synth",
            og_rules::pattern_rule::PatternRuleEngine::new()
                .with_suggestion_synthesizer(std::sync::Arc::new(synth)),
        ),
        ("plain", og_rules::pattern_rule::PatternRuleEngine::new()),
    ] {
        for compiled in &rule_set.rules {
            let matches = engine.match_rule(compiled, &sentence);
            println!("{name}/{}: {} matches", compiled.id, matches.len());
            for m in &matches {
                println!(
                    "   {}..{} {:?}",
                    m.offset(),
                    m.offset() + m.length(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
