//! The n-gram lookup metric: a fixed key list, timed against the English
//! data the engine itself loads, with a synthetic fallback for hosts that do
//! not have the ~15.9 GB of TSVs.

use crate::WARMUPS;
use og_languagemodel::{DiskLanguageModel, LanguageModel};
use std::fmt::Write as _;
use std::hint::black_box;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Looked up once per pass, pre-tokenized so the timed region holds the
/// lookup and nothing else: 1-, 2- and 3-grams around the bench corpora,
/// frequent phrases next to typo-bearing likely misses.
pub static NGRAM_KEYS: [&[&str]; 16] = [
    &["the"],
    &["problem"],
    &["probleme"],
    &["sentense"],
    &["Their"],
    &["of", "the"],
    &["there", "is"],
    &["their", "is"],
    &["lazy", "dog"],
    &["lazy", "dogg"],
    &["tree", "cats"],
    &["in", "the", "morning"],
    &["there", "is", "a"],
    &["their", "is", "a"],
    &["the", "quick", "brown"],
    &["I", "has", "tree"],
];

/// Which data a lookup run used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NgramSource {
    /// `ngram-data/extracted/en`, resolved exactly as the engine resolves it
    /// (`DiskLanguageModel::shared_for_language("en")`).
    Real,
    /// Synthetic sorted TSVs written to this directory.
    Synthetic(PathBuf),
}

/// Whether the engine's n-gram rules have their English data on this host.
pub fn en_ngram_data_available() -> bool {
    DiskLanguageModel::shared_for_language("en").is_some()
}

/// The English model the engine uses or, when that data is unreachable, a
/// synthetic one opened through the same public API (`from_dir`), so the
/// lookups run the same seek-and-read binary search over a sorted TSV.
pub fn english_ngram_model() -> io::Result<(Arc<DiskLanguageModel>, NgramSource)> {
    if let Some(model) = DiskLanguageModel::shared_for_language("en") {
        return Ok((model, NgramSource::Real));
    }
    let dir = synthetic_ngram_dir();
    write_synthetic_ngrams(&dir)?;
    let model = DiskLanguageModel::from_dir(&dir).ok_or_else(|| {
        io::Error::other(format!("{} did not open as n-gram data", dir.display()))
    })?;
    Ok((Arc::new(model), NgramSource::Synthetic(dir)))
}

/// `<target>/og-bench/synthetic-ngrams/en`, where `<target>` is
/// `$CARGO_TARGET_DIR` or the workspace's `target/`.
pub fn synthetic_ngram_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .join("og-bench")
        .join("synthetic-ngrams")
        .join("en")
}

/// Filler rows per n-gram order in the synthetic data.
const SYNTHETIC_ROWS: usize = 100_000;

/// The count the synthetic data gives `NGRAM_KEYS[index]`: keys at even
/// positions are planted as hits, the others stay misses.
pub fn planted_count(index: usize) -> Option<u64> {
    (index % 2 == 0).then(|| 1_000 + index as u64)
}

/// Writes `1grams.tsv`..`3grams.tsv` (plus `1grams.meta`) into `dir`: the
/// planted keys and 100k pseudo-random lowercase n-grams per order, sorted
/// byte-wise like the real files. The content is a fixed function of the
/// seed, so every host times the same files. Rewritten on every call.
pub fn write_synthetic_ngrams(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut rng = Lcg(0x6f67_2d62_656e_6368);
    let mut total_token_count = 0u64;
    for order in 1..=3usize {
        let mut rows: Vec<(String, u64)> = NGRAM_KEYS
            .iter()
            .enumerate()
            .filter(|(_, key)| key.len() == order)
            .filter_map(|(index, key)| planted_count(index).map(|count| (key.join(" "), count)))
            .collect();
        for _ in 0..SYNTHETIC_ROWS {
            let words: Vec<String> = (0..order).map(|_| rng.word()).collect();
            rows.push((words.join(" "), 1 + rng.next_u64() % 100_000));
        }
        // Stable sort: on a key collision the planted row (pushed first)
        // survives the dedup, so planted counts never depend on the filler.
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.dedup_by(|later, earlier| later.0 == earlier.0);
        if order == 1 {
            total_token_count = rows.iter().map(|(_, count)| count).sum();
        }
        let mut tsv = String::with_capacity(rows.len() * 24);
        for (key, count) in &rows {
            let _ = writeln!(tsv, "{key}\t{count}");
        }
        std::fs::write(dir.join(format!("{order}grams.tsv")), tsv)?;
    }
    std::fs::write(
        dir.join("1grams.meta"),
        format!("total_token_count\t{total_token_count}\n"),
    )
}

