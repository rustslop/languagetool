use og_tagger::morfologik::MorfologikDictionary;
fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let hun = dir.join("hunspell");
    match MorfologikDictionary::load(&hun.join("de_DE.dict"), &hun.join("de_DE.info")) {
        Some(d) => {
            eprintln!("loaded, entries={}", d.len());
            for probe in [
                "hocheffizientes",
                "hocheffizient",
                "rüberlassen",
                "hinterherspioniert",
                "Haus",
                "hochexplosiv",
            ] {
                let v = d.lookup(probe);
                if v.is_empty() {
                    println!("{probe}: NOT-FOUND");
                } else {
                    println!("{probe}: {:?}", &v[..v.len().min(2)]);
                }
            }
        }
        None => eprintln!("load failed"),
    }
}
