use og_spell::dictionary::Dictionary;
use og_spell::german::GermanSpeller;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let dir = PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let tagger_dict = og_tagger::german_tagger::shared_german_dictionary(&dir).expect("dict");
    let mut d = Dictionary::new();
    for word in tagger_dict.keys() {
        d.add_word(word);
    }
    if let Ok(words) = std::fs::read_to_string(dir.join("hunspell/spelling.txt")) {
        for line in words.lines() {
            for word in og_rules::spelling_support::prepare_line_for_speller(
                og_rules::spelling_support::SpellerVariant::German,
                line,
            ) {
                d.add_word(&word);
            }
        }
    }
    let speller = GermanSpeller::new(Arc::new(d), &dir, None, None, "de-DE");
    for word in [
        "rüberlassen",
        "hinterherspioniert",
        "vorhaben",
        "hocheffizientes",
        "hochexplosiv",
        "rüber",
        "lassen",
    ] {
        println!("{word}: misspelled={}", speller.is_misspelled(word));
    }
}
