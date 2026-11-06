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
        // dictionary candidates, with a scratch row pair shared between
        // calls so candidate scoring does not allocate per word.
        let lower_chars: Vec<char> = lower.chars().collect();
        let mut scratch = (Vec::new(), Vec::new());

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
/// a few rows instead of a full matrix. The minimum of any DP row is a lower
/// bound on the final distance, and the length-difference pre-check bounds
/// it before any cell is computed. Distances <= max are exact, matching the
/// uncapped computation, so the accepted set and stored distances are
/// unchanged.
fn levenshtein_distance_capped(
    a: &[char],
    b: &str,
    max: usize,
    scratch: &mut (Vec<usize>, Vec<usize>),
) -> usize {
    levenshtein_impl(a, b, Some(max), scratch)
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
                let mut scratch = (Vec::new(), Vec::new());
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
