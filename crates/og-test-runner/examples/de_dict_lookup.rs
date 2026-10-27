use og_tagger::GermanTagger;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let tagger = GermanTagger::from_resource_dir(&dir).expect("tagger");
    for word in ["reparier", "abonnier", "bemal", "hast"] {
        println!("{word}:");
        for r in tagger.lookup(word).into_iter().flatten() {
            println!("  {} {}", r.lemma().unwrap_or("?"), r.pos_tags().join("|"));
        }
    }
}
