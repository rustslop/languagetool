use crate::dictionary::Dictionary;
use crate::suggestion::SuggestionEngine;
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchKind,
    RuleMatchRule, SuggestedReplacement,
};
use std::collections::HashSet;

pub struct SpellingCheckRule {
    id: String,
    dictionary: std::sync::Arc<Dictionary>,
    extra_words: HashSet<String>,
    ignore_words: HashSet<String>,
    category: Category,
    suggestion_engine: SuggestionEngine,
}

impl SpellingCheckRule {
    pub fn new(dictionary: Dictionary) -> Self {
        let dictionary = std::sync::Arc::new(dictionary);
        let suggestion_engine = SuggestionEngine::new(std::sync::Arc::clone(&dictionary));
        Self {
            id: "SPELLING_RULE".to_string(),
            dictionary,
            extra_words: HashSet::new(),
            ignore_words: HashSet::new(),
            category: Category::new("TYPOS", "Possible Typo")
                .with_description("Possible spelling mistake"),
            suggestion_engine,
        }
    }

    pub fn new_shared(
        dictionary: std::sync::Arc<Dictionary>,
        id: impl Into<String>,
        extra_words: HashSet<String>,
    ) -> Self {
        let mut rule = Self::with_shared_dictionary(dictionary);
        rule.id = id.into();
        rule.extra_words = extra_words;
        rule
    }

    pub fn with_shared_dictionary(dictionary: std::sync::Arc<Dictionary>) -> Self {
        let suggestion_engine = SuggestionEngine::new(std::sync::Arc::clone(&dictionary));
        Self {
            id: "SPELLING_RULE".to_string(),
            dictionary,
            extra_words: HashSet::new(),
            ignore_words: HashSet::new(),
            category: Category::new("TYPOS", "Possible Typo")
                .with_description("Possible spelling mistake"),
            suggestion_engine,
        }
    }

    pub fn shared_dictionary(&self) -> std::sync::Arc<Dictionary> {
        self.dictionary.clone()
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
        if word.contains('/')
            || word.contains('@')
            || word.starts_with("http")
            || word.starts_with("www.")
        {
            return true;
        }
        if word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        self.dictionary.contains(word) || self.extra_words.contains(word)
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
        let tokens = sentence.tokens();
        let mut is_first_word = true;

        for (idx, token_readings) in tokens.iter().enumerate() {
            if token_readings.is_whitespace() {
                continue;
            }

            let text = token_readings.token().token();

            if text == "<S>" || token_readings.is_sentence_start() {
                continue;
            }

            if token_readings.is_ignored_by_speller() || token_readings.is_immunized() {
                continue;
            }

            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }

            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }

            if !self.is_known_word(text) {
                let start = token_readings.token().start();
                let end = token_readings.token().end();
                let full_text = sentence.text();
                let rel_start = start.saturating_sub(sentence.start());
                let rel_end = end.saturating_sub(sentence.start()).min(full_text.len());
                let mut context_start = rel_start.saturating_sub(40);
                while context_start < full_text.len() && !full_text.is_char_boundary(context_start)
                {
                    context_start += 1;
                }
                let mut context_end = std::cmp::min(rel_end + 40, full_text.len());
                while context_end > context_start && !full_text.is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = full_text[context_start..context_end].to_string();

                let suggestions = self.get_suggestions(text, 5);
                let replacements: Vec<SuggestedReplacement> = suggestions
                    .into_iter()
                    .map(SuggestedReplacement::new)
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
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(replacements)
                    .with_match_type(RuleMatchKind::UnknownWord),
                );

