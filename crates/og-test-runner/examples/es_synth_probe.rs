use og_rules::postponed_adjective::LangSynthesizer;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/resource/es",
    );
    let synth = LangSynthesizer::for_language(&dir, "es").expect("synth");
    for lemma in ["persona", "perro", "huevo", "casa", "destacada"] {
        println!(
            "{lemma}: exact NCFP000: {:?}  full (A...|NC.|V.P..)S(.*)/$1P$2 -> NCFP000: {:?}",
            synth.synthesize_exact(lemma, "NCFP000"),
            synth.synthesize_full(lemma, "NCFP000"),
        );
    }
    println!(
        "target_postag [NCFS000] -> {:?}",
        synth.target_postag(&["NCFS000"])
    );
    println!(
        "target_postag [VMIP3S0,VMM02S0] -> {:?}",
        synth.target_postag(&["VMIP3S0", "VMM02S0"])
    );
    println!(
        "target_postag [VMM02S0,VMIP3S0,Z] -> {:?}",
        synth.target_postag(&["VMM02S0", "VMIP3S0", "Z"])
    );
}
