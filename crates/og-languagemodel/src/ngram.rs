use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Distance between sampled index entries.
const SAMPLE_STRIDE: u64 = 256 * 1024;

/// Size of the per-probe block: one read covers the 512-byte backscan
/// window plus the start of the probed line, and the converging probes of
/// a bisection usually land inside the block that is already loaded.
const PROBE_BLOCK_CAP: usize = 4096;

pub struct SortedTsvLookup {
    file: fs::File,
    len: u64,
    /// The file's first line, read once: `get` no longer re-reads offset 0
    /// on every lookup. The inner `Option` mirrors `line_at`'s read failure.
    first_line: OnceLock<Option<Box<[u8]>>>,
    /// `(line_start_offset, term)` samples every `SAMPLE_STRIDE` bytes,
    /// built on the first `get` that needs a search window. Interior
    /// mutability is safe behind `&self`: the `OnceLock` builds the index
    /// once and lookups only read it afterwards (no lock is held across a
    /// lookup). The struct is shared through an `Arc` model, so several
    /// threads may race to build it; all of them then use the same index.
    index: OnceLock<Vec<IndexEntry>>,
    #[cfg(test)]
    stats: LookupStats,
}

struct IndexEntry {
    /// File offset of the sampled line's start (a genuine line start).
    line_start_offset: u64,
    /// The bytes before the first tab of the line at that offset, with
    /// `line_at` semantics (an empty line contributes the next line's
    /// term, but keeps its own offset).
    term: Box<[u8]>,
}

#[cfg(test)]
#[derive(Default)]
struct LookupStats {
    disk_reads: AtomicUsize,
    block_line_serves: AtomicUsize,
}

thread_local! {
    /// Scratch for the per-probe block reads, reused across lookups so the
    /// hot path neither allocates nor re-faults a buffer per call.
    static BLOCK_SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

/// A small slice of the file held in memory to serve bisection probes.
struct Block<'a> {
    base: u64,
    data: &'a [u8],
}

impl SortedTsvLookup {
    fn open(path: &Path) -> Option<Self> {
        let file = fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        if len == 0 {
            return None;
        }
        Some(Self {
            file,
            len,
            first_line: OnceLock::new(),
            index: OnceLock::new(),
            #[cfg(test)]
            stats: LookupStats::default(),
        })
    }

