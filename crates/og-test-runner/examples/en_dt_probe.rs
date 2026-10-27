use og_rules::postponed_adjective::LangSynthesizer;
use og_test_runner::runner::TestRunner;

fn main() {
    let runner = TestRunner::for_language("en");
    let sentence = runner.debug_sentence("He is friend.");
    let xml = r#"<rule id="DT_MISSING" name="article missing">
        <pattern>
            <token>is</token>
            <marker><token postag="NN" postag_regexp="yes"><exception postag="NNP"/></token></marker>
        </pattern>
        <message>The article might be missing before the noun.</message>
        <suggestion><match no="2" postag="+DT"/></suggestion>
    </rule>"#;
    let compiler = og_xml::compiler::XmlCompiler::new();
    let rule_set = compiler.compile_file(xml).expect("compile");
    let compiled = &rule_set.rules[0];
    let en_dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let synth = LangSynthesizer::for_language(&en_dir, "en").expect("synth");
    let engine = og_rules::pattern_rule::PatternRuleEngine::new()
        .with_suggestion_synthesizer(std::sync::Arc::new(synth));
    for m in engine.match_rule(compiled, &sentence) {
        println!(
            "match at {}..{}: {:?}",
            m.offset(),
            m.offset() + m.length(),
            m.replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<_>>()
        );
    }
}
