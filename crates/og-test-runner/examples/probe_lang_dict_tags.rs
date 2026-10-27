use og_tagger::morfologik::MorfologikDictionary;

fn main() {
    let dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/resource/es",
    );
    let Some(dict) =
        MorfologikDictionary::load(&dir.join("spanish.dict"), &dir.join("spanish.info"))
    else {
        println!("LOAD FAILED");
        return;
    };
    println!("present-es: entries={}", dict.len());
    for w in ["cantar", "casa", "destacadamente"] {
        let readings: Vec<String> = dict
            .lookup(w)
            .iter()
            .map(|(l, t)| format!("{l}/{t}"))
            .collect();
        println!("  {w} => [{}]", readings.join(", "));
    }
}
