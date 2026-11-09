use crate::dictionary::Dictionary;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

pub struct SuggestionEngine {
    dictionary: Arc<Dictionary>,
    words: OnceLock<WordList>,
}

impl SuggestionEngine {
    pub fn new(dictionary: Arc<Dictionary>) -> Self {
        Self {
            dictionary,
            words: OnceLock::new(),
        }
    }

    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        self.suggest_with(word, max, &mut LevenshteinScratch::default())
    }

    /// The dictionary's words in scan order, packed on first use.
    fn word_list(&self) -> &WordList {
        self.words
            .get_or_init(|| WordList::new(self.dictionary.word_list()))
    }

    /// `suggest` with the distance kernels' scratch passed in, so tests can
    /// read its kernel-visit probe after a scan.
    fn suggest_with(
        &self,
        word: &str,
        max: usize,
        scratch: &mut LevenshteinScratch,
    ) -> Vec<String> {
        let words = self.word_list();
        if max == 0 {
            return Vec::new();
        }
        let lower = word.to_lowercase();
        let is_capitalized = word
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
            && word
                .chars()
                .skip(1)
                .all(|c| !c.is_uppercase() || !c.is_alphabetic());
        let is_all_upper = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase());

        let word_len = lower.len();
        let max_dist = if word_len <= 4 { 2 } else { 3 };

        // The query's chars are collected once and reused across all
        // dictionary candidates; the scratch (the query's bit-parallel
        // equivalence table and the DP fallback's row pair) is shared between
        // calls so candidate scoring does not allocate per word.
        let lower_chars: Vec<char> = lower.chars().collect();

        // Only the best `max` candidates by (distance, scan index) are held,
        // which is exactly the head a stable sort of every candidate by
        // distance keeps, and only those are cloned. Once `max` are held, a
        // later candidate enters only if it is strictly closer than the worst
        // of them: at an equal distance its later scan index ranks it after
        // all of them. So the distance cap drops to one below the worst held
        // distance, and the kernels give up on such candidates sooner. At cap
        // 0 nothing can enter any more (distance 0 is the query itself, never
        // suggested), so the scan ends.
        //
        // The byte-length prefilter keeps `max_dist`, because a byte-length
        // gap does not bound the char distance: one substituted char can add
        // 3 bytes. A char takes at least one byte, though, so a candidate
        // with fewer bytes than the query has chars, minus the cap, is too
        // short to enter. Both checks read only the word's span, not its
        // text; the kernels' own char-length check applies the cap to the
        // rest.
        let query_chars = lower_chars.len();
        let mut top = TopK::new(max, max_dist);
        let mut cap = max_dist;
        for (index, span) in words.spans().enumerate() {
            let wlen = span.len();
            if wlen <= 1 || wlen.abs_diff(word_len) > max_dist || wlen + cap < query_chars {
                continue;
            }
            let dist = levenshtein_distance_capped(&lower_chars, words.word(span), cap, scratch);
            if dist <= cap && dist > 0 {
                top.push(dist, index);
                cap = top.admission_cap(max_dist);
                if cap == 0 {
                    break;
                }
            }
        }

        top.ranked()
            .map(|index| recase(words.get(index), is_all_upper, is_capitalized))
            .collect()
    }

    /// The ranking `suggest` replaced, kept verbatim as its parity oracle:
    /// clone every candidate within `max_dist`, stable-sort them all by
    /// distance, keep the first `max`. It reads the packed word list, which
    /// yields the words of the per-word allocations it replaced in the same
    /// order (`word_list_packs_the_sorted_words_back_to_back`).
    #[cfg(test)]
    fn suggest_by_full_sort(&self, word: &str, max: usize) -> Vec<String> {
        let words = self.word_list();
        let lower = word.to_lowercase();
        let is_capitalized = word
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
            && word
                .chars()
                .skip(1)
                .all(|c| !c.is_uppercase() || !c.is_alphabetic());
        let is_all_upper = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase());

        let word_len = lower.len();
        let max_dist = if word_len <= 4 { 2 } else { 3 };

        // The query's chars are collected once and reused across all
        // dictionary candidates, with one scratch (the query's bit-parallel
        // equivalence table and the DP fallback's row pair) shared between
        // calls so candidate scoring does not allocate per word.
        let lower_chars: Vec<char> = lower.chars().collect();
        let mut scratch = LevenshteinScratch::default();

        let mut scored: Vec<(String, usize)> = words
            .iter()
            .filter(|w| {
                let wlen = w.len();
                wlen > 1 && wlen.abs_diff(word_len) <= max_dist
            })
            .filter_map(|w| {
                let dist = levenshtein_distance_capped(&lower_chars, w, max_dist, &mut scratch);
                if dist <= max_dist && dist > 0 {
                    Some((w.to_owned(), dist))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by_key(|(_, dist)| *dist);

        scored
            .into_iter()
            .take(max)
            .map(|(w, _)| {
                if is_all_upper {
                    w.to_uppercase()
                } else if is_capitalized {
                    let mut chars = w.chars();
                    chars
                        .next()
                        .unwrap_or_default()
                        .to_uppercase()
                        .collect::<String>()
                        + chars.as_str()
                } else {
                    w
                }
            })
            .collect()
    }
}

/// The dictionary's words in scan order (sorted byte-wise, duplicates kept),
/// packed back to back into one buffer.
///
/// `Dictionary::word_list` yields its inline words in per-process `HashSet`
/// order, and ranking keeps the scan order among equal-distance words.
/// Sorting the words once, byte-wise, makes suggestions a pure function of the
/// word set: ascending distance, ties in byte order, identical in every run.
/// Equal entries are identical strings, so an unstable sort is enough.
///
/// The words arrive as one heap allocation each, laid out in that `HashSet`
/// order, and sorting moves only their handles. A scan in sorted order would
/// then reach an unrelated address for nearly every word, and those cache and
/// TLB misses cost more than scoring the candidate. Packed, the scan streams
/// through memory. The words and their order are unchanged, so every
/// suggestion is too.
struct WordList {
    /// Every word in scan order, with nothing in between.
    text: String,
    /// `ends[i]` is where word `i` ends in `text`; it starts where word
    /// `i - 1` ends, or at 0.
    ends: Vec<usize>,
}

impl WordList {
    /// Sorts and packs `words`. Their allocations are freed on return, so
    /// only the packed copy stays alive.
    fn new(mut words: Vec<String>) -> Self {
        words.sort_unstable();
        let mut text = String::with_capacity(words.iter().map(String::len).sum());
        let ends = words
            .iter()
            .map(|word| {
                text.push_str(word);
                text.len()
            })
            .collect();
        Self { text, ends }
    }

    /// The byte range of each word in `text`, in scan order, so a scan can
    /// filter on word lengths without reading the words.
    fn spans(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        let mut start = 0;
        self.ends.iter().map(move |&end| {
            let span = start..end;
            start = end;
            span
        })
    }

    /// The word at `span`, one of the ranges `spans` yields.
    fn word(&self, span: Range<usize>) -> &str {
        &self.text[span]
    }

    /// Word `index` in scan order.
    fn get(&self, index: usize) -> &str {
        let start = index
            .checked_sub(1)
            .map_or(0, |previous| self.ends[previous]);
        &self.text[start..self.ends[index]]
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.ends.len()
    }

    /// The words in scan order.
    #[cfg(test)]
    fn iter(&self) -> impl Iterator<Item = &str> + '_ {
        self.spans().map(|span| self.word(span))
    }
}

