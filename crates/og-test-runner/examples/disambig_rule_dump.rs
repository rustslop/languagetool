fn main() {
    let lang = std::env::args().nth(1).unwrap_or_else(|| "pl".into());
    let xml = std::fs::read_to_string(format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}/disambiguation.xml"
    ))
    .expect("xml");
    let parser = og_xml::DisambigXmlParser::new();
    let set = parser.parse(&xml).expect("parse");
    for rule in &set.rules {
        println!("{}\t{:?}", rule.id.as_deref().unwrap_or("?"), rule.disambig);
    }
}
