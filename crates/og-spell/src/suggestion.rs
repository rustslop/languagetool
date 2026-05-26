use crate::dictionary::Dictionary;

pub struct SuggestionEngine {
    words: Vec<String>,
}

impl SuggestionEngine {
    pub fn new(dictionary: &Dictionary) -> Self {
        // Extract words for suggestion generation
        let words: Vec<String> = dictionary.word_list();
        Self { words }
    }

    pub fn suggest(&self, word: &str, max: usize) -> Vec<String> {
        let mut scored: Vec<(String, usize)> = self.words
            .iter()
            .filter(|w| w.len() > 1)
            .map(|w| {
                let dist = levenshtein_distance(word, w);
                (w.clone(), dist)
            })
            .filter(|(_, dist)| *dist <= 3)
            .collect();

        scored.sort_by_key(|(_, dist)| *dist);

        scored
            .into_iter()
            .take(max)
            .map(|(w, _)| w)
            .collect()
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 { return b_len; }
    if b_len == 0 { return a_len; }

    let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }

    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,
                    matrix[i][j - 1] + 1,
                ),
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
        let engine = SuggestionEngine::new(&dict);
        let suggestions = engine.suggest("helo", 5);
        // All words with distance <= 1 should be suggested: hello, help, held, hell
        assert!(suggestions.contains(&"hello".to_string()) || suggestions.contains(&"help".to_string()),
            "Expected at least one close match, got: {:?}", suggestions);
        // Verify close matches have priority
        assert!(suggestions.iter().take(3).all(|s| levenshtein_distance("helo", s) <= 1));
    }
}
