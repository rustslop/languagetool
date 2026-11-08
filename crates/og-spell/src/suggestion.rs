use crate::dictionary::Dictionary;
use std::sync::{Arc, OnceLock};

pub struct SuggestionEngine {
    dictionary: Arc<Dictionary>,
    words: OnceLock<Vec<String>>,
}

impl SuggestionEngine {
    pub fn new(dictionary: Arc<Dictionary>) -> Self {
        Self {
            dictionary,
            words: OnceLock::new(),
        }
    }

    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        // `Dictionary::word_list` yields its inline words in per-process
        // `HashSet` order, and the stable sort by distance below keeps the
        // candidate order among equal-distance words. Sorting the candidates
        // once, byte-wise, makes suggestions a pure function of the word set:
        // ascending distance, ties in byte order, identical in every run.
        // Equal entries are identical strings, so an unstable sort is enough.
        let words = self.words.get_or_init(|| {
            let mut words = self.dictionary.word_list();
            words.sort_unstable();
            words
        });
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
                    Some((w.clone(), dist))
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
}
