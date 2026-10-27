use std::path::{Path, PathBuf};
use std::time::Instant;

use og_langs::engine::LanguageEngine;

const LT_ROOT: &str = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules";

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn vm_hwm_mib() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let rest = line.strip_prefix("VmHWM:")?;
                rest.trim()
                    .strip_suffix("kB")
                    .and_then(|kib| kib.trim().parse::<f64>().ok())
            })
        })
        .map(|kib| kib / 1024.0)
        .unwrap_or(0.0)
}

fn mem_available_mib() -> f64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|info| {
            info.lines().find_map(|line| {
                let rest = line.strip_prefix("MemAvailable:")?;
                rest.trim()
                    .strip_suffix("kB")
                    .and_then(|kib| kib.trim().parse::<f64>().ok())
            })
        })
        .map(|kib| kib / 1024.0)
        .unwrap_or(0.0)
}

fn wait_for_mem(min_mib: f64) {
    let deadline = std::time::Duration::from_secs(300);
    let start = Instant::now();
    while mem_available_mib() < min_mib && start.elapsed() < deadline {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
    eprintln!(
        "starting with mem_available_mib={:.0} (waited {:.0}s)",
        mem_available_mib(),
        start.elapsed().as_secs_f64()
    );
}

fn resource_dir(short: &str) -> PathBuf {
    Path::new(LT_ROOT).join(format!(
        "{short}/src/main/resources/org/languagetool/resource/{short}"
    ))
}

fn rules_dir(short: &str) -> PathBuf {
    Path::new(LT_ROOT).join(format!(
        "{short}/src/main/resources/org/languagetool/rules/{short}"
    ))
}

fn tagger_stem(short: &str) -> Option<&'static str> {
    match short {
        "da" => Some("danish"),
        "ca" => Some("catalan"),
        "fr" => Some("french"),
        "es" => Some("spanish"),
        "pt" => Some("portuguese"),
        "nl" => Some("dutch"),
        "pl" => Some("polish"),
        "ru" => Some("russian"),
        "it" => Some("italian"),
        "ast" => Some("asturian"),
        "crh" => Some("crimean_tatar"),
        "br" => Some("breton"),
        "el" => Some("greek"),
        "gl" => Some("galician"),
        "ro" => Some("romanian"),
        "sk" => Some("slovak"),
        _ => None,
    }
}

fn synth_stem(short: &str) -> Option<&'static str> {
    match short {
        "gl" => Some("galician_synth"),
        "ro" => Some("romanian_synth"),
        "sk" => Some("slovak_synth"),
        _ => None,
    }
}

fn sample(short: &str) -> &'static str {
    match short {
        "de" => "Gestern schneit es ein wenig.",
        "en" => "This is a short test sentence.",
        "nl" => "Dit is een korte proefzin.",
        "sk" => "Toto je krátka skúšobná veta.",
        "eo" => "Tio estas mallonga prova frazo.",
        "pl" => "To jest krótkie zdanie próbne.",
        "fr" => "Ceci est une courte phrase de test.",
        "es" => "Esta es una frase de prueba corta.",
        "pt" => "Isto é uma frase de teste curta.",
        "ca" => "Aquesta és una frase de prova curta.",
        _ => "This is a short test sentence.",
    }
}

fn build_engine(code: &str) -> LanguageEngine {
    match code {
        "de" => LanguageEngine::german(),
        "en" => LanguageEngine::english(),
        other => LanguageEngine::for_code(other).expect("engine build"),
    }
}

fn bench_lang(code: &str, index: usize) -> String {
    let t = Instant::now();
    let engine = build_engine(code);
    let cold_build_ms = ms(t.elapsed());
    eprintln!(
        "[{code}] cold_build {cold_build_ms:.0}ms rss={:.0}",
        vm_hwm_mib()
    );

    let t = Instant::now();
    let first = engine.check(sample(code));
    let first_check_ms = ms(t.elapsed());
    eprintln!(
        "[{code}] first_check {first_check_ms:.0}ms rss={:.0}",
        vm_hwm_mib()
    );

    let t = Instant::now();
    let engine2 = build_engine(code);
    let warm_rebuild_ms = ms(t.elapsed());
    let rule_count = engine2.rule_count();
    drop(engine2);
    eprintln!(
        "[{code}] warm_rebuild {warm_rebuild_ms:.0}ms rss={:.0}",
        vm_hwm_mib()
    );

    let t = Instant::now();
    let second = engine.check(sample(code));
    let second_check_ms = ms(t.elapsed());

    let rounds = 20;
    let t = Instant::now();
    for _ in 0..rounds {
        let result = engine.check(sample(code));
        std::hint::black_box(&result);
    }
    let steady_check_ms = ms(t.elapsed()) / rounds as f64;

    let steady_rss = vm_hwm_mib();
    drop(engine);

    format!(
        "bench_engine_build lang={code}#{index} cold_build_ms={cold_build_ms:.0} warm_rebuild_ms={warm_rebuild_ms:.0} \
first_check_ms={first_check_ms:.0} second_check_ms={second_check_ms:.0} steady_check_ms={steady_check_ms:.2} \
first_matches={} second_matches={} rule_count={} peak_rss_mib={steady_rss:.0}\n",
        first.matches.len(),
        second.matches.len(),
        rule_count,
    )
}

