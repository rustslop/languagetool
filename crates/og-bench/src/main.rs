//! Builds the English engine once, times the fixed corpora and the n-gram
//! lookups, and prints the metrics as exactly one JSON line on stdout.
//! Progress, spreads and the parity canary go to stderr.

use og_bench::{Command, Corpus, Metrics, NgramSource, WARMUPS};
use og_langs::engine::LanguageEngine;
use og_languagemodel::LanguageModel as _;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let iters = match og_bench::parse_args(std::env::args().skip(1)) {
        Ok(Command::Run { iters }) => iters,
        Ok(Command::Help) => {
            eprint!("{}", og_bench::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("og-bench: {error}");
            eprint!("{}", og_bench::USAGE);
            return ExitCode::from(2);
        }
    };
    match run(iters) {
        Ok(metrics) => {
            println!("{}", metrics.to_json_line());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("og-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(iters: usize) -> Result<Metrics, String> {
    if og_langs::engine::find_lt_resource_dir().is_none() {
        return Err("LanguageTool resources not found \
                    (languagetool/languagetool-language-modules): the English engine \
                    would be built without its rules, so its timings would mean nothing"
            .to_string());
    }
    eprintln!("og-bench: {WARMUPS} warmups + {iters} timed runs per median");

    let start = Instant::now();
    let engine = LanguageEngine::english();
    let build_ms = millis(start.elapsed());
    eprintln!(
        "og-bench: build: {build_ms:.1} ms ({} rules, {} text-level rules)",
        engine.rule_count(),
        engine.text_level_rule_ids().len()
    );

    let en_ngram_data = og_bench::en_ngram_data_available();
    let [clean_ms, err_ms, multi_ms] = og_bench::CORPORA
        .each_ref()
        .map(|corpus| time_corpus(&engine, corpus, en_ngram_data, iters));

    let ngram_lookup_us = time_ngram_lookups(iters)?;

    let peak_rss_mb = og_bench::peak_rss_mb().unwrap_or_else(|| {
        eprintln!("og-bench: VmHWM unavailable (no /proc/self/status); reporting null");
        f64::NAN
    });
    eprintln!("og-bench: peak RSS (VmHWM): {peak_rss_mb:.1} MiB");

    Ok(Metrics {
        build_ms,
        clean_ms,
        err_ms,
        multi_ms,
        ngram_lookup_us,
        peak_rss_mb,
    })
}

/// The median wall time of `engine.check` on the corpus. The first warmup's
/// result is compared with the committed golden: a timing only compares
/// with an earlier one if the output is unchanged too.
fn time_corpus(engine: &LanguageEngine, corpus: &Corpus, en_ngram_data: bool, iters: usize) -> f64 {
    let mut samples = Vec::with_capacity(iters);
    for run in 0..WARMUPS + iters {
        let start = Instant::now();
        let result = engine.check(black_box(corpus.text));
        let elapsed = start.elapsed();
        if run == 0 {
            eprintln!(
                "og-bench: {}: {} matches, {}",
                corpus.name,
                result.matches.len(),
                og_bench::golden_status(corpus, en_ngram_data, &result.matches)
            );
        }
        drop(black_box(result));
        if run >= WARMUPS {
            samples.push(millis(elapsed));
        }
    }
    let median = og_bench::median(&samples);
    eprintln!(
        "og-bench: {}: median {median:.3} ms {}",
        corpus.name,
        spread(&samples)
    );
    median
}

/// The median latency of one `get_count` over the fixed key list.
fn time_ngram_lookups(iters: usize) -> Result<f64, String> {
    let (model, source) =
        og_bench::english_ngram_model().map_err(|error| format!("n-gram model: {error}"))?;
    match &source {
        NgramSource::Real => {
            eprintln!(
                "og-bench: ngram: real English data via \
                 DiskLanguageModel::shared_for_language(\"en\"), as the engine resolves it"
            );
            if model.get_count(&["the"]) == 0 {
                eprintln!("og-bench: ngram: WARNING: \"the\" has no count; the data looks incomplete");
            }
        }
        NgramSource::Synthetic(dir) => eprintln!(
            "og-bench: ngram: real data unreachable; timing synthetic sorted TSVs in {} \
             (not comparable with real-data numbers)",
            dir.display()
        ),
    }
    let (samples, hits) = og_bench::time_lookups(model.as_ref(), iters);
    let median = og_bench::median(&samples);
    eprintln!(
        "og-bench: ngram: median {median:.3} us per lookup, {} keys ({hits} hits) {}",
        og_bench::NGRAM_KEYS.len(),
        spread(&samples)
    );
    Ok(median)
}

fn millis(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1e3
}

fn spread(samples: &[f64]) -> String {
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    format!("(n={}, min {min:.3}, max {max:.3})", samples.len())
}
