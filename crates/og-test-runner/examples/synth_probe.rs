use og_rules::postponed_adjective::LangSynthesizer;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let synth = LangSynthesizer::for_language(&dir, "de").expect("synth");
    for lemma in [
        "Eigenleistung",
        "besuchen",
        "schneien",
        "tönen",
        "wollen",
        "Eichhörnchen",
    ] {
        println!(
            "{lemma}: SUB-PLU-FEM: {:?}  SUB-PLU-NEU: {:?}  PRT: {:?}",
            synth.synthesize_full(lemma, "SUB:NOM:PLU:FEM"),
            synth.synthesize_full(lemma, "SUB:NOM:PLU:NEU"),
            synth.synthesize_full(lemma, "VER:3:SIN:PRT:SFT|VER:2:PLU:PRT:SFT")
        );
    }
}
