fn main() {
    let dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr",
    );
    let dict = og_tagger::morfologik::MorfologikDictionary::load(
        &dir.join("french.dict"),
        &dir.join("french.info"),
    )
    .expect("dict");
    for w in ["atlas", "géologue", "cher", "intelligent"] {
        let readings: Vec<String> = dict
            .lookup(w)
            .iter()
            .map(|(l, t)| format!("{l}[{t}]"))
            .collect();
        println!("{w} => {}", readings.join(" ; "));
    }
}
