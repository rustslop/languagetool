use std::time::Instant;

fn main() {
    let mut report = String::new();
    let grammar_path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/grammar.xml";
    let dir = Some(std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    ));

    let t = Instant::now();
    let xml = std::fs::read_to_string(grammar_path).expect("grammar.xml");
    let xml_read = t.elapsed();

    let t = Instant::now();
    let rule_set = og_xml::compiler::XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(grammar_path).parent())
        .expect("compile");
    let grammar_compile = t.elapsed();

    let disambig_compile = dir
        .as_deref()
        .map(|dir| {
            let t = Instant::now();
            let xml = std::fs::read_to_string(dir.join("disambiguation.xml")).expect("xml");
            let disamb = og_tagger::XmlDisambiguator::from_xml(&xml).expect("disambiguator");
            (t.elapsed(), disamb.rule_count())
        })
        .unwrap_or_default();

    let tagger_build = dir
        .as_deref()
        .map(|dir| {
            let t = Instant::now();
            let tagger = og_tagger::GermanTagger::from_resource_dir(dir).expect("tagger");
            (t.elapsed(), tagger.entry_count())
        })
        .unwrap_or_default();

    let t = Instant::now();
    let runner = og_test_runner::runner::TestRunner::for_language("de");
    let runner_build = t.elapsed();

    let tagger_rebuild = dir
        .as_deref()
        .map(|dir| {
            let t = Instant::now();
            let tagger = og_tagger::GermanTagger::from_resource_dir(dir).expect("tagger");
            (t.elapsed(), tagger.entry_count())
        })
        .unwrap_or_default();

    let t = Instant::now();
    let runner2 = og_test_runner::runner::TestRunner::for_language("de");
    let runner_rebuild = t.elapsed();
    drop(runner2);

    let t = Instant::now();
    let sentence = runner.debug_sentence("Gestern schneit es ein wenig.");
    let first_check = t.elapsed();

    let t = Instant::now();
    let engine = og_langs::engine::LanguageEngine::german();
    let engine_build = t.elapsed();

    let t = Instant::now();
    let engine2 = og_langs::engine::LanguageEngine::german();
    let engine_rebuild = t.elapsed();
    drop((engine, engine2));

    let t = Instant::now();
    let _ = runner.debug_sentence("Gestern schneit es ein wenig.");
    let second_check = t.elapsed();

    report.push_str(&format!(
        "de_build_profile xml_read_ms={:.0} grammar_compile_ms={:.0} rules={} \\\n",
        xml_read.as_secs_f64() * 1e3,
        grammar_compile.as_secs_f64() * 1e3,
        rule_set.rules.len()
    ));
    report.push_str(&format!(
        "  disambig_compile_ms={:.0} disambig_rules={} tagger_build_ms={:.0} dict_entries={} \\\n",
        disambig_compile.0.as_secs_f64() * 1e3,
        disambig_compile.1,
        tagger_build.0.as_secs_f64() * 1e3,
        tagger_build.1,
    ));
    report.push_str(&format!(
        "  tagger_rebuild_ms={:.0} runner_build_ms={:.0} runner_rebuild_ms={:.0} engine_build_ms={:.0} engine_rebuild_ms={:.0} first_check_ms={:.0} second_check_ms={:.0} tokens={}\n",
        tagger_rebuild.0.as_secs_f64() * 1e3,
        runner_build.as_secs_f64() * 1e3,
        runner_rebuild.as_secs_f64() * 1e3,
        engine_build.as_secs_f64() * 1e3,
        engine_rebuild.as_secs_f64() * 1e3,
        first_check.as_secs_f64() * 1e3,
        second_check.as_secs_f64() * 1e3,
        sentence.tokens().len()
    ));
    print!("{report}");
    let _ = std::fs::write("/tmp/bench_engine_de_build.txt", &report);
}