/// A bounded top-k buffer over a dictionary scan: the best `max` candidates
/// by (distance, scan index), exactly the head that a stable sort of every
/// candidate by distance followed by `take(max)` keeps.
///
/// Distances are small (`1..=max_dist`), so held candidates are bucketed by
/// distance, each bucket in scan order: holding a candidate is a push,
/// evicting the worst one is a pop from the highest non-empty bucket, and the
/// ranking is the buckets in order. No step's cost grows with `max`.
struct TopK {
    /// `by_distance[d]`: the scan indices of the held candidates at distance
    /// `d`, ascending because candidates are offered in scan order.
    by_distance: Vec<Vec<usize>>,
    /// Slots left before `max` candidates are held.
    room: usize,
}

impl TopK {
    fn new(max: usize, max_dist: usize) -> Self {
        Self {
            by_distance: vec![Vec::new(); max_dist + 1],
            room: max,
        }
    }

    /// Holds the candidate at `index`, which must be greater than every index
    /// offered before. On a full buffer the worst entry is evicted, which is
    /// this candidate itself unless it is strictly closer than the worst held
    /// one.
    fn push(&mut self, distance: usize, index: usize) {
        self.by_distance[distance].push(index);
        if self.room > 0 {
            self.room -= 1;
        } else if let Some(worst) = self.by_distance.iter_mut().rev().find(|b| !b.is_empty()) {
            worst.pop();
        }
    }

    /// The largest distance a later candidate can have and still enter:
    /// `max_dist` while there is room; once full, one below the worst held
    /// distance, because a later candidate at that distance ranks after every
    /// held one. 0 means nothing at distance 1 or more can enter.
    fn admission_cap(&self, max_dist: usize) -> usize {
        if self.room > 0 {
            return max_dist;
        }
        self.by_distance
            .iter()
            .rposition(|bucket| !bucket.is_empty())
            .map_or(0, |worst| worst.saturating_sub(1))
    }

    /// The held scan indices, best first.
    fn ranked(&self) -> impl Iterator<Item = usize> + '_ {
        self.by_distance.iter().flatten().copied()
    }
}

/// A dictionary word in the query's casing: all upper for an all-upper
/// query, first letter upper for a capitalized one, unchanged otherwise.
fn recase(w: &str, is_all_upper: bool, is_capitalized: bool) -> String {
    if is_all_upper {
        w.to_uppercase()
    } else if is_capitalized {
        let mut chars = w.chars();
        chars
            .next()
            .unwrap_or_default()
            .to_uppercase()
            .collect::<String>()
            + chars.as_str()
    } else {
        w.to_owned()
    }
}

/// Exact Levenshtein distance over chars. Kept as the correctness oracle;
/// the suggest path uses the capped variant below.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    levenshtein_impl(&a_chars, b, None, &mut (Vec::new(), Vec::new()))
}

