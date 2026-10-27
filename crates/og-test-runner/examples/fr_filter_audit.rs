use og_xml::compiler::XmlCompiler;

fn main() {
    let path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/rules/fr/grammar.xml";
    let xml = std::fs::read_to_string(path).unwrap();
    let rules = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(path).parent())
        .unwrap();
    let mut n = 0;
    for rule in &rules.rules {
        if let Some(f) = &rule.filter {
            n += 1;
            println!("{} sub {:?} -> {}", rule.id, rule.sub_id, f.class);
        }
    }
    println!("total rules with filter: {n} of {}", rules.rules.len());
}
