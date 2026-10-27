use og_rules::pattern_rule::PatternRuleEngine;
use og_test_runner::runner::TestRunner;
use og_xml::compiler::XmlCompiler;

const RULES: [&str; 2] = ["MD_BASEFORM", "SENT_START_CONJUNCTIVE_LINKING_ADVERB_COMMA"];

fn main() {
    let grammar_path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
    let xml = std::fs::read_to_string(grammar_path).expect("grammar.xml");
    let rule_set = XmlCompiler::new().compile_file(&xml).expect("compile");
    let runner = TestRunner::for_language("en");
    let engine = PatternRuleEngine::new();

    let artifact = std::fs::read_to_string("/tmp/test_failures_all.txt").expect("artifact");
    let mut wanted: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for line in artifact.lines() {
        if !(line.contains("Expected match on incorrect example but got none")
            || line.contains("Expected no match on correct example"))
        {
            continue;
        }
        for rule in RULES {
            if line.starts_with(&format!("FAIL: [{rule}]")) {
                let text = line
                    .split(": '")
                    .nth(1)
                    .map(|rest| rest.strip_suffix('\'').unwrap_or(rest))
                    .unwrap_or("");
                let text = text.replace("<marker>", "").replace("</marker>", "");
                let entry = wanted.entry(rule.to_string()).or_default();
                if !entry.contains(&text) {
                    entry.push(text);
                }
            }
        }
    }

    for rule_id in RULES.iter() {
        println!("===================== {rule_id}");
        for rule in rule_set.rules.iter().filter(|r| r.id == *rule_id) {
            println!("--- sub rule (pattern tokens, first 8):");
            for (i, tok) in rule.pattern.elements.iter().take(8).enumerate() {
                println!("  {i}: {tok:?}");
            }
            for text in wanted
                .get(*rule_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[])
                .iter()
            {
                let sentence = runner.debug_sentence(text);
                println!("  TEXT: {text}");
                for atr in sentence.tokens() {
                    if atr.is_whitespace() {
                        continue;
                    }
                    let readings: Vec<String> = atr
                        .readings()
                        .iter()
                        .map(|r| {
                            format!("{}[{}]", r.lemma().unwrap_or("?"), r.pos_tags().join("|"))
                        })
                        .collect();
                    println!("    {:?} => {}", atr.token().token(), readings.join(" ; "));
                }
                let matches = engine.match_rule(rule, &sentence);
                println!("  MATCHES: {}", matches.len());
            }
        }
    }
}