/// Levenshtein distance with an early cutoff: returns `max + 1` as soon as
/// the distance provably exceeds `max`, so far candidates are rejected after
/// a few columns (rows, on the DP fallback) instead of a full matrix.
/// Distances <= max are exact, matching the uncapped computation, so the
/// accepted set and stored distances are unchanged.
///
/// Queries that fit one `u64` block (at most 64 chars, all Latin-1) go to
/// the Myers/Hyyro bit-parallel kernel below; everything else — longer
/// queries, or queries with characters beyond Latin-1, which have no slot in
/// the kernel's 256-entry equivalence table — falls back to the two-row DP,
/// which stays in-tree unchanged as the exactness oracle for both paths.
fn levenshtein_distance_capped(
    a: &[char],
    b: &str,
    max: usize,
    scratch: &mut LevenshteinScratch,
) -> usize {
    if fits_single_block(a) {
        let b_len = b.chars().count();
        if b_len == 0 {
            return a.len();
        }
        if a.len().abs_diff(b_len) > max {
            return max + 1;
        }
        ensure_peq(a, scratch);
        scratch.myers_calls += 1;
        levenshtein_myers_capped(a, b, b_len, max, &scratch.peq)
    } else {
        levenshtein_impl(a, b, Some(max), &mut scratch.rows)
    }
}

/// Whether the query can be scored by the bit-parallel kernel: non-empty,
/// at most one `u64` block of chars, and Latin-1 only, because the
/// equivalence table indexes characters directly.
fn fits_single_block(a: &[char]) -> bool {
    !a.is_empty() && a.len() <= 64 && a.iter().all(|&c| (c as u32) < 256)
}

/// Rebuilds the query's equivalence table when the query changed. The table
/// is the only per-query state: across a dictionary scan the query is fixed,
/// so the rebuild happens once per `suggest` call, not once per candidate.
fn ensure_peq(a: &[char], scratch: &mut LevenshteinScratch) {
    if scratch.peq_query != a {
        scratch.peq = [0; 256];
        for (i, &c) in a.iter().enumerate() {
            scratch.peq[c as u32 as usize] |= 1 << i;
        }
        scratch.peq_query.clear();
        scratch.peq_query.extend_from_slice(a);
    }
}

/// Myers/Hyyro bit-parallel Levenshtein distance with an early cutoff.
///
/// The query `a` maps onto the bits of one machine word: bit `i` of
/// `peq[c]` is set iff `a[i] == c`. Each text character then advances a
/// whole DP column with a handful of single-word operations on the
/// horizontal/vertical delta vectors (`Ph`/`Mh`, `Pv`/`Mv`) instead of one
/// cell at a time, so an m x n matrix costs n word steps rather than m x n
/// cell steps. This is Hyyro's formulation of Myers' algorithm: the
/// horizontal deltas come from the carry chain of `((Eq & Pv) + Pv) ^ Pv`,
/// which stays exact under `wrapping_add` because carries only travel
/// upward and every state vector is masked to the m valid bits (the carry
/// out of bit 63 for a 64-char query lands outside the word and is never
/// read again). Characters of `b` beyond Latin-1 have a zero column of `Eq`,
/// which is exact: they match nothing in a Latin-1 query.
///
/// The cutoff mirrors the DP's row-minimum exit through two sound lower
/// bounds on the final distance, evaluated after every column:
/// * the smallest cell of the current column, `min(columns_done, score -
///   popcount(Pv))`: each cell below row 0 is the bottom cell minus a
///   suffix sum of the column's vertical deltas, and that sum is at most
///   `popcount(Pv)`, the number of +1 deltas (not `Mv`: subtracting the -1
///   count would overestimate the minimum and cut exact matches); row 0 is
///   the empty-prefix cell `D[0][t] = columns_done`;
/// * `score - remaining_columns`, since the bottom cell drops by at most one
///   per remaining column.
/// When either bound exceeds `max` the kernel returns `max + 1`; otherwise
/// it returns the exact distance — also when that distance exceeds `max`,
/// like the DP, which returns the true bottom cell whenever no row minimum
/// cut fires. `score` never goes negative: it is `D[m][t] >= |m - t|`.
fn levenshtein_myers_capped(
    a: &[char],
    b: &str,
    b_len: usize,
    max: usize,
    peq: &[u64; 256],
) -> usize {
    let m = a.len();
    debug_assert!((1..=64).contains(&m));
    let mask = if m == 64 { u64::MAX } else { (1u64 << m) - 1 };
    let top = 1u64 << (m - 1);
    let bound_to_beat = max as i64;
    let mut score = m as i64;
    let mut pv = mask;
    let mut mv = 0u64;
    for (index, ch) in b.chars().enumerate() {
        let code = ch as u32 as usize;
        let eq = if code < 256 { peq[code] } else { 0 };
        let xv = eq | mv;
        let xh = (((eq & pv).wrapping_add(pv)) ^ pv) | eq;
        let mut ph = mv | !(xh | pv);
        let mh = pv & xh;
        if ph & top != 0 {
            score += 1;
        } else if mh & top != 0 {
            score -= 1;
        }
        ph = ((ph << 1) | 1) & mask;
        let mh = (mh << 1) & mask;
        // `!` and the shifts are u64-wide; masking keeps the delta vectors
        // confined to the m pattern bits the recurrence is defined over.
        pv = mh | (!(xv | ph) & mask);
        mv = ph & xv;

        let columns_done = (index + 1) as i64;
        let remaining = (b_len - index - 1) as i64;
        let column_min = (score - pv.count_ones() as i64).min(columns_done);
        if column_min > bound_to_beat || score - remaining > bound_to_beat {
            return max + 1;
        }
    }
    score as usize
}

