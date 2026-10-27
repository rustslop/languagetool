use og_spell::hunspell::HunspellChecker;
use std::io::{BufRead, Write};

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de/hunspell",
    );
    let checker =
        HunspellChecker::load(&dir.join("de_DE.aff"), &dir.join("de_DE.dic")).expect("load");
    let stdin = std::io::stdin();
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    for line in stdin.lock().lines() {
        let word = line.unwrap();
        writeln!(
            out,
            "{}: {}",
            word,
            if checker.spell(&word) { 1 } else { 0 }
        )
        .unwrap();
    }
}
