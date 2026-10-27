use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probability {
    pub prob: f64,
    pub coverage: f32,
    pub total_count: u64,
}

pub trait LanguageModel: Send + Sync {
    fn get_count(&self, tokens: &[&str]) -> u64;
    fn get_total_token_count(&self) -> u64;
    fn get_probability(&self, context: &[&str]) -> Probability;
}

impl LanguageModel for Arc<dyn LanguageModel> {
    fn get_probability(&self, context: &[&str]) -> Probability {
        (**self).get_probability(context)
    }

    fn get_count(&self, tokens: &[&str]) -> u64 {
        (**self).get_count(tokens)
    }

    fn get_total_token_count(&self) -> u64 {
        (**self).get_total_token_count()
    }
}

pub fn get_pseudo_probability_from_ref(model: &dyn LanguageModel, context: &[&str]) -> Probability {
    let total_token_count = model.get_total_token_count();
    let mut max_coverage = 0usize;
    let mut coverage = 0usize;

    let Some(&first) = context.first() else {
        return Probability {
            prob: 0.0,
            coverage: 0.0,
            total_count: 0,
        };
    };
    let first_word_count = model.get_count(&[first]);
    max_coverage += 1;
    if first_word_count > 0 {
        coverage += 1;
    }
    let mut p = (first_word_count as f64 + 1.0) / (total_token_count as f64 + 1.0);

    let mut total_count = 0u64;
    for i in 2..=context.len() {
        let sub = &context[..i];
        let phrase_count = model.get_count(sub);
        if sub.len() == 3 {
            total_count = phrase_count;
        }
        let this_p = (phrase_count as f64 + 1.0) / (first_word_count as f64 + 1.0);
        max_coverage += 1;
        if phrase_count > 0 {
            coverage += 1;
        }
        p *= this_p;
    }
    Probability {
        prob: p,
        coverage: coverage as f32 / max_coverage as f32,
        total_count,
    }
}

pub struct DiskLanguageModel {
    grams: [GramStore; 4],
    total_token_count: u64,
}

enum GramStore {
    Map(HashMap<Box<str>, u64>),
    SortedTsv(SortedTsvLookup),
}

impl GramStore {
    fn get(&self, key: &str) -> Option<u64> {
        match self {
            GramStore::Map(map) => map.get(key).copied(),
            GramStore::SortedTsv(tsv) => tsv.get(key),
        }
    }
}

pub struct SortedTsvLookup {
    file: fs::File,
    len: u64,
}

impl SortedTsvLookup {
    fn open(path: &Path) -> Option<Self> {
        let file = fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        if len == 0 {
            return None;
        }
        Some(Self { file, len })
    }

    fn read_some(&self, pos: u64, buf: &mut [u8]) -> Option<usize> {
        if pos >= self.len || buf.is_empty() {
            return Some(0);
        }
        file_read_at(&self.file, buf, pos).ok()
    }

