use og_tagger::GermanTagger;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let tagger = GermanTagger::from_resource_dir(&dir).expect("tagger");
    for word in std::env::args().skip(1) {
        match tagger.lookup(&word) {
            Some(readings) => {
                for r in &readings {
                    println!(
                        "{word}: {}[{}]",
                        r.lemma().unwrap_or("?"),
                        r.pos_tags().join("|")
                    );
                }
            }
            None => println!("{word}: <none>"),
        }
    }
}
