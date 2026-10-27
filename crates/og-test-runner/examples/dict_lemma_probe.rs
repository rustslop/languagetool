use og_tagger::morfologik::MorfologikDictionary;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let dict = MorfologikDictionary::load(&dir.join("german.dict"), &dir.join("german.info"))
        .expect("dict");
    for w in [
        "Eigenleistungen",
        "Eigenleistung",
        "Frauen",
        "besuchte",
        "besuchtet",
    ] {
        println!("{w}: {:?}", dict.lookup(w));
    }
    let forms = dict.forms_by_lemma();
    println!(
        "Eigenleistung forms: {:?}",
        forms
            .get("Eigenleistung")
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    );
}