    fn line_at<'a>(&self, pos: u64, buf: &'a mut Vec<u8>) -> Option<&'a [u8]> {
        let mut start = pos;
        let mut cap = 512usize;
        let mut retried = false;
        loop {
            buf.clear();
            buf.resize(cap, 0);
            let n = self.read_some(start, buf)?;
            buf.truncate(n);
            if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                buf.truncate(nl);
            } else if start + (n as u64) < self.len {
                cap = cap.saturating_mul(2);
                continue;
            }
            while buf.last() == Some(&b'\r') {
                buf.pop();
            }
            if buf.is_empty() && !retried && start < self.len {
                retried = true;
                start += 1;
                continue;
            }
            return Some(buf);
        }
    }

    pub fn get(&self, key: &str) -> Option<u64> {
        let mut buf = Vec::with_capacity(512);
        let first = self.line_at(0, &mut buf)?;
        if first.is_empty() {
            return None;
        }
        match compare_line(first, key) {
            Some((std::cmp::Ordering::Equal, count)) => return Some(count),
            Some((std::cmp::Ordering::Greater, _)) => return None,
            _ => {}
        }
        let mut lo = 0u64;
        let mut hi = self.len;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if mid <= lo {
                break;
            }
            let ls = self.line_start_of(mid);
            let line = self.line_at(ls, &mut buf)?;
            if line.is_empty() {
                break;
            }
            match compare_line(line, key) {
                Some((std::cmp::Ordering::Equal, count)) => return Some(count),
                Some((std::cmp::Ordering::Less, _)) => {
                    lo = ls + line.len() as u64 + 1;
                }
                Some((std::cmp::Ordering::Greater, _)) => hi = ls,
                None => break,
            }
        }
        None
    }

    fn line_start_of(&self, pos: u64) -> u64 {
        let window = 512u64;
        let base = pos.saturating_sub(window);
        let mut buf = vec![0u8; (pos - base) as usize];
        if self.read_some(base, &mut buf).is_none() {
            return pos;
        }
        match buf.iter().rposition(|b| *b == b'\n') {
            Some(idx) => base + idx as u64 + 1,
            None => {
                if base == 0 {
                    0
                } else {
                    self.line_start_of(base)
                }
            }
        }
    }
}

#[cfg(unix)]
fn file_read_at(file: &fs::File, buf: &mut [u8], pos: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, pos)
}

#[cfg(windows)]
fn file_read_at(file: &fs::File, buf: &mut [u8], pos: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, pos)
}

fn compare_line(line: &[u8], key: &str) -> Option<(std::cmp::Ordering, u64)> {
    let tab = line.iter().position(|&b| b == b'\t')?;
    let (term, count) = line.split_at(tab);
    let count = std::str::from_utf8(&count[1..])
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some((term.cmp(key.as_bytes()), count))
}

impl DiskLanguageModel {
    pub fn from_dir(dir: &Path) -> Option<Self> {
        let mut grams: [Option<SortedTsvLookup>; 4] = Default::default();
        let mut total_token_count: Option<u64> = None;
        let mut found_any = false;
        for (n, slot) in grams.iter_mut().enumerate().skip(1) {
            let path: PathBuf = dir.join(format!("{n}grams.tsv"));
            if !path.exists() {
                continue;
            }
            let Some(lookup) = SortedTsvLookup::open(&path) else {
                continue;
            };
            found_any = true;
            *slot = Some(lookup);
            if n == 1 {
                let meta = fs::read_to_string(dir.join(format!("{n}grams.meta")))
                    .ok()
                    .and_then(|m| {
                        m.lines().find_map(|l| {
                            l.strip_prefix("total_token_count\t")
                                .and_then(|v| v.trim().parse::<u64>().ok())
                        })
                    });
                if let Some(total) = meta {
                    total_token_count = Some(total);
                }
            }
        }
        if !found_any {
            return None;
        }
        Some(Self {
            grams: [
                GramStore::Map(HashMap::new()),
                grams[1]
                    .take()
                    .map(GramStore::SortedTsv)
                    .unwrap_or(GramStore::Map(HashMap::new())),
                grams[2]
                    .take()
                    .map(GramStore::SortedTsv)
                    .unwrap_or(GramStore::Map(HashMap::new())),
                grams[3]
                    .take()
                    .map(GramStore::SortedTsv)
                    .unwrap_or(GramStore::Map(HashMap::new())),
            ],
            total_token_count: total_token_count.unwrap_or(1).max(1),
        })
    }

    pub fn shared_for_language(lang_code: &str) -> Option<Arc<Self>> {
        static CACHE: std::sync::OnceLock<
            std::sync::Mutex<HashMap<String, Option<Arc<DiskLanguageModel>>>>,
        > = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let mut guard = cache.lock().ok()?;
        if let Some(hit) = guard.get(lang_code) {
            return hit.clone();
        }
        let loaded = find_ngram_dir().and_then(|root| {
            let candidates = [root.join("extracted").join(lang_code), root.join(lang_code)];
            candidates
                .iter()
                .find_map(|dir| Self::from_dir(dir))
                .map(Arc::new)
        });
        guard.insert(lang_code.to_string(), loaded.clone());
        loaded
    }

