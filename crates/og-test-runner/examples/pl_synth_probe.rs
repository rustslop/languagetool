use og_rules::postponed_adjective::LangSynthesizer;

fn main() {
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl",
    );
    let synth = LangSynthesizer::for_language(&dir, "pl").expect("pl synth");
    println!(
        "aff -> {:?}",
        synth.synthesize_exact_for("zrobić", None, "ger:sg:nom.acc:n2:perf:aff:refl.nonrefl")
    );
    println!(
        "neg -> {:?}",
        synth.synthesize_exact_for(
            "zrobić",
            Some("ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"),
            "ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"
        )
    );
    println!(
        "regexp aff? -> {:?}",
        synth.synthesize_full_for("zrobić", None, "ger:sg:nom\\.acc:n2:perf:aff:refl.nonrefl")
    );
    println!(
        "regexp neg (target :aff?) -> {:?}",
        synth.synthesize_full_for(
            "zrobić",
            Some("ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"),
            "ger:sg:nom\\.acc:n2:perf:neg:refl.nonrefl"
        )
    );
}