/// A 64-bit LCG (Knuth's MMIX constants): deterministic everywhere.
struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    /// A lowercase ASCII word of 2..=9 letters.
    fn word(&mut self) -> String {
        let len = 2 + self.below(8);
        (0..len)
            .map(|_| char::from(b'a' + self.below(26) as u8))
            .collect()
    }
}

/// Times one `get_count` per key in `NGRAM_KEYS` per pass: `WARMUPS`
/// untimed passes, then `iters` timed ones. Returns the latency of every
/// timed lookup in microseconds, and how many keys hit.
pub fn time_lookups<M: LanguageModel + ?Sized>(model: &M, iters: usize) -> (Vec<f64>, usize) {
    let mut samples = Vec::with_capacity(iters * NGRAM_KEYS.len());
    let mut hits = 0;
    for pass in 0..WARMUPS + iters {
        for key in &NGRAM_KEYS {
            let start = Instant::now();
            let count = model.get_count(black_box(*key));
            let elapsed = start.elapsed();
            if pass == 0 && black_box(count) > 0 {
                hits += 1;
            }
            if pass >= WARMUPS {
                samples.push(elapsed.as_secs_f64() * 1e6);
            }
        }
    }
    (samples, hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_data_opens_and_answers_planted_keys() {
        let dir = std::env::temp_dir().join(format!("og-bench-synthetic-{}", std::process::id()));
        write_synthetic_ngrams(&dir).expect("synthetic TSVs written");
        let first = std::fs::read(dir.join("2grams.tsv")).expect("2grams.tsv");
        let model = DiskLanguageModel::from_dir(&dir).expect("synthetic dir opens");
        let mut planted = 0;
        for (index, key) in NGRAM_KEYS.iter().enumerate() {
            if let Some(count) = planted_count(index) {
                assert_eq!(model.get_count(key), count, "{key:?}");
                planted += 1;
            }
        }
        assert_eq!(planted, 8);
        // The generator only emits lowercase letters, so this cannot hit.
        assert_eq!(model.get_count(&["Zz9", "miss"]), 0);
        assert!(model.get_total_token_count() > 1);

        write_synthetic_ngrams(&dir).expect("rewritten");
        assert_eq!(
            std::fs::read(dir.join("2grams.tsv")).expect("2grams.tsv"),
            first,
            "the synthetic data is deterministic"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn synthetic_rows_are_sorted_byte_wise_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("og-bench-sorted-{}", std::process::id()));
        write_synthetic_ngrams(&dir).expect("synthetic TSVs written");
        for order in 1..=3 {
            let tsv = std::fs::read_to_string(dir.join(format!("{order}grams.tsv"))).unwrap();
            let keys: Vec<&str> = tsv.lines().map(|line| line.split('\t').next().unwrap()).collect();
            // 2-letter filler words collide heavily in a 676-word space, so
            // the deduped 1-gram file lands around 84k rows.
            assert!(keys.len() > 80_000, "order {order}: {}", keys.len());
            assert!(
                keys.windows(2).all(|pair| pair[0].as_bytes() < pair[1].as_bytes()),
                "order {order} is strictly ascending"
            );
            assert!(keys.iter().all(|key| key.split(' ').count() == order));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lookups_are_timed_per_key_after_warmups() {
        let model = DiskLanguageModel::from_pairs(&[
            (vec!["the"], 100),
            (vec!["of", "the"], 10),
            (vec!["in", "the", "morning"], 1),
        ]);
        let (samples, hits) = time_lookups(&model, 3);
        assert_eq!(samples.len(), 3 * NGRAM_KEYS.len());
        assert!(samples.iter().all(|us| us.is_finite() && *us >= 0.0));
        assert_eq!(hits, 3);
    }

    #[test]
    fn synthetic_dir_is_under_target() {
        if std::env::var_os("CARGO_TARGET_DIR").is_none() {
            let dir = synthetic_ngram_dir();
            assert!(
                dir.ends_with("target/og-bench/synthetic-ngrams/en"),
                "{dir:?}"
            );
        }
    }
}
