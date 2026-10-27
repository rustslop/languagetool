use og_tagger::morfologik::shared_synth_dictionary;

fn main() {
    let code = std::env::args().nth(1).expect("lang code");
    let stem = std::env::args().nth(2).expect("dict stem");
    let dir = std::path::PathBuf::from(format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/resource/{code}"
    ));
    let d = shared_synth_dictionary(&dir, &stem).unwrap_or_else(|| panic!("{stem} load failed"));
    for key in std::env::args().skip(3) {
        println!("{key} => {:?}", d.lookup(&key));
    }
}