/// Reusable state for the distance kernels, shared across a dictionary scan
/// so candidate scoring allocates only when the query changes: the DP rows
/// for queries outside the bit-parallel kernel's reach, and the kernel's
/// equivalence table for the current query. `myers_calls` counts dispatcher
/// visits to the kernel — a probe the tests assert the dispatch on.
struct LevenshteinScratch {
    rows: (Vec<usize>, Vec<usize>),
    peq: [u64; 256],
    peq_query: Vec<char>,
    myers_calls: usize,
}

impl Default for LevenshteinScratch {
    fn default() -> Self {
        Self {
            rows: (Vec::new(), Vec::new()),
            peq: [0; 256],
            peq_query: Vec::new(),
            myers_calls: 0,
        }
    }
}

/// Two-row DP over chars (never bytes: a byte-level distance would change
/// results for non-ASCII words). `scratch` carries the two rows so repeated
/// calls over a dictionary do not allocate.
fn levenshtein_impl(
    a: &[char],
    b: &str,
    max: Option<usize>,
    (prev, cur): &mut (Vec<usize>, Vec<usize>),
) -> usize {
    let a_len = a.len();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }
    if let Some(max) = max
        && a_len.abs_diff(b_len) > max
    {
        return max + 1;
    }

    prev.clear();
    prev.extend(0..=b_len);
    cur.clear();
    cur.resize(b_len + 1, 0);

    for i in 1..=a_len {
        cur[0] = i;
        let a_char = a[i - 1];
        let mut row_min = i;
        for (j, b_char) in b.chars().enumerate() {
            let cost = usize::from(a_char != b_char);
            let value = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
            cur[j + 1] = value;
            if value < row_min {
                row_min = value;
            }
        }
        if let Some(max) = max
            && row_min > max
        {
            return max + 1;
        }
        std::mem::swap(prev, cur);
    }
    prev[b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("hello", "helo"), 1);
        assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        assert_eq!(levenshtein_distance("hello", "world"), 4);
    }

    #[test]
    fn capped_levenshtein_matches_the_exact_oracle() {
        let cases = [
            ("hello", "hello"),
            ("hello", "helo"),
            ("hello", "hallo"),
            ("hello", "world"),
            ("café", "cafe"),
            ("grübchen", "grubchen"),
            ("naïve", "naive"),
            ("a", "b"),
            ("", "abc"),
            ("abc", ""),
        ];
        for (a, b) in cases {
            let a_chars: Vec<char> = a.chars().collect();
            for max in [1usize, 2, 3, 5] {
                let mut scratch = LevenshteinScratch::default();
                let capped = levenshtein_distance_capped(&a_chars, b, max, &mut scratch);
                let exact = levenshtein_distance(a, b);
                if exact <= max {
                    assert_eq!(capped, exact, "({a:?}, {b:?}, max={max})");
                } else {
                    assert!(capped > max, "({a:?}, {b:?}, max={max}): {capped}");
                }
            }
        }
    }

    /// Numerical Recipes' 64-bit LCG with a fixed seed: the corpus below is
    /// identical on every run and machine.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // The high bits of an LCG are the well-mixed ones.
            ((self.0 >> 33) % n as u64) as usize
        }

        fn string(&mut self, alphabet: &[char]) -> String {
            let len = 1 + self.below(70);
            (0..len)
                .map(|_| alphabet[self.below(alphabet.len())])
                .collect()
        }

        /// A near neighbour of `a`: `edits` random substitutions, insertions
        /// and deletions, so the exact distance stays small and the corpus
        /// covers the accepted-candidate regime (distance <= max), which
        /// independent random strings almost never do.
        fn neighbour(&mut self, a: &str, alphabet: &[char]) -> String {
            let mut chars: Vec<char> = a.chars().collect();
            for _ in 0..self.below(7) {
                if chars.is_empty() {
                    chars.push(alphabet[self.below(alphabet.len())]);
                    continue;
                }
                let at = self.below(chars.len());
                match self.below(3) {
                    0 => chars[at] = alphabet[self.below(alphabet.len())],
                    1 => chars.insert(at, alphabet[self.below(alphabet.len())]),
                    _ => {
                        chars.remove(at);
                    }
                }
            }
            chars.into_iter().collect()
        }

        /// A word of 1..=`max_len` chars over `alphabet`.
        fn word(&mut self, alphabet: &[char], max_len: usize) -> String {
            let len = 1 + self.below(max_len);
            (0..len)
                .map(|_| alphabet[self.below(alphabet.len())])
                .collect()
        }

        /// `a` after exactly `edits` random substitutions, insertions and
        /// deletions.
        fn edited(&mut self, a: &str, alphabet: &[char], edits: usize) -> String {
            let mut chars: Vec<char> = a.chars().collect();
            for _ in 0..edits {
                let letter = alphabet[self.below(alphabet.len())];
                if chars.is_empty() {
                    chars.push(letter);
                    continue;
                }
                let at = self.below(chars.len());
                match self.below(3) {
                    0 => chars[at] = letter,
                    1 => chars.insert(at, letter),
                    _ => {
                        chars.remove(at);
                    }
                }
            }
            chars.into_iter().collect()
        }
    }

    #[test]
    fn bit_parallel_matches_capped_dp_and_exact_oracle() {
        let alphabets: [&[char]; 3] = [
            // ASCII only.
            &[
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
            ],
            // Latin-1 beyond ASCII: queries stay on the bit-parallel kernel.
            &[
                'a', 'e', 'i', 'o', 'u', 'é', 'è', 'ü', 'ö', 'ä', 'ñ', 'ç', 'ß', 'ï', 'û', 'å',
            ],
            // Beyond Latin-1: such a char in the query forces the DP
            // fallback; in the candidate only, it is a zero column of `Eq`.
            &['a', 'z', 'é', 'x', 'α', 'β', 'γ', 'Ω', '日', '本'],
        ];
        let mut rng = Lcg(0x5EED_0F_0A11_C0DE);
        // One scratch for the whole corpus, as in `suggest`, so the
        // per-query table rebuild is exercised too.
        let mut scratch = LevenshteinScratch::default();
        let (mut kernel_checks, mut kernel_exact, mut fallback_checks) = (0usize, 0usize, 0usize);
        for alphabet in alphabets {
            for pair in 0..600 {
                let a = rng.string(alphabet);
                // Alternate far pairs (the cutoff regime) with near pairs
                // (the exact regime: what `suggest` accepts).
                let b = if pair % 2 == 0 {
                    rng.string(alphabet)
                } else {
                    rng.neighbour(&a, alphabet)
                };
                let a_chars: Vec<char> = a.chars().collect();
                let b_len = b.chars().count();
                let exact = levenshtein_distance(&a, &b);
                for max in [0usize, 1, 2, 3, 5, 8] {
                    let case = format!("({a:?}, {b:?}, max={max}, exact={exact})");
                    let capped = levenshtein_distance_capped(&a_chars, &b, max, &mut scratch);
                    // A capped result is either exact or the cutoff sentinel.
                    assert!(capped == exact || capped == max + 1, "{case}: capped={capped}");
                    if exact <= max {
                        assert_eq!(capped, exact, "{case}");
                    } else {
                        assert!(capped > max, "{case}: capped={capped}");
                    }
                    if !fits_single_block(&a_chars) {
                        fallback_checks += 1;
                        continue;
                    }
                    // The kernel itself, called directly (the dispatcher's
                    // length pre-check first, as in production).
                    let bit_parallel = if a_chars.len().abs_diff(b_len) > max {
                        max + 1
                    } else {
                        ensure_peq(&a_chars, &mut scratch);
                        levenshtein_myers_capped(&a_chars, &b, b_len, max, &scratch.peq)
                    };
                    assert!(
                        bit_parallel == exact || bit_parallel == max + 1,
                        "{case}: bit_parallel={bit_parallel}"
                    );
                    if exact <= max {
                        assert_eq!(bit_parallel, exact, "{case}");
                        kernel_exact += 1;
                    } else {
                        assert!(bit_parallel > max, "{case}: bit_parallel={bit_parallel}");
                    }
                    kernel_checks += 1;
                }
            }
        }
        // The corpus reaches both paths — lengths 65..=70 and chars beyond
        // Latin-1 in the query take the DP, the rest the kernel — and the
        // kernel is checked in the exact regime, not only on cutoffs.
        assert!(kernel_checks > 3000, "kernel checks: {kernel_checks}");
        assert!(kernel_exact > 1000, "kernel exact-regime checks: {kernel_exact}");
        assert!(fallback_checks > 300, "fallback checks: {fallback_checks}");
        assert!(scratch.myers_calls > 0);
    }

    #[test]
    fn bit_parallel_is_exact_at_the_block_boundary() {
        // 64 chars use every bit of the word (mask == u64::MAX, and the
        // `wrapping_add` carry leaves the word); 65 fall back to the DP.
        let mut rng = Lcg(64);
        let alphabet = ['a', 'b', 'c', 'ü'];
        for len in [63usize, 64, 65] {
            for _ in 0..50 {
                let a: String = (0..len).map(|_| alphabet[rng.below(4)]).collect();
                let mut b: Vec<char> = a.chars().collect();
                for _ in 0..rng.below(4) {
                    let at = rng.below(b.len());
                    b[at] = alphabet[rng.below(4)];
                }
                let b: String = b.into_iter().collect();
                let a_chars: Vec<char> = a.chars().collect();
                let exact = levenshtein_distance(&a, &b);
                let mut scratch = LevenshteinScratch::default();
                for max in [0usize, 1, 2, 3, 64, 70] {
                    let capped = levenshtein_distance_capped(&a_chars, &b, max, &mut scratch);
                    if exact <= max {
                        assert_eq!(capped, exact, "len={len} max={max}");
                    } else {
                        assert!(capped > max, "len={len} max={max}: {capped}");
                    }
                }
                assert_eq!(scratch.myers_calls > 0, len <= 64, "len={len}");
            }
        }
    }

    #[test]
    fn capped_dispatches_block_sized_latin1_queries_to_the_kernel() {
        let mut scratch = LevenshteinScratch::default();
        let query: Vec<char> = "café".chars().collect();
        assert_eq!(levenshtein_distance_capped(&query, "cafe", 2, &mut scratch), 1);
        assert_eq!(scratch.myers_calls, 1);
        // Same query again: the table is reused, the kernel still runs.
        assert_eq!(levenshtein_distance_capped(&query, "café", 2, &mut scratch), 0);
        assert_eq!(scratch.myers_calls, 2);

        // Longer than one block: the DP.
        let long: Vec<char> = "a".repeat(65).chars().collect();
        let long_b = format!("{}b", "a".repeat(64));
        assert_eq!(levenshtein_distance_capped(&long, &long_b, 2, &mut scratch), 1);
        // A query char beyond Latin-1: the DP.
        let exotic: Vec<char> = "café日".chars().collect();
        assert_eq!(levenshtein_distance_capped(&exotic, "cafe日", 2, &mut scratch), 1);
        // An empty query: the DP's empty-input path.
        assert_eq!(levenshtein_distance_capped(&[], "abc", 2, &mut scratch), 3);
        assert_eq!(scratch.myers_calls, 2);

        // The kernel's own early exits, through the dispatcher.
        let hello: Vec<char> = "hello".chars().collect();
        assert_eq!(levenshtein_distance_capped(&hello, "", 2, &mut scratch), 5);
        // Distance 4 with cap 2: rejected, either by the cutoff sentinel or
        // by an exact completed score.
        assert!(levenshtein_distance_capped(&hello, "hellooooo", 2, &mut scratch) > 2);
        assert!(levenshtein_distance_capped(&hello, "world", 2, &mut scratch) > 2);
        // Cap 4 admits the distance: it must come back exact.
        assert_eq!(levenshtein_distance_capped(&hello, "world", 4, &mut scratch), 4);
    }

    #[test]
    fn test_suggest() {
        let dict = Dictionary::from_words(&["hello", "world", "help", "held", "hell", "helm"]);
        let engine = SuggestionEngine::new(Arc::new(dict));
        let suggestions = engine.suggest("helo", 5);
        assert!(
            suggestions.contains(&"hello".to_string()) || suggestions.contains(&"help".to_string()),
            "Expected at least one close match, got: {:?}",
            suggestions
        );
        assert!(
            suggestions
                .iter()
                .take(3)
                .all(|s| levenshtein_distance("helo", s) <= 1)
        );
    }

    fn engine(words: &[&str]) -> SuggestionEngine {
        SuggestionEngine::new(Arc::new(Dictionary::from_words(words)))
    }

    #[test]
    fn all_upper_query_uppercases_candidates() {
        let engine = engine(&["hello", "help", "world"]);
        let suggestions = engine.suggest("HELO", 5);
        assert!(suggestions.contains(&"HELLO".to_string()));
        assert!(suggestions.iter().all(|s| s == &s.to_uppercase()));
    }

    #[test]
    fn capitalized_query_capitalizes_candidates() {
        let engine = engine(&["hello", "help", "world"]);
        let suggestions = engine.suggest("Helo", 5);
        assert!(suggestions.contains(&"Hello".to_string()));
        assert!(!suggestions.contains(&"hello".to_string()));
    }

    #[test]
    fn short_queries_use_distance_cap_two() {
        let engine = engine(&["hello", "held", "hxyz"]);
        let suggestions = engine.suggest("helo", 5);
        assert!(suggestions.contains(&"hello".to_string()));
        assert!(suggestions.contains(&"held".to_string()));
        assert!(!suggestions.contains(&"hxyz".to_string()));
    }

    #[test]
    fn long_queries_use_distance_cap_three() {
        let engine = engine(&["greeting", "greetinx", "greetinxyza"]);
        let suggestions = engine.suggest("greeting", 5);
        assert!(suggestions.contains(&"greetinx".to_string()));
        assert!(!suggestions.contains(&"greetinxyza".to_string()));
    }

    #[test]
    fn max_truncates_and_single_letter_words_are_skipped() {
        let engine = engine(&["hello", "hell", "helm", "x"]);
        assert!(engine.suggest("helo", 1).len() <= 1);
        assert!(!engine.suggest("helo", 5).contains(&"x".to_string()));
    }

    #[test]
    fn equal_distance_ties_come_out_in_byte_order() {
        // Every candidate is one edit from "helo"; before the candidate list
        // was sorted, their order followed the dictionary's per-process
        // HashSet order and changed from run to run.
        let expected = ["held", "hell", "hello", "helm", "help"];
        let forward = engine(&["help", "hello", "helm", "held", "hell"]);
        let backward = engine(&["hell", "held", "helm", "hello", "help"]);
        assert_eq!(forward.suggest("helo", 5), expected);
        assert_eq!(backward.suggest("helo", 5), expected);
        // Truncation keeps the head of that stable order.
        assert_eq!(forward.suggest("helo", 2), ["held", "hell"]);
        // Distance still ranks first: "hello" (1 edit) beats "heal" (2 edits)
        // even though "heal" sorts first byte-wise.
        let ranked = engine(&["heal", "hello"]);
        assert_eq!(ranked.suggest("helo", 5), ["hello", "heal"]);
    }

    #[test]
    fn empty_query_and_no_match_yield_nothing() {
        let engine = engine(&["hello", "world"]);
        assert!(engine.suggest("", 5).is_empty());
        assert!(engine.suggest("zzzzzz", 5).is_empty());
    }

    #[test]
    fn levenshtein_handles_empty_inputs() {
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", ""), 0);
    }

    /// An engine that scans exactly `words`, sorted, duplicates kept: the
    /// word list a dictionary yields when its inline words overlap its
    /// backing word list.
    fn engine_over(words: Vec<String>) -> SuggestionEngine {
        let engine = SuggestionEngine::new(Arc::new(Dictionary::new()));
        assert!(
            engine.words.set(WordList::new(words)).is_ok(),
            "a fresh engine has no word list yet"
        );
        engine
    }

    /// `word` in one of four query casings: all upper, capitalized, or as is
    /// (twice as likely, since it may be lower or mixed case).
    fn cased(word: String, style: usize) -> String {
        match style {
            0 => word.to_uppercase(),
            1 => {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => word,
                }
            }
            _ => word,
        }
    }

    #[test]
    fn top_k_keeps_the_head_of_a_stable_sort_by_distance() {
        let mut rng = Lcg(0x709C_0B0F);
        for max_dist in [2usize, 3] {
            for _ in 0..400 {
                let len = rng.below(40);
                let distances: Vec<usize> = (0..len).map(|_| 1 + rng.below(max_dist)).collect();
                for max in [0usize, 1, 2, 3, 5, 8, 64] {
                    let case = format!("max={max}, max_dist={max_dist}, distances={distances:?}");
                    // What the full sort kept: every candidate, stable-sorted
                    // by distance, then the first `max`.
                    let mut expected: Vec<usize> = (0..len).collect();
                    expected.sort_by_key(|&index| distances[index]);
                    expected.truncate(max);

                    // Every candidate offered: the buffer alone is exact.
                    let mut every = TopK::new(max, max_dist);
                    for (index, &distance) in distances.iter().enumerate() {
                        every.push(distance, index);
                    }
                    assert_eq!(every.ranked().collect::<Vec<_>>(), expected, "{case}");

                    // As the scan offers them: only within the admission cap,
                    // and nothing after the cap reaches 0. The cap never turns
                    // away a candidate that the full sort keeps.
                    let mut scanned = TopK::new(max, max_dist);
                    let mut cap = max_dist;
                    for (index, &distance) in distances.iter().enumerate() {
                        if distance <= cap {
                            scanned.push(distance, index);
                            cap = scanned.admission_cap(max_dist);
                            if cap == 0 {
                                break;
                            }
                        }
                    }
                    assert_eq!(scanned.ranked().collect::<Vec<_>>(), expected, "{case}");
                }
            }
        }
    }

    #[test]
    fn suggest_matches_the_full_sort_oracle_on_random_dictionaries() {
        let alphabets: [&[char]; 3] = [
            // ASCII, with capitals that sort before every lowercase word.
            &['a', 'b', 'c', 'd', 'e', 'A', 'B'],
            // Latin-1 beyond ASCII: the bit-parallel kernel; 'ß' uppercases
            // to two chars.
            &['a', 'e', 'n', 'r', 'é', 'ü', 'ß', 'É', 'Ü'],
            // Beyond Latin-1: in the query, the DP fallback.
            &['a', 'z', 'x', 'é', 'α', 'β', 'Ω', '日'],
        ];
        let mut rng = Lcg(0x70B_C0DE);
        // How often the buffer turned candidates away (more candidates than
        // `max`), how often the cap reached 0 and ended the scan (at least
        // `max` candidates at distance 1), and how often fewer than `max`
        // candidates existed at all.
        let (mut trimmed, mut capped_out, mut short) = (0usize, 0usize, 0usize);
        for alphabet in alphabets {
            for dictionary in 0..10 {
                let size = 50 + rng.below(250);
                let mut words: Vec<String> = (0..size).map(|_| rng.word(alphabet, 7)).collect();
                let engine = if dictionary % 2 == 0 {
                    let words: Vec<&str> = words.iter().map(String::as_str).collect();
                    engine(&words)
                } else {
                    for _ in 0..size / 5 {
                        let duplicate = words[rng.below(size)].clone();
                        words.push(duplicate);
                    }
                    engine_over(words.clone())
                };
                let scanned: Vec<&str> = engine.word_list().iter().collect();
                for _ in 0..30 {
                    let base = if rng.below(4) == 0 {
                        rng.word(alphabet, 9)
                    } else {
                        words[rng.below(words.len())].clone()
                    };
                    let edits = rng.below(4);
                    let query = cased(rng.edited(&base, alphabet, edits), rng.below(4));

                    let lower = query.to_lowercase();
                    let max_dist = if lower.len() <= 4 { 2 } else { 3 };
                    let candidates: Vec<usize> = scanned
                        .iter()
                        .filter(|w| w.len() > 1 && w.len().abs_diff(lower.len()) <= max_dist)
                        .map(|w| levenshtein_distance(&lower, w))
                        .filter(|&distance| distance > 0 && distance <= max_dist)
                        .collect();
                    let at_one = candidates.iter().filter(|&&d| d == 1).count();

                    for max in [0usize, 1, 2, 3, 5, 10, 1000] {
                        assert_eq!(
                            engine.suggest(&query, max),
                            engine.suggest_by_full_sort(&query, max),
                            "query {query:?}, max {max}, over {} words",
                            scanned.len()
                        );
                        if max > 0 && candidates.len() > max {
                            trimmed += 1;
                        }
                        if max > 0 && at_one >= max {
                            capped_out += 1;
                        }
                        if candidates.len() < max && max < 1000 {
                            short += 1;
                        }
                    }
                }
            }
        }
        assert!(trimmed > 2400, "trimmed: {trimmed}");
        assert!(capped_out > 1000, "capped out: {capped_out}");
        assert!(short > 1000, "short: {short}");
    }

    /// The en-US speller's word list, read the way `og-langs` builds it
    /// (`variant_dictionary`), or `None` without the LanguageTool resources.
    fn english_dictionary() -> Option<Dictionary> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/hunspell",
        );
        if !dir.join("en_US.words.txt").exists() {
            return None;
        }
        let mut dict = Dictionary::new();
        for source in ["en_US.words.txt", "spelling.txt", "spelling_en-US.txt"] {
            if let Ok(words) = std::fs::read_to_string(dir.join(source)) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }
        }
        Some(dict)
    }

    #[test]
    fn suggest_matches_the_full_sort_oracle_on_the_english_dictionary() {
        let Some(dict) = english_dictionary() else {
            eprintln!("Skipping: en hunspell resources not present");
            return;
        };
        let engine = SuggestionEngine::new(Arc::new(dict));
        // The packed list holds exactly the words, in exactly the order, of
        // the sorted per-word allocations it replaced.
        let mut unpacked = engine.dictionary.word_list();
        unpacked.sort_unstable();
        let packed: Vec<&str> = engine.word_list().iter().collect();
        assert!(
            packed == unpacked,
            "packed words differ from the sorted word list"
        );
        drop((packed, unpacked));
        // The og-bench corpora's misspellings, then the audit's.
        let typos = [
            "probleme",
            "sentense",
            "Teh",
            "guick",
            "jumpd",
            "dogg",
            "wonderfull",
            "colected",
            "cannt",
            "spel",
            "corectly",
            "dosent",
            "wich",
            "dont",
        ];
        for typo in typos {
            for max in [1, 5, 10, 20] {
                assert_eq!(
                    engine.suggest(typo, max),
                    engine.suggest_by_full_sort(typo, max),
                    "{typo:?} max={max}"
                );
            }
        }
        // 200 more: dictionary words one or two edits away, in all casings.
        let words = engine.word_list();
        let letters: Vec<char> = ('a'..='z').chain(['é', 'ö', '\'']).collect();
        let mut rng = Lcg(0xE4_5115);
        for _ in 0..200 {
            let base = words.get(rng.below(words.len()));
            let edits = 1 + rng.below(2);
            let typo = cased(rng.edited(base, &letters, edits), rng.below(4));
            assert_eq!(
                engine.suggest(&typo, 5),
                engine.suggest_by_full_sort(&typo, 5),
                "{typo:?} (from {base:?})"
            );
        }
    }

    #[test]
    fn word_list_packs_the_sorted_words_back_to_back() {
        // Unsorted, with a duplicate, an empty word, capitals, and multi-byte
        // chars of 2 and 3 bytes.
        let words = ["délai", "b", "a", "", "b", "Zoo", "日本", "ab"];
        let mut sorted: Vec<String> = words.map(String::from).to_vec();
        sorted.sort_unstable();
        let list = WordList::new(words.map(String::from).to_vec());

        // The scan sees the words the per-word allocations held, in the same
        // order, duplicates and the empty word included.
        assert_eq!(list.len(), sorted.len());
        assert_eq!(list.iter().collect::<Vec<_>>(), sorted);
        for (index, word) in sorted.iter().enumerate() {
            assert_eq!(list.get(index), word, "word {index}");
        }
        // One buffer, no gaps: each word starts where the previous one ends,
        // so a scan reads `text` front to back.
        assert_eq!(list.text, sorted.concat());
        let spans: Vec<Range<usize>> = list.spans().collect();
        assert_eq!(spans.first().map(|span| span.start), Some(0));
        assert_eq!(spans.last().map(|span| span.end), Some(list.text.len()));
        for pair in spans.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }

        let empty = WordList::new(Vec::new());
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.iter().count(), 0);
        assert!(empty.text.is_empty());
    }

    #[test]
    fn full_buffer_tightens_the_cap_and_ends_the_scan_at_zero() {
        // Byte order: haalo, hbllp, hellooo, helo, hxllo, yello. Against
        // "hello" (cap 3) the distances are 2, 2, 2, 1, 1, 1.
        let tightening = engine(&["yello", "hxllo", "helo", "hellooo", "hbllp", "haalo"]);
        let mut scratch = LevenshteinScratch::default();
        assert_eq!(
            tightening.suggest_with("hello", 2, &mut scratch),
            tightening.suggest_by_full_sort("hello", 2)
        );
        assert_eq!(tightening.suggest("hello", 2), ["helo", "hxllo"]);
        // haalo and hbllp fill the buffer at distance 2: the cap drops to 1,
        // so hellooo (2 chars longer) is turned away before the kernel. helo
        // evicts hbllp, hxllo evicts haalo: both held are at distance 1, the
        // cap reaches 0 and yello is never scored. The full sort scores all
        // six.
        assert_eq!(scratch.myers_calls, 4);

        // Three distance-1 words fill a buffer of three: the scan ends there.
        let ending = engine(&["helot", "help", "helm", "hello", "hell", "held"]);
        let mut scratch = LevenshteinScratch::default();
        assert_eq!(
            ending.suggest_with("helo", 3, &mut scratch),
            ["held", "hell", "hello"]
        );
        assert_eq!(scratch.myers_calls, 3);
    }

    #[test]
    fn short_candidates_are_bounded_by_query_chars_not_bytes() {
        // "éé" is 2 chars in 4 bytes (cap 2). ab and ac fill a buffer of two
        // at distance 2, so the cap drops to 1. "é" (2 bytes, 1 char) is one
        // deletion away: 2 bytes + cap 1 is not below the query's 2 chars, so
        // it must still be scored (its bytes against the query's 4 bytes
        // would turn it away) and evict ac; éa then evicts ab.
        let multibyte = engine(&["éa", "é", "ac", "ab"]);
        assert_eq!(multibyte.suggest_by_full_sort("éé", 2), ["é", "éa"]);
        assert_eq!(multibyte.suggest("éé", 2), ["é", "éa"]);
    }
}