    pub fn from_pairs(entries: &[(Vec<&str>, u64)]) -> Self {
        let mut maps: [HashMap<Box<str>, u64>; 4] = Default::default();
        let mut total_token_count = 0u64;
        for (tokens, count) in entries {
            let n = tokens.len().min(4);
            if n == 0 {
                continue;
            }
            let key: Box<str> = tokens.join(" ").into();
            maps[n].insert(key, *count);
            if n == 1 {
                total_token_count += count;
            }
        }
        Self {
            grams: [
                GramStore::Map(maps[0].clone()),
                GramStore::Map(maps[1].clone()),
                GramStore::Map(maps[2].clone()),
                GramStore::Map(maps[3].clone()),
            ],
            total_token_count,
        }
    }
}

impl LanguageModel for DiskLanguageModel {
    fn get_probability(&self, context: &[&str]) -> Probability {
        get_pseudo_probability_from_ref(self, context)
    }

    fn get_count(&self, tokens: &[&str]) -> u64 {
        if tokens.is_empty() || tokens.len() > 4 {
            return 0;
        }
        let key: String = tokens.join(" ");
        self.grams[tokens.len()].get(&key).unwrap_or(0)
    }

    fn get_total_token_count(&self) -> u64 {
        self.total_token_count
    }
}

