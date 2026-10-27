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
        let words = self.words.get_or_init(|| self.dictionary.word_list());
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

        let mut scored: Vec<(String, usize)> = words
            .iter()
            .filter(|w| {
                let wlen = w.len();
                wlen > 1 && wlen.abs_diff(word_len) <= max_dist
            })
            .filter_map(|w| {
                let dist = levenshtein_distance(&lower, w);
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

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }

    for (j, cell) in matrix[0].iter_mut().enumerate() {
        *cell = j;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(matrix[i - 1][j] + 1, matrix[i][j - 1] + 1),
                matrix[i - 1][j - 1] + cost,
            );
        }
    }

    matrix[a_len][b_len]
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
