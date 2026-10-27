use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

fn main() {
    let text = std::env::args().nth(1).expect("sentence");
    let rule_id = std::env::args().nth(2).expect("rule id");
    let runner = TestRunner::for_language("en");
    let sentence = runner.debug_sentence(&text);
    let grammar = std::fs::read_to_string("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml").unwrap();
    let rule_set = XmlCompiler::new().compile_file(&grammar).expect("compile");
    let engine = og_rules::pattern_rule::PatternRuleEngine::new();
    og_rules::pattern_trace::set_enabled(true);
    for (n, compiled) in rule_set.rules.iter().enumerate() {
        if compiled.id != rule_id {
            continue;
        }
        let matches = engine.match_rule(compiled, &sentence);
        println!(
            "subrule {n} (sub_id={:?}): {} matches",
            compiled.sub_id,
            matches.len()
        );
        if !matches.is_empty() {
            for tok in &compiled.pattern.tokens {
                println!(
                    "  ELEM text={:?} postag={:?} re={:?} neg={}",
                    tok.text,
                    tok.postag,
                    tok.compiled_regexp.is_some(),
                    tok.negate
                );
            }
        }
        for ev in og_rules::pattern_trace::take() {
            println!(
                "  elem={} tok={} token={:?} cause={}",
                ev.pattern_idx, ev.token_idx, ev.token, ev.cause
            );
        }
        og_rules::pattern_trace::clear();
    }
    og_rules::pattern_trace::set_enabled(false);
}