    fn read_some(&self, pos: u64, buf: &mut [u8]) -> Option<usize> {
        #[cfg(test)]
        self.stats.disk_reads.fetch_add(1, Ordering::Relaxed);
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
        let first = self
            .first_line
            .get_or_init(|| {
                let mut buf = Vec::with_capacity(512);
                self.line_at(0, &mut buf)
                    .map(|line| line.to_vec().into_boxed_slice())
            })
            .as_deref()?;
        if first.is_empty() {
            return None;
        }
        match compare_line(first, key) {
            Some((std::cmp::Ordering::Equal, count)) => return Some(count),
            Some((std::cmp::Ordering::Greater, _)) => return None,
            _ => {}
        }
        let index = self.index.get_or_init(|| self.build_index());
        // The greatest sampled term <= key: on a sorted file every line
        // with this key lies at or after that sample's line start and
        // strictly before the next sample's line start.
        let at = index.partition_point(|entry| entry.term.as_ref() <= key.as_bytes());
        if at == 0 {
            // The key is below every sampled term. On well-formed data the
            // first-line check above has already answered this; for
            // anything else, keep byte-for-byte compatibility with the
            // original whole-file disk bisection.
            return self.get_via_disk_bisection(key);
        }
        let lo = index[at - 1].line_start_offset;
        let hi = if at < index.len() {
            index[at].line_start_offset
        } else {
            self.len
        };
        BLOCK_SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            self.bisect_with_probe_blocks(&mut scratch, lo, hi, key)
        })
    }

    /// The original whole-file bisection: the first-line check plus the
    /// probe loop over `[0, len)` with every read served from disk. `get`
    /// falls back to this for keys below every sampled term, and the
    /// property test compares `get` against it.
    fn get_via_disk_bisection(&self, key: &str) -> Option<u64> {
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

    /// Walk the file at `SAMPLE_STRIDE` bytes and sample one line per
    /// stride point. Entry 0 is the file's first line. For files smaller
    /// than one stride this is a single entry covering `[0, len)`, so
    /// lookups probe exactly the bounds the whole-file bisection used.
    fn build_index(&self) -> Vec<IndexEntry> {
        let mut entries = Vec::new();
        let mut point = 0u64;
        while point < self.len {
            entries.push(self.sample_entry(point));
            point += SAMPLE_STRIDE;
        }
        entries
    }

    /// Sample the line governing `point`: the start of the line containing
    /// `point`, found with the `line_start_of` backscan over one small disk
    /// window, plus that line's term.
    fn sample_entry(&self, point: u64) -> IndexEntry {
        const WINDOW: u64 = 512;
        let back = point.saturating_sub(WINDOW);
        let ahead = point.saturating_add(WINDOW).min(self.len);
        let mut win = vec![0u8; (ahead - back) as usize];
        let n = match self.read_some(back, &mut win) {
            Some(n) => n,
            None => {
                // Read failure: record a degenerate entry at the stride
                // point; lookups then behave like the unindexed path.
                return IndexEntry {
                    line_start_offset: point,
                    term: Vec::new().into_boxed_slice(),
                };
            }
        };
        win.truncate(n);
        let rel = ((point - back) as usize).min(win.len());
        let line_start = match win[..rel].iter().rposition(|&b| b == b'\n') {
            Some(idx) => back + idx as u64 + 1,
            // No newline in the backscan window: the line containing
            // `point` starts before `back`; recurse on disk exactly like
            // `line_start_of` does.
            None if back == 0 => 0,
            None => self.line_start_of(back),
        };
        // `line_at` semantics for the sampled term: an empty line
        // contributes the next line's bytes (a single skip-retry), but the
        // entry's offset stays the empty line's start.
        let (mut line, from_window) = self
            .sampled_line_bytes(&win, back, line_start)
            .unwrap_or_else(|| (Vec::new(), false));
        if from_window && line.is_empty() && line_start < self.len {
            if let Some((next, _)) = self.sampled_line_bytes(&win, back, line_start + 1) {
                line = next;
            }
        }
        let term = match line.iter().position(|&b| b == b'\t') {
            Some(tab) => line[..tab].to_vec().into_boxed_slice(),
            None => line.into_boxed_slice(),
        };
        IndexEntry {
            line_start_offset: line_start,
            term,
        }
    }

    /// The bytes of the line at `start` (up to the next newline or EOF,
    /// trailing '\r's stripped), served from the sample window when the
    /// line fits inside it, otherwise from disk via `line_at` (which also
    /// grows its buffer for lines longer than 512 bytes). Returns the bytes
    /// and whether they came from the window.
    fn sampled_line_bytes(&self, win: &[u8], win_base: u64, start: u64) -> Option<(Vec<u8>, bool)> {
        let idx = match start.checked_sub(win_base) {
            Some(idx) if (idx as usize) < win.len() => idx as usize,
            _ => {
                let mut buf = Vec::with_capacity(512);
                return self
                    .line_at(start, &mut buf)
                    .map(|line| (line.to_vec(), false));
            }
        };
        if let Some(nl) = win[idx..].iter().position(|&b| b == b'\n') {
            let mut end = idx + nl;
            while end > idx && win[end - 1] == b'\r' {
                end -= 1;
            }
            return Some((win[idx..end].to_vec(), true));
        }
        if win_base + win.len() as u64 >= self.len {
            // The window reached EOF: its remainder is the whole line.
            let mut end = win.len();
            while end > idx && win[end - 1] == b'\r' {
                end -= 1;
            }
            return Some((win[idx..end].to_vec(), true));
        }
        // The line is longer than the window's remainder: read it from disk.
        let mut buf = Vec::with_capacity(512);
        self.line_at(start, &mut buf)
            .map(|line| (line.to_vec(), false))
    }

    /// The `get` bisection loop over `[lo, hi)`, with every probe served
    /// from a small in-memory block: when the probe falls outside the
    /// currently loaded block, one read covers the probe's 512-byte
    /// backscan window plus the start of its line, and the converging
    /// probes of the search usually land inside the block that is already
    /// loaded. A lookup therefore costs a handful of small reads instead
    /// of a pair of reads per probe over the whole file.
    ///
    /// Reading one buffer the size of the whole window (about a stride,
    /// 256 KiB) was measured slower than the unindexed lookups on this
    /// host: the copy of a quarter MiB of hot page-cache pages alone
    /// exceeds the cost of the handful of small reads. Keeping the block
    /// probe-local preserves the win the index makes possible.
    fn bisect_with_probe_blocks(
        &self,
        scratch: &mut Vec<u8>,
        lo: u64,
        hi: u64,
        key: &str,
    ) -> Option<u64> {
        let mut block_base = u64::MAX;
        let mut block_len = 0usize;
        let mut lo = lo;
        let mut hi = hi;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if mid <= lo {
                break;
            }
            if mid < block_base || mid >= block_base + (block_len as u64) {
                let base = mid.saturating_sub(512);
                let want = PROBE_BLOCK_CAP.min((self.len - base) as usize);
                if scratch.len() < want {
                    scratch.resize(want, 0);
                }
                let mut got = 0usize;
                while got < want {
                    let n = self.read_some(base + (got as u64), &mut scratch[got..want])?;
                    if n == 0 {
                        break;
                    }
                    got += n;
                }
                block_base = base;
                block_len = got;
            }
            let block = Block {
                base: block_base,
                data: &scratch[..block_len],
            };
            let ls = self.block_line_start_of(&block, mid);
            let line = self.block_line_at(&block, ls)?;
            if line.is_empty() {
                break;
            }
            match compare_line(&line, key) {
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

    /// `line_start_of` over the block buffer: the same 512-byte backscan
    /// and recursion, with byte-identical semantics. The loaded block
    /// starts exactly at the current probe's backscan base, so only the
    /// recursion (a line longer than 512 bytes whose start lies before the
    /// block) falls back to the original disk-based path for that probe.
    fn block_line_start_of(&self, block: &Block<'_>, pos: u64) -> u64 {
        let window = 512u64;
        let base = pos.saturating_sub(window);
        let block_end = block.base + block.data.len() as u64;
        if base < block.base || pos > block_end {
            return self.line_start_of(pos);
        }
        let start = (base - block.base) as usize;
        let end = (pos - block.base) as usize;
        match block.data[start..end].iter().rposition(|&b| b == b'\n') {
            Some(idx) => base + idx as u64 + 1,
            None => {
                if base == 0 {
                    0
                } else {
                    self.block_line_start_of(block, base)
                }
            }
        }
    }

    /// `line_at` over the block buffer, with byte-identical semantics: the
    /// cap-doubling growth, the trailing-'\r' strip and the single
    /// empty-line skip-retry all match `line_at`. Anything that would read
    /// at or past the block's end (a line longer than the block's reach,
    /// or the skip-retry stepping past the block's final line) falls back
    /// to the disk path for that line.
    fn block_line_at<'b>(&self, block: &'b Block<'_>, pos: u64) -> Option<Cow<'b, [u8]>> {
        let block_end = block.base + block.data.len() as u64;
        let mut start = pos;
        let mut cap = 512usize;
        let mut retried = false;
        loop {
            if start < block.base || start >= block_end {
                let mut buf = Vec::with_capacity(512);
                return self
                    .line_at(start, &mut buf)
                    .map(|line| Cow::Owned(line.to_vec()));
            }
            let off = (start - block.base) as usize;
            let take = cap.min(block.data.len() - off);
            let bytes = &block.data[off..off + take];
            if let Some(nl) = bytes.iter().position(|&b| b == b'\n') {
                let mut end = off + nl;
                while end > off && block.data[end - 1] == b'\r' {
                    end -= 1;
                }
                let line = &block.data[off..end];
                if line.is_empty() && !retried && start < self.len {
                    retried = true;
                    start += 1;
                    continue;
                }
                self.count_block_serve();
                return Some(Cow::Borrowed(line));
            }
            if start + (take as u64) < block_end {
                // The newline sits further into the block: grow the cap,
                // exactly like `line_at`, and take the same bytes from the
                // block.
                cap = cap.saturating_mul(2);
                continue;
            }
            if start + (take as u64) < self.len {
                // The line continues past the block: `line_at` would keep
                // reading the file, so serve this line from disk.
                let mut buf = Vec::with_capacity(cap);
                return self
                    .line_at(start, &mut buf)
                    .map(|line| Cow::Owned(line.to_vec()));
            }
            // EOF within the block: the remainder of the block is the line.
            let mut end = block.data.len();
            while end > off && block.data[end - 1] == b'\r' {
                end -= 1;
            }
            let line = &block.data[off..end];
            if line.is_empty() && !retried && start < self.len {
                retried = true;
                start += 1;
                continue;
            }
            self.count_block_serve();
            return Some(Cow::Borrowed(line));
        }
    }

    #[cfg(test)]
    fn count_block_serve(&self) {
        self.stats.block_line_serves.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(not(test))]
    #[inline]
    fn count_block_serve(&self) {}

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

    // ---- sampled offset index ----

    /// Deterministic xorshift64 PRNG for the property corpus.
    struct TestRng(u64);

    impl TestRng {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next_u64() % bound
        }

        fn chance(&mut self, percent: u64) -> bool {
            self.below(100) < percent
        }
    }

    fn random_term(rng: &mut TestRng) -> String {
        // Mostly short words with occasional giants, so the corpus
        // exercises the cap-doubling growth and the recursive backscan on
        // both paths.
        let len = match rng.below(100) {
            0..=3 => 520 + rng.below(600) as usize,
            4 => 1500 + rng.below(900) as usize,
            _ => 1 + rng.below(10) as usize,
        };
        (0..len)
            .map(|_| char::from(b'a' + rng.below(26) as u8))
            .collect()
    }

    /// One generated sorted TSV: random terms sorted byte-wise, with
    /// optional CRLF endings, duplicate terms, blank lines, malformed
    /// (tab-less) lines at their sorted position, and a ragged tail.
    /// Returns the emitted well-formed (term, count) rows.
    #[allow(clippy::too_many_arguments)]
    fn write_random_sorted_tsv(
        path: &Path,
        rng: &mut TestRng,
        target_bytes: usize,
        crlf_pct: u64,
        dup_pct: u64,
        blank_pct: u64,
        malformed_pct: u64,
        dup_same_count: bool,
    ) -> Vec<(String, u64)> {
        let mut rows: Vec<(String, u64)> = Vec::new();
        let mut bytes = 0usize;
        while bytes < target_bytes {
            let term = random_term(rng);
            bytes += term.len() + 8;
            rows.push((term, 1 + rng.below(1_000_000)));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        // Duplicate terms, adjacent and in sorted position.
        let mut emitted: Vec<(String, u64)> = Vec::with_capacity(rows.len() * 2);
        for row in &rows {
            emitted.push(row.clone());
            if dup_pct > 0 && rng.chance(dup_pct) {
                let count = if dup_same_count {
                    row.1
                } else {
                    1 + rng.below(1_000_000)
                };
                emitted.push((row.0.clone(), count));
            }
        }
        let mut out = String::with_capacity(bytes + 4096);
        let mut well_formed = Vec::new();
        for (term, count) in &emitted {
            if blank_pct > 0 && rng.chance(blank_pct) {
                out.push_str(if rng.chance(50) { "\n" } else { "\r\n" });
            }
            if malformed_pct > 0 && rng.chance(malformed_pct) {
                out.push_str(term);
                out.push_str(if rng.chance(50) { "\n" } else { "\r\n" });
                continue;
            }
            out.push_str(term);
            out.push('\t');
            out.push_str(&count.to_string());
            out.push_str(if crlf_pct > 0 && rng.chance(crlf_pct) {
                "\r\n"
            } else {
                "\n"
            });
            well_formed.push((term.clone(), *count));
        }
        // Ragged tails: no trailing newline, a lone '\r', extra blanks.
        match rng.below(10) {
            0 => {
                // Strip the trailing newline (and any trailing blank tail):
                // the last well-formed row stays, just unterminated.
                while matches!(out.as_bytes().last(), Some(b'\n' | b'\r')) {
                    out.pop();
                }
            }
            1 => out.push('\r'),
            2 => out.push_str("\r\n"),
            3 => out.push_str("\n\n"),
            _ => {}
        }
        fs::write(path, out).unwrap();
        well_formed
    }

    /// Parity of `get` (sampled index + block bisection) against the
    /// original whole-file disk bisection over ~200 seeded random sorted
    /// TSVs of varying shapes and sizes, with hit and miss keys.
    ///
    /// Files below one stride (including the dirty shapes: mid-file blank
    /// and malformed lines, duplicate terms with differing counts) have a
    /// single-entry index, so the window is the whole file, the probe
    /// bounds are identical and results are byte-identical. Stride-crossing
    /// files exercise the windowed search with clean sorted rows, CRLF,
    /// duplicate runs and ragged tails.
    ///
    /// One divergence is structural and tolerated explicitly: a duplicate
    /// term whose copies carry DIFFERENT counts in a multi-entry file. The
    /// reference bisection returns whichever copy its whole-file probe
    /// sequence happens to hit first - an artifact of probe geometry, not a
    /// well-defined answer - so no windowed search can match it in general.
    /// For those keys the meaningful property is asserted instead: the
    /// indexed lookup must still land inside the run (return one of the
    /// run's counts), never miss it or invent a value.
    #[test]
    fn tsv_index_matches_disk_bisection_on_random_sorted_tsvs() {
        let clean_sizes: [usize; 10] = [
            200, 1_024, 8_192, 65_536, 200_000,
            262_000, // just under one stride: single-entry index
            270_000, // just over one stride: two entries
            400_000, 700_000, 1_100_000,
        ];
        // Dirty shapes keep the file below one stride (duplicates and CRLF
        // inflate the emitted file past the row budget, so stay well under).
        let dirty_sizes: [usize; 5] = [200, 1_024, 8_192, 65_536, 150_000];
        let mut rng = TestRng(0x5f5e_11ff_00c0_ffee);
        for case in 0..200u32 {
            let dirty = case % 3 == 0;
            let target = if dirty {
                dirty_sizes[(case as usize / 3) % dirty_sizes.len()]
            } else {
                clean_sizes[(case as usize) % clean_sizes.len()]
            };
            let dup_same_count = case % 2 == 0;
            let crlf_pct = rng.below(60);
            let dup_pct = rng.below(20);
            let (blank_pct, malformed_pct) = if dirty {
                (2 + rng.below(6), 2 + rng.below(6))
            } else {
                (0, 0)
            };
            let dir = scratch_dir(&format!("prop-{case}"));
            let path = dir.join("1grams.tsv");
            let rows = write_random_sorted_tsv(
                &path,
                &mut rng,
                target,
                crlf_pct,
                dup_pct,
                blank_pct,
                malformed_pct,
                dup_same_count,
            );
            let file_len = fs::metadata(&path).unwrap().len();
            if dirty {
                // The byte-identical single-window argument needs this.
                assert!(
                    file_len < SAMPLE_STRIDE,
                    "case {case}: dirty file came out at {file_len} bytes"
                );
            }
            // term -> set of counts, to recognise the tolerated case.
            let mut counts_of: HashMap<&str, std::collections::BTreeSet<u64>> = HashMap::new();
            for (term, count) in &rows {
                counts_of.entry(term.as_str()).or_default().insert(*count);
            }
            let lookup = SortedTsvLookup::open(&path).unwrap();
            let mut keys: Vec<String> = Vec::new();
            if let Some((first, _)) = rows.first() {
                keys.push(first.clone());
            }
            if let Some((last, _)) = rows.last() {
                keys.push(last.clone());
            }
            if let Some((longest, _)) = rows.iter().max_by_key(|(t, _)| t.len()) {
                keys.push(longest.clone());
            }
            if !rows.is_empty() {
                for _ in 0..12 {
                    if let Some((term, _)) = rows.get(rng.below(rows.len() as u64) as usize) {
                        keys.push(term.clone());
                    }
                }
                // Near-miss neighbours of a sampled term.
                if let Some((term, _)) = rows.get(rng.below(rows.len() as u64) as usize) {
                    let mut before = term.clone();
                    before.pop();
                    if !before.is_empty() {
                        keys.push(before);
                    }
                    keys.push(format!("{term}a"));
                }
            }
            for _ in 0..10 {
                keys.push(random_term(&mut rng));
            }
            keys.push(String::new());
            keys.push("zzzzzzzzzz".to_string());
            for key in &keys {
                let indexed = lookup.get(key);
                let disk = lookup.get_via_disk_bisection(key);
                if indexed == disk {
                    continue;
                }
                let run_counts = counts_of.get(key.as_str());
                let tolerated = file_len >= SAMPLE_STRIDE
                    && run_counts.is_some_and(|set| set.len() > 1)
                    && indexed.is_some_and(|value| run_counts.unwrap().contains(&value))
                    && disk.is_some_and(|value| run_counts.unwrap().contains(&value));
                assert!(
                    tolerated,
                    "case {case}: target={target}B len={file_len} dirty={dirty} \
                     dup_same_count={dup_same_count} crlf={crlf_pct}% dup={dup_pct}% \
                     blank={blank_pct}% malformed={malformed_pct}% rows={} key={key:?}: \
                     indexed={indexed:?} disk={disk:?}",
                    rows.len(),
                );
            }
            cleanup_dir(&dir);
        }
    }

    #[test]
    fn tsv_index_build_is_lazy() {
        let dir = scratch_dir("lazy-index");
        let path = dir.join("1grams.tsv");
        fs::write(&path, "a\t1\nb\t2\nc\t3\n").unwrap();
        let lookup = SortedTsvLookup::open(&path).unwrap();
        assert_eq!(
            lookup.stats.disk_reads.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "opening must not read the file"
        );
        assert!(lookup.first_line.get().is_none());
        assert!(lookup.index.get().is_none());
        assert_eq!(lookup.get("b"), Some(2));
        assert!(lookup.first_line.get().is_some());
        assert!(lookup.index.get().is_some());
        assert!(
            lookup.stats.disk_reads.load(std::sync::atomic::Ordering::Relaxed) > 0,
            "the first get does the deferred reads"
        );
        cleanup_dir(&dir);
    }

    #[test]
    fn tsv_lookups_serve_from_the_block_once_the_index_is_built() {
        // A file spanning several strides, so the index has several entries
        // and each lookup narrows to one window before probing.
        let dir = scratch_dir("block-path");
        let path = dir.join("1grams.tsv");
        let mut out = String::new();
        for i in 0..45_000u32 {
            out.push_str(&format!("k{:07}\t{}\n", i, i + 1));
        }
        fs::write(&path, out).unwrap();
        let lookup = SortedTsvLookup::open(&path).unwrap();
        // The first lookup builds the index (and already serves its probes
        // from the block).
        assert_eq!(lookup.get("k0000450"), Some(451));
        let entries = lookup.index.get().unwrap();
        assert!(entries.len() >= 2, "expected a multi-entry index");
        let reads_before = lookup.stats.disk_reads.load(std::sync::atomic::Ordering::Relaxed);
        let serves_before = lookup
            .stats
            .block_line_serves
            .load(std::sync::atomic::Ordering::Relaxed);
        let keys = ["k0000000", "k0012345", "k0044999", "a0000000", "zzzzzzz", "k000"];
        for key in keys {
            let _ = lookup.get(key);
        }
        let reads_after = lookup.stats.disk_reads.load(std::sync::atomic::Ordering::Relaxed);
        let serves_after = lookup
            .stats
            .block_line_serves
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            serves_after - serves_before >= 20,
            "lookups after the build must serve probes from the block ({} serves)",
            serves_after - serves_before
        );
        // Each narrowed lookup reloads the probe block only a handful of
        // times - far below the two-reads-per-probe of a whole-file
        // bisection over the same data.
        assert!(
            reads_after - reads_before <= 10 * keys.len(),
            "a few block reads per lookup plus bounded fallbacks ({} reads for {} lookups)",
            reads_after - reads_before,
            keys.len()
        );
        cleanup_dir(&dir);
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