fn phases_lang(code: &str) -> String {
    let short = code;
    let dir = resource_dir(short);
    let mut out = format!(
        "bench_engine_phases lang={short} start_rss_mib={:.0}\n",
        vm_hwm_mib()
    );

    if let Some(stem) = tagger_stem(short) {
        let t = Instant::now();
        let dict = og_tagger::generic_tagger::shared_dictionary(&dir, stem);
        let dict_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  dict_load[{stem}] ms={dict_ms:.0} loaded={} rss_mib={:.0}\n",
            dict.is_some(),
            vm_hwm_mib()
        ));

        let t = Instant::now();
        let tagger = og_tagger::GenericTagger::from_resource_dir(&dir, stem);
        let tagger_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  tagger_build[{stem}] ms={tagger_ms:.0} has_dictionary={} rss_mib={:.0}\n",
            tagger.has_dictionary(),
            vm_hwm_mib()
        ));
    }

    if short == "de" {
        let t = Instant::now();
        let tagger = og_tagger::GermanTagger::from_resource_dir(&dir);
        let tagger_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  tagger_build[german] ms={tagger_ms:.0} loaded={} rss_mib={:.0}\n",
            tagger.is_some(),
            vm_hwm_mib()
        ));
    }

    if short == "eo" {
        let t = Instant::now();
        let tagger = og_tagger::EsperantoTagger::from_resource_dir(&dir);
        let tagger_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  tagger_build[esperanto] ms={tagger_ms:.0} loaded={} rss_mib={:.0}\n",
            tagger.is_some(),
            vm_hwm_mib()
        ));
    }

    if let Some(stem) = synth_stem(short) {
        let t = Instant::now();
        let synth = og_tagger::morfologik::shared_synth_dictionary(&dir, stem);
        let synth_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  synth_load[{stem}] ms={synth_ms:.0} loaded={} rss_mib={:.0}\n",
            synth.is_some(),
            vm_hwm_mib()
        ));
    }

    if short == "en" {
        let t = Instant::now();
        drop(og_tagger::EnglishTagger::from_resource_dir(&dir));
        let tagger_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  tagger_build[english] ms={tagger_ms:.0} rss_mib={:.0}\n",
            vm_hwm_mib()
        ));
        let t = Instant::now();
        let support = og_rules::spelling_support::SpellingSupport::dictionary_from_dir(&dir);
        let support_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  spelling_dictionary_from_dir ms={support_ms:.0} loaded={} rss_mib={:.0}\n",
            support.is_some(),
            vm_hwm_mib()
        ));
    }

    let mut rule_total = 0usize;
    for file in ["grammar.xml", "style.xml"] {
        let path = rules_dir(short).join(file);
        if !path.exists() {
            continue;
        }
        let t = Instant::now();
        let xml = std::fs::read_to_string(&path).expect("grammar xml");
        let rule_set = og_xml::compiler::XmlCompiler::new()
            .compile_file_with_base(&xml, path.parent())
            .expect("compile");
        let compile_ms = ms(t.elapsed());
        rule_total += rule_set.rules.len();
        out.push_str(&format!(
            "  xml_compile[{file}] ms={compile_ms:.0} rules={} rss_mib={:.0}\n",
            rule_set.rules.len(),
            vm_hwm_mib()
        ));
    }

    let disambig_path = dir.join("disambiguation.xml");
    if disambig_path.exists() {
        let t = Instant::now();
        let xml = std::fs::read_to_string(&disambig_path).expect("disambig xml");
        let disambig = og_tagger::XmlDisambiguator::from_xml(&xml).expect("disambiguator");
        let disambig_ms = ms(t.elapsed());
        out.push_str(&format!(
            "  disambig_compile ms={disambig_ms:.0} rules={} rss_mib={:.0}\n",
            disambig.rule_count(),
            vm_hwm_mib()
        ));
    }

    let t = Instant::now();
    let engine = build_engine(code);
    let warm_engine_ms = ms(t.elapsed());
    let t = Instant::now();
    let result = engine.check(sample(code));
    let warm_check_ms = ms(t.elapsed());
    out.push_str(&format!(
        "  engine_build_all_warm ms={warm_engine_ms:.0} steady_check_ms={warm_check_ms:.2} matches={} rule_count={} peak_rss_mib={:.0}\n",
        result.matches.len(),
        engine.rule_count(),
        vm_hwm_mib()
    ));
    let _ = rule_total;
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: bench_engine_build <lang>... | bench_engine_build --phases <lang>...");
        return;
    }
    if args[0] == "--phases" {
        wait_for_mem(2500.0);
        let code = args[1].clone();
        let report = phases_lang(&code);
        print!("{report}");
        let _ = std::fs::write(format!("/tmp/bench_engine_phases_{code}.txt"), &report);
        return;
    }
    wait_for_mem(3500.0);
    for (index, code) in args.iter().enumerate() {
        let line = bench_lang(code, index);
        print!("{line}");
        let _ = std::fs::write(format!("/tmp/bench_engine_build_{code}.txt"), &line);
    }
}
