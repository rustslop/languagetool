use og_tagger::morfologik::MorfologikDictionary;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl",
    );
    let d =
        MorfologikDictionary::load(&dir.join("dutch.dict"), &dir.join("dutch.info")).expect("load");
    println!("entries: {}", d.len());
    for w in ["verstoort", "koken", "zwijnden", "aardappels"] {
        let readings: Vec<String> = d
            .lookup(w)
            .iter()
            .map(|(l, t)| format!("{l}/{t}"))
            .collect();
        println!("{w} => [{}]", readings.join(", "));
    }
}
