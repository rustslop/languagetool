use og_core::checker::Tagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::GermanTagger;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let tagger = GermanTagger::from_resource_dir(&dir).expect("tagger");
    println!(
        "lookup first: {:?}",
        tagger.lookup("Abgeordneter").map(|v| v.len())
    );
    let toks: Vec<&str> = vec!["Der", "Abgeordneter", "ging"];
    let mut atrs: Vec<AnalyzedTokenReadings> = toks
        .iter()
        .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::new(*t, 0, 0)))
        .collect();
    tagger.tag(&mut atrs);
    for (i, atr) in atrs.iter().enumerate() {
        println!("{:?} => {} readings", toks[i], atr.readings().len());
    }
}
