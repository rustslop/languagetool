fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let dict = og_tagger::german_tagger::shared_german_dictionary(&dir).expect("dict");
    for probe in [
        "hocheffizient",
        "hocheffiziente",
        "hocheffizientes",
        "hochexplosiv",
        "hochexplosive",
        "hinterherspioniert",
        "hinterherspionieren",
        "rüberlassen",
        "rüber",
        "hinterher",
        "spioniert",
    ] {
        let v = dict.lookup(probe);
        if v.is_empty() {
            println!("{probe}: NOT-IN-TAGGER-DICT");
        } else {
            println!("{probe}: {:?}", v.iter().take(4).collect::<Vec<_>>());
        }
    }
}