                if is_first_word && !matches.is_empty() && idx < tokens.len() - 1 {
                    let capitalized: Vec<SuggestedReplacement> = matches[0]
                        .replacements()
                        .iter()
                        .map(|r| {
                            let value = r.value();
                            if value == value.to_lowercase() {
                                let mut chars = value.chars();
                                chars
                                    .next()
                                    .map(|first| {
                                        let upper: String =
                                            first.to_uppercase().collect::<String>()
                                                + chars.as_str();
                                        upper
                                    })
                                    .unwrap_or_else(|| value.to_string())
                            } else {
                                value.to_string()
                            }
                        })
                        .map(SuggestedReplacement::new)
                        .collect();
                    matches[0].set_replacements(capitalized);
                }
            }

            if idx > 0 && is_first_word && text.chars().any(|c| c.is_alphanumeric()) {
                is_first_word = false;
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::Rule;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};

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
        let hunspell_dir = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/hunspell";
        let resource_dir = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en";

        if !std::path::Path::new(hunspell_dir).exists() {
            eprintln!("Skipping: hunspell dir not found");
            return;
        }

        let mut dict = Dictionary::new();

        if let Ok(words) = std::fs::read_to_string(format!("{}/common_words.txt", resource_dir)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        if let Ok(words) = std::fs::read_to_string(format!("{}/spelling.txt", hunspell_dir)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        if let Ok(words) = std::fs::read_to_string(format!("{}/spelling_merged.txt", hunspell_dir))
        {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }

        println!("Loaded {} words into dictionary", dict.len());
        assert!(dict.len() > 1000, "Expected at least 1000 words");

        assert!(dict.contains("the"));
        assert!(dict.contains("hello"));
        assert!(dict.contains("world"));

        let rule = SpellingCheckRule::new(dict);
        let sentence = make_sentence_with_words("hello qzrmple test");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "Expected 1 match for 'qzrmple'");
        assert!(matches[0].message().contains("qzrmple"));
    }

    fn base_dict() -> Dictionary {
        Dictionary::from_words(&["hello", "world", "the", "test", "Haus"])
    }

    #[test]
    fn constructors_wire_id_extras_and_shared_dictionary() {
        let dict = std::sync::Arc::new(base_dict());
        let shared = SpellingCheckRule::with_shared_dictionary(std::sync::Arc::clone(&dict));
        assert_eq!(shared.id(), "SPELLING_RULE");
        assert!(shared.shared_dictionary().contains("hello"));

        let renamed = shared
            .with_id("DE_SPELLER")
            .with_ignore_words(HashSet::from(["igno".to_string()]));
        assert_eq!(renamed.id(), "DE_SPELLER");
        assert!(renamed.is_known_word("IGNO"));

        let mut extra = HashSet::new();
        extra.insert("extra".to_string());
        let rule = SpellingCheckRule::new_shared(std::sync::Arc::clone(&dict), "OTHER_ID", extra);
        assert_eq!(rule.id(), "OTHER_ID");
        assert!(rule.is_known_word("extra"));
        assert!(!rule.is_known_word("Hausx"));
        assert_eq!(rule.dict_size(), dict.len());
    }

    #[test]
    fn trait_accessors_report_metadata() {
        let rule = SpellingCheckRule::new(base_dict());
        assert_eq!(rule.id(), "SPELLING_RULE");
        assert_eq!(rule.description(), "Possible spelling mistake");
        assert_eq!(rule.category().name(), "Possible Typo");
        assert_eq!(rule.issue_type(), IssueType::Misspelling);
    }

    #[test]
    fn is_known_word_skips_non_words_and_ignores() {
        let rule = SpellingCheckRule::new(base_dict());
        for word in [
            "123", "-", "--", "3.5", "a", "B", "x/y", "u@v", "httpx", "www.site", "abc12",
        ] {
            assert!(rule.is_known_word(word), "expected known: {word}");
        }
        assert!(rule.is_known_word("hello"));
        assert!(rule.is_known_word("haus"));
        assert!(!rule.is_known_word("hellos"));

        let ignoring = SpellingCheckRule::new(base_dict())
            .with_ignore_words(HashSet::from(["Wrdx".to_string(), "igno".to_string()]));
        assert!(ignoring.is_known_word("Wrdx"));
        assert!(ignoring.is_known_word("IGNO"));
    }

    #[test]
    fn match_sentence_skips_special_tokens() {
        let rule = SpellingCheckRule::new(base_dict());
        let mut sentence = AnalyzedSentence::new("hello wrld test", 0, "hello wrld test".len());
        let hello = AnalyzedTokenReadings::new(AnalyzedToken::new("hello", 0, 5));
        let sent_start = AnalyzedTokenReadings::new(AnalyzedToken::new("wrld", 6, 10))
            .with_readings(vec![
                AnalyzedToken::new("wrld", 6, 10).with_pos_tags(vec!["SENT_START".to_string()]),
            ]);
        let marker = AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 11, 14));
        let mut immunized = AnalyzedTokenReadings::new(AnalyzedToken::new("qqqq", 0, 0));
        immunized.immunize();
        let mut ignored = AnalyzedTokenReadings::new(AnalyzedToken::new("zzzz", 0, 0));
        ignored.ignore_spelling();
        let whitespace = AnalyzedTokenReadings::new(AnalyzedToken::new(" ", 0, 0));
        let multi_dot = AnalyzedTokenReadings::new(AnalyzedToken::new("e.g.", 0, 0));
        let digits = AnalyzedTokenReadings::new(AnalyzedToken::new("abc12", 0, 0));
        let non_alpha = AnalyzedTokenReadings::new(AnalyzedToken::new("123", 0, 0));
        sentence.set_tokens(vec![
            hello, sent_start, marker, immunized, ignored, whitespace, multi_dot, digits, non_alpha,
        ]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn first_word_misspelling_gets_capitalized_replacement() {
        let rule = SpellingCheckRule::new(base_dict());
        let sentence = make_sentence_with_words("helo wrld");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 2);
        let first = matches
            .iter()
            .find(|m| m.message().contains("helo"))
            .expect("helo flagged");
        let first_values: Vec<String> = first
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(first_values.first(), Some(&"Hello".to_string()));
        let second = matches
            .iter()
            .find(|m| m.message().contains("wrld"))
            .expect("wrld flagged");
        assert!(
            second
                .replacements()
                .iter()
                .all(|r| r.value() == r.value().to_lowercase())
        );
    }

    #[test]
    fn capitalized_first_word_keeps_already_capitalized_replacement() {
        let rule = SpellingCheckRule::new(base_dict());
        let sentence = make_sentence_with_words("Helo test");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        let values: Vec<String> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values.first(), Some(&"Hello".to_string()));
        assert_eq!(
            matches[0].message(),
            "Possible spelling mistake found: 'Helo'"
        );
    }

    #[test]
    fn context_snaps_to_char_boundaries_around_multibyte_text() {
        let rule = SpellingCheckRule::new(base_dict());
        let text = format!(
            "{}x{} wrld {}",
            "ä".repeat(20),
            "ö".repeat(20),
            "ü".repeat(30)
        );
        let mut sentence = AnalyzedSentence::new(text.clone(), 0, text.len());
        let mut readings = Vec::new();
        let mut search_from = 0;
        for word in text.split_whitespace() {
            let start = text[search_from..].find(word).unwrap() + search_from;
            let end = start + word.len();
            search_from = end;
            readings.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
        }
        sentence.set_tokens(readings);
        let matches = rule.match_sentence(&sentence);
        let target = matches
            .iter()
            .find(|m| m.message().contains("wrld"))
            .expect("wrld flagged");
        let context = target.context().text();
        assert!(context.contains("wrld"));
        assert!(text.contains(context));
    }
}
