use std::sync::Arc;
use std::time::Instant;

const SENTENCES: &[&str] = &[
    "The quick brown fox jumps over the lazy dog near the river bank.",
    "Scientists have discovered a new species of frog in the Amazon rainforest.",
    "She said that she would come to the party, but she never showed up.",
    "The government has taken its time to catch up with the digital economy.",
    "In my opinion, this is the best solution to the problem we are facing.",
    "He was reading a book about the history of ancient Rome when I called.",
    "The company announced that they will invest in renewable energy projects.",
    "Its important to remember that not everyone shares the same perspective.",
    "We could of done better, but the circumstances were beyond our control.",
    "The children walked slowly towards the old harbour before sunset.",
    "There going to announce the results tomorrow morning at ten o'clock.",
    "Between you and I, the plan seems a little too ambitious for this team.",
    "The manager decided to postpone the meeting until next week.",
    "Researchers found that the drug was effective in treating the disease.",
    "The report highlights the most significant challenges facing the industry.",
];

const GERMAN_SENTENCES: &[&str] = &[
    "Der schnelle braune Fuchs springt über den faulen Hund am Flussufer.",
    "Die Wissenschaftler haben eine neue Froschart im Regenwald entdeckt.",
    "Sie sagte, dass sie zur Party kommen würde, aber sie ist nie aufgetaucht.",
    "Die Regierung hat gebrauchte Zeit, um mit der digitalen Wirtschaft Schritt zu halten.",
    "Nach meiner Meinung ist das die beste Lösung für das Problem.",
    "Er las ein Buch über die Geschichte des alten Roms, als ich anrief.",
    "Die Firma kündigte an, in erneuerbare Energien zu investieren.",
    "Es ist wichtig zu bedenken, dass nicht jeder die gleiche Sichtweise teilt.",
    "Die Kinder gingen langsam zum alten Hafen vor Sonnenuntergang.",
    "Der Zug fährt an jedem Werktag um halb neun Uhr morgens ab.",
];

const BENCH_FILE: &str = "/tmp/bench_engine.txt";

fn build_english() -> Arc<og_langs::engine::LanguageEngine> {
    Arc::new(og_langs::engine::LanguageEngine::english())
}

fn build_german() -> Arc<og_langs::engine::LanguageEngine> {
    Arc::new(og_langs::engine::LanguageEngine::german())
}

struct Sample {
    ms_per_sentence: f64,
    matches: usize,
}

fn measure(
    engine: &og_langs::engine::LanguageEngine,
    label: &str,
    iterations: usize,
    sentences: &[&str],
) -> Sample {
    let mut total_matches = 0usize;
    let start = Instant::now();
    for _ in 0..iterations {
        for sentence in sentences {
            let result = engine.check(sentence);
            total_matches += result.matches.len();
        }
    }
    let elapsed = start.elapsed();
    let sentences_run = sentences.len() * iterations;
    let ms_per_sentence = elapsed.as_secs_f64() * 1000.0 / sentences_run as f64;
    eprintln!(
        "{label}: {sentences_run} sentences in {elapsed:?} => {ms_per_sentence:.3} ms/sentence ({total_matches} matches)"
    );
    Sample {
        ms_per_sentence,
        matches: total_matches,
    }
}

fn persist(summary: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(BENCH_FILE)
    {
        use std::io::Write;
        let _ = writeln!(file, "{summary}");
    }
}

#[test]
#[ignore]
fn bench_engine_check_latency() {
    let build_start = Instant::now();
    let english = build_english();
    let english_build = build_start.elapsed();

    let build_start = Instant::now();
    let german = build_german();
    let german_build = build_start.elapsed();

    eprintln!(
        "engine build: english {english_build:?} ({} rules), german {german_build:?} ({} rules)",
        english.rule_count(),
        german.rule_count()
    );

    let en_cold = measure(&english, "english cold", 1, SENTENCES);
    let en_warm = measure(&english, "english warm", 2, SENTENCES);
    let de_cold = measure(&german, "german cold", 1, GERMAN_SENTENCES);
    let de_warm = measure(&german, "german warm", 1, GERMAN_SENTENCES);

    let summary = format!(
        "en_build_ms={:.1} en_rules={} de_build_ms={:.1} de_rules={} en_cold_ms={:.3} en_warm_ms={:.3} de_cold_ms={:.3} de_warm_ms={:.3} de_over_en_cold={:.2}x de_over_en_warm={:.2}x en_matches={} de_matches={}",
        english_build.as_secs_f64() * 1000.0,
        english.rule_count(),
        german_build.as_secs_f64() * 1000.0,
        german.rule_count(),
        en_cold.ms_per_sentence,
        en_warm.ms_per_sentence,
        de_cold.ms_per_sentence,
        de_warm.ms_per_sentence,
        de_cold.ms_per_sentence / en_cold.ms_per_sentence,
        de_warm.ms_per_sentence / en_warm.ms_per_sentence,
        en_cold.matches + en_warm.matches,
        de_cold.matches + de_warm.matches,
    );
    eprintln!("persisted: {summary}");
    persist(&summary);
}
