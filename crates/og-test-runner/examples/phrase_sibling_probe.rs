use std::collections::BTreeMap;

use og_xml::compiler::XmlCompiler;

fn main() {
    let code = std::env::args().nth(1).expect("lang code");
    let grammar_path = format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
    );
    let xml = std::fs::read_to_string(&grammar_path).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(grammar_path.as_str()).parent())
        .expect("compile");

    let mut groups: BTreeMap<(String, Option<String>), Vec<&og_xml::compiler::CompiledRule>> =
        BTreeMap::new();
    for rule in &rule_set.rules {
        groups
            .entry((rule.id.clone(), rule.sub_id.clone()))
            .or_default()
            .push(rule);
    }

    let filter = std::env::args().nth(2);
    for ((id, sub), rules) in &groups {
        if let Some(ref f) = filter {
            if !id.contains(f.as_str()) {
                continue;
            }
        } else if rules.len() < 2 {
            continue;
        }
        println!(
            "[{id}{}] -> {} siblings",
            sub.as_deref().unwrap_or(""),
            rules.len()
        );
        for rule in rules {
            let head: Vec<String> = rule
                .pattern
                .tokens
                .iter()
                .take(4)
                .map(|t| {
                    t.text
                        .clone()
                        .or_else(|| t.postag.clone())
                        .unwrap_or_else(|| "<any>".into())
                })
                .collect();
            println!("    elems={} head={:?}", rule.pattern.tokens.len(), head);
        }
    }
    println!("total compiled rules: {}", rule_set.rules.len());
}