fn find_ngram_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("ngram-data"),
        PathBuf::from("../ngram-data"),
        PathBuf::from("/home/agent/opengrammar-rs/ngram-data"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub type SharedModel = Arc<dyn LanguageModel>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_lookups_stay_correct_under_concurrent_reads() {
        let dir = PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/de");
        if !dir.join("1grams.tsv").exists() {
            eprintln!("Skipping: extracted German ngram data not present");
            return;
        }
        let model = Arc::new(
            DiskLanguageModel::from_dir(&dir).expect("model must load from extracted dir"),
        );
        let probes = ["Leerzeile", "Lehrzeile", "Haus", "der", "Zebra"];
        let expected: Vec<u64> = probes.iter().map(|w| model.get_count(&[w])).collect();
        let expected = Arc::new(expected);
        let mut handles = Vec::new();
        for _ in 0..4 {
            let model = Arc::clone(&model);
            let expected = Arc::clone(&expected);
            handles.push(std::thread::spawn(move || {
                for _ in 0..2000 {
                    for (word, want) in probes.iter().zip(expected.iter()) {
                        assert_eq!(model.get_count(&[word]), *want);
                    }
                }
            }));
        }
        for handle in handles {
            handle.join().expect("no thread may panic");
        }
    }

    #[test]
    fn chain_rule_matches_java_pseudo_probability() {
        let model = DiskLanguageModel::from_pairs(&[
            (vec!["the"], 1000),
            (vec!["quick"], 200),
            (vec!["the", "quick"], 50),
            (vec!["the", "quick", "fox"], 10),
        ]);
        let p = model.get_probability(&["the", "quick", "fox"]);
        assert!((p.prob - (1001.0 / 1201.0) * (51.0 / 1001.0) * (11.0 / 1001.0)).abs() < 1e-12);
        assert_eq!(p.coverage, 1.0);
        assert_eq!(p.total_count, 10);
    }

    #[test]
    fn german_extracted_index_lookups() {
        let dir = PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/de");
        if !dir.join("1grams.tsv").exists() {
            eprintln!("Skipping: extracted German ngram data not present");
            return;
        }
        let Some(model) = DiskLanguageModel::from_dir(&dir) else {
            panic!("model must load from extracted dir");
        };
        assert_eq!(model.get_count(&["der"]), 1446429476);
        assert!(model.get_count(&["der", "Haus"]) > 0, "2gram lookup");
        assert!(
            model.get_count(&["Berlin", "der", "Stadt"]) > 0,
            "3gram lookup"
        );
        assert_eq!(model.get_total_token_count(), 2057002144632);
        let prob = model.get_probability(&["der", "Haus"]);
        assert!(prob.prob > 0.0 && prob.prob < 1.0);
    }

    #[test]
    fn shared_model_resolves_extracted_dirs_per_language() {
        for lang in ["en", "de"] {
            let dir = PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted").join(lang);
            if !dir.join("1grams.tsv").exists() {
                eprintln!("Skipping {lang}: extracted ngram data not present");
                continue;
            }
            let model = DiskLanguageModel::shared_for_language(lang).unwrap_or_else(|| {
                panic!("shared model for {lang} must resolve the extracted dir")
            });
            assert!(model.get_total_token_count() > 0);
        }
    }

    #[test]
    fn english_extracted_index_lookups() {
        let dir = PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/en");
        if !dir.join("2grams.tsv").exists() {
            eprintln!("Skipping: extracted English ngram data not present");
            return;
        }
        let Some(model) = DiskLanguageModel::from_dir(&dir) else {
            panic!("model must load from extracted en dir");
        };
        assert_eq!(model.get_count(&["the"]), 19966238696);
        assert_eq!(model.get_count(&["in", "the"]), 1759394592);
        assert_eq!(model.get_count(&["of", "the"]), 3385103919);
        assert!(
            model.get_count(&["in", "the", "morning"]) > 0,
            "3gram lookup"
        );
    }

    #[test]
    fn unknown_words_get_smoothed_probability() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["the"], 100)]);
        let p = model.get_probability(&["zebra"]);
        assert!((p.prob - 1.0 / 101.0).abs() < 1e-12);
        assert_eq!(p.coverage, 0.0);
    }

    #[test]
    fn arc_dyn_model_delegates_to_the_inner_model() {
        let shared: Arc<dyn LanguageModel> = Arc::new(DiskLanguageModel::from_pairs(&[
            (vec!["the"], 100),
            (vec!["the", "cat"], 10),
        ]));
        assert_eq!(shared.get_total_token_count(), 100);
        assert_eq!(shared.get_count(&["the", "cat"]), 10);
        let p = shared.get_probability(&["the", "cat"]);
        assert!((p.prob - 1.0 * (11.0 / 101.0)).abs() < 1e-12);
    }

    #[test]
    fn empty_context_returns_zeroed_probability() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["the"], 100)]);
        let p = model.get_probability(&[]);
        assert_eq!(
            p,
            Probability {
                prob: 0.0,
                coverage: 0.0,
                total_count: 0
            }
        );
    }

    #[test]
    fn from_pairs_skips_empty_entries_and_bounds_get_count() {
        let model = DiskLanguageModel::from_pairs(&[(vec![], 7), (vec!["x"], 3)]);
        assert_eq!(model.get_total_token_count(), 3);
        assert_eq!(model.get_count(&[]), 0);
        assert_eq!(model.get_count(&["a", "b", "c", "d", "e"]), 0);
    }

    #[test]
    fn tsv_lookup_answers_from_the_first_line_and_before_it() {
        let dir = scratch_dir("first-line");
        fs::write(dir.join("1grams.tsv"), "b\t2\nc\t3\n").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["b"]), 2);
        assert_eq!(model.get_count(&["a"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_strips_carriage_returns() {
        let dir = scratch_dir("crlf");
        fs::write(dir.join("1grams.tsv"), "alpha\t10\r\nbeta\t20\r\n").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["alpha"]), 10);
        assert_eq!(model.get_count(&["beta"]), 20);
        assert_eq!(model.get_count(&["alphabet"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_breaks_off_the_search_on_a_malformed_line() {
        let dir = scratch_dir("malformed");
        fs::write(dir.join("1grams.tsv"), "aa\t1\nbb\t2\nno_tab_line\nzz\t9\n").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["aa"]), 1);
        assert_eq!(model.get_count(&["bb"]), 0);
        assert_eq!(model.get_count(&["m"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_stops_when_the_tail_is_all_carriage_returns() {
        let dir = scratch_dir("cr-tail");
        fs::write(dir.join("1grams.tsv"), "a\t1\nb\t2\n\r").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["b"]), 2);
        assert_eq!(model.get_count(&["c"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_ends_the_search_on_a_run_of_empty_lines() {
        let dir = scratch_dir("empty-run");
        fs::write(dir.join("1grams.tsv"), "aa\t1\nbb\t2\n\r\r\r\r\r\r\r\r").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["aa"]), 1);
        assert_eq!(model.get_count(&["z"]), 0);
        cleanup_dir(&dir);

        let dir = scratch_dir("newline-run");
        fs::write(dir.join("1grams.tsv"), "aa\t1\n\n\n").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["aa"]), 1);
        assert_eq!(model.get_count(&["zz"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_grows_its_buffer_for_lines_longer_than_512_bytes() {
        let long_a = format!("a{}\t1\n", "x".repeat(700));
        let long_z = format!("z{}\t2\n", "y".repeat(700));
        let dir = scratch_dir("long-lines");
        fs::write(dir.join("1grams.tsv"), format!("{long_a}{long_z}")).unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&[&format!("a{}", "x".repeat(700))]), 1);
        assert_eq!(model.get_count(&[&format!("z{}", "y".repeat(700))]), 2);
        assert_eq!(model.get_count(&["m"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_recurses_past_a_1500_byte_line_to_find_its_start() {
        let long_first = format!("a{}\t1\n", "x".repeat(1498));
        let dir = scratch_dir("recurse");
        fs::write(dir.join("1grams.tsv"), format!("{long_first}zz\t9\n")).unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&[&format!("a{}", "x".repeat(1498))]), 1);
        assert_eq!(model.get_count(&["zz"]), 9);
        assert_eq!(model.get_count(&["m"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookup_rejects_a_file_whose_first_line_is_blank() {
        let dir = scratch_dir("blank-first");
        fs::write(dir.join("1grams.tsv"), "\n\r\nb\t2\n").unwrap();
        let model = DiskLanguageModel::from_dir(&dir).unwrap();
        assert_eq!(model.get_count(&["b"]), 0);
        cleanup_dir(&dir);
    }

    #[test]
    fn from_dir_rejects_dirs_without_usable_grams() {
        let empty = scratch_dir("no-grams");
        assert!(DiskLanguageModel::from_dir(&empty).is_none());
        cleanup_dir(&empty);

        let blank = scratch_dir("blank-gram");
        fs::write(blank.join("1grams.tsv"), "").unwrap();
        assert!(DiskLanguageModel::from_dir(&blank).is_none());
        cleanup_dir(&blank);
    }

    #[test]
    fn from_dir_reads_the_meta_total_and_defaults_to_one() {
        let with_meta = scratch_dir("meta");
        fs::write(with_meta.join("1grams.tsv"), "a\t1\nb\t2\n").unwrap();
        fs::write(
            with_meta.join("1grams.meta"),
            "junk line\ntotal_token_count\t12345\n",
        )
        .unwrap();
        let model = DiskLanguageModel::from_dir(&with_meta).unwrap();
        assert_eq!(model.get_total_token_count(), 12345);
        cleanup_dir(&with_meta);

        let without_meta = scratch_dir("no-meta");
        fs::write(without_meta.join("1grams.tsv"), "a\t1\nb\t2\n").unwrap();
        let model = DiskLanguageModel::from_dir(&without_meta).unwrap();
        assert_eq!(model.get_total_token_count(), 1);
        let p = model.get_probability(&["a"]);
        assert!((p.prob - 2.0 / 2.0).abs() < 1e-12);
        cleanup_dir(&without_meta);
    }

    #[test]
    fn shared_model_caches_unresolvable_languages() {
        assert!(DiskLanguageModel::shared_for_language("zz").is_none());
        assert!(DiskLanguageModel::shared_for_language("zz").is_none());
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oglm-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_dir(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
}
