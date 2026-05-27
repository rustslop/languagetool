use crate::dictionary::Dictionary;
use crate::suggestion::SuggestionEngine;
use og_core::{AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, SuggestedReplacement};
use og_core::rule::Rule;
use std::collections::HashSet;

pub struct SpellingCheckRule {
    id: String,
    dictionary: Dictionary,
    ignore_words: HashSet<String>,
    category: Category,
    suggestion_engine: SuggestionEngine,
}

impl SpellingCheckRule {
    pub fn new(dictionary: Dictionary) -> Self {
        let suggestion_engine = SuggestionEngine::new(&dictionary);
        Self {
            id: "SPELLING_RULE".to_string(),
            dictionary,
            ignore_words: HashSet::new(),
            category: Category::new("TYPOS", "Possible Typo")
                .with_description("Possible spelling mistake"),
            suggestion_engine,
        }
    }

    pub fn with_ignore_words(mut self, words: HashSet<String>) -> Self {
        self.ignore_words = words;
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn dict_size(&self) -> usize {
        self.dictionary.len()
    }

    pub fn is_known_word(&self, word: &str) -> bool {
        if !word.chars().any(|c| c.is_alphabetic()) {
            return true;
        }
        if self.ignore_words.contains(word) || self.ignore_words.contains(&word.to_lowercase()) {
            return true;
        }
        if word.len() == 1 && word.chars().next().unwrap().is_alphabetic() {
            return true;
        }
        // Skip words that look like URLs, email addresses, or file paths
        if word.contains('/') || word.contains('@') || word.starts_with("http") || word.starts_with("www.") {
            return true;
        }
        // Skip words with digits (likely codes, part numbers, etc.)
        if word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        self.dictionary.contains(word)
    }

    pub fn get_suggestions(&self, word: &str, max: usize) -> Vec<String> {
        self.suggestion_engine.suggest(word, max)
    }
}

impl Rule for SpellingCheckRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Possible spelling mistake"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();

        for token_readings in sentence.tokens() {
            if token_readings.is_whitespace() {
                continue;
            }

            let text = token_readings.token().token();

            // Skip non-word tokens
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }

            // Skip mixed alphanumeric (URLs, etc.)
            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }

            if !self.is_known_word(text) {
                let start = token_readings.token().start();
                let end = token_readings.token().end();
                let context_start = if start >= 40 { start - 40 } else { 0 };
                let context_end = std::cmp::min(end + 40, sentence.text().len());
                let context_text = sentence.text()[context_start..context_end].to_string();

                let suggestions = self.get_suggestions(text, 5);
                let replacements: Vec<SuggestedReplacement> = suggestions
                    .into_iter()
                    .map(|s| SuggestedReplacement::new(s))
                    .collect();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Misspelling.as_str());

                matches.push(
                    RuleMatch::new(
                        format!("Possible spelling mistake found: '{}'", text),
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(context_text, context_start, context_end - context_start),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(replacements)
                );
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::Rule;

    fn make_sentence_with_words(text: &str) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        for word in text.split_whitespace() {
            let start = text[pos..].find(word).unwrap() + pos;
            let end = start + word.len();
            let at = AnalyzedToken::new(word, start, end);
            tokens.push(AnalyzedTokenReadings::new(at));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_spelling_detection() {
        let dict = Dictionary::from_words(&["hello", "world", "the", "test"]);
        let rule = SpellingCheckRule::new(dict);
        let sentence = make_sentence_with_words("hello wrld test");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("wrld"));
    }

    #[test]
    fn test_no_false_positive() {
        let dict = Dictionary::from_words(&["hello", "world", "the", "test"]);
        let rule = SpellingCheckRule::new(dict);
        let sentence = make_sentence_with_words("hello world test");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_suggestions() {
        let dict = Dictionary::from_words(&["hello", "world", "help", "held", "hell"]);
        let rule = SpellingCheckRule::new(dict);
        let suggestions = rule.get_suggestions("helo", 3);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_load_real_english_spelling() {
        let hunspell_dir = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/hunspell";
        let resource_dir = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en";

        if !std::path::Path::new(hunspell_dir).exists() {
            eprintln!("Skipping: hunspell dir not found");
            return;
        }

        let mut dict = Dictionary::new();

        // Load common words
        if let Ok(words) = std::fs::read_to_string(format!("{}/common_words.txt", resource_dir)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        // Load spelling
        if let Ok(words) = std::fs::read_to_string(format!("{}/spelling.txt", hunspell_dir)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        // Load spelling_merged
        if let Ok(words) = std::fs::read_to_string(format!("{}/spelling_merged.txt", hunspell_dir)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        println!("Loaded {} words into dictionary", dict.len());
        assert!(dict.len() > 1000, "Expected at least 1000 words");

        // Test common words are known
        assert!(dict.contains("the"));
        assert!(dict.contains("hello"));
        assert!(dict.contains("world"));

        // Test the rule detects unknown words
        let rule = SpellingCheckRule::new(dict);
        let sentence = make_sentence_with_words("hello qzrmple test");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "Expected 1 match for 'qzrmple'");
        assert!(matches[0].message().contains("qzrmple"));
    }
}
