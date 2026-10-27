use og_tagger::morfologik::MorfologikDictionary;

fn main() {
    let dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let dict = MorfologikDictionary::load(&dir.join("german.dict"), &dir.join("german.info"))
        .unwrap_or_else(|| panic!("german.dict load failed"));
    println!("dict entries: {}", dict.len());
    for w in std::env::args().skip(1) {
        let readings: Vec<String> = dict
            .lookup(&w)
            .iter()
            .map(|(l, t)| format!("{l}/{t}"))
            .collect();
        println!("{w} => [{}]", readings.join(", "));
    }
}
