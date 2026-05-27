use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

struct ReplaceEntry {
    suggestions: Vec<String>,
    message: Option<String>,
    url: Option<String>,
}

/// Rust port of LanguageTool's SimpleReplaceRule.
/// Data-driven word/phrase replacement based on replace.txt format.
pub struct SimpleReplaceRule {
    id: String,
    name: String,
    category_id: String,
    category_name: String,
    entries: HashMap<String, ReplaceEntry>,
}

impl SimpleReplaceRule {
    pub fn new(id: &str, name: &str, category_id: &str, category_name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            category_id: category_id.to_string(),
            category_name: category_name.to_string(),
            entries: HashMap::new(),
        }
    }

    /// Load replacement data from LT replace.txt format
    pub fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }

            let wrong_forms: Vec<&str> = parts[0].split('|').collect();
            let right_side = parts[1];

            let (suggestions_str, message) = if let Some(tab_pos) = right_side.find('\t') {
                let suggestions = &right_side[..tab_pos];
                let msg = &right_side[tab_pos + 1..];
                (suggestions, Some(msg.to_string()))
            } else {
                (right_side, None)
            };

            let suggestions: Vec<String> = suggestions_str.split('|').map(String::from).collect();
            let url = message.as_ref().and_then(|m| {
                if m.starts_with("http://") || m.starts_with("https://") {
                    Some(m.clone())
                } else {
                    None
                }
            });

            let entry = ReplaceEntry {
                suggestions,
                message: if url.is_some() { None } else { message },
                url,
            };

            for wrong in &wrong_forms {
                self.entries.insert(wrong.trim().to_lowercase(), entry.clone());
            }
        }
    }

    /// Create the default English SimpleReplaceRule
    pub fn english_default() -> Self {
        let mut rule = Self::new(
            "EN_SIMPLE_REPLACE",
            "Possible spelling mistake",
            "TYPOS",
            "Possible Typo",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/replace.txt");
        rule.load_data(data);
        rule
    }

    /// Create the English redundancies rule
    pub fn english_redundancies() -> Self {
        let mut rule = Self::new(
            "EN_REDUNDANCY",
            "Redundant expression",
            "REDUNDANCY",
            "Redundancy",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/redundancies.txt");
        rule.load_data(data);
        rule
    }

    /// Create the English profanity replacement rule
    pub fn english_profanity() -> Self {
        let mut rule = Self::new(
            "EN_REPLACE_PROFANITY",
            "Profanity replacement",
            "TYPOS",
            "Possible Typo",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/replace_profanity.txt");
        rule.load_data(data);
        rule
    }

    /// British words easily confused in American English
    pub fn american_replace() -> Self {
        let mut rule = Self::new(
            "EN_US_SIMPLE_REPLACE",
            "British words easily confused in American English",
            "STYLE",
            "Style",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/en-US/replace.txt");
        rule.load_data(data);
        rule
    }

    /// American words easily confused in British English
    pub fn british_replace() -> Self {
        let mut rule = Self::new(
            "EN_GB_SIMPLE_REPLACE",
            "American words easily confused in British English",
            "STYLE",
            "Style",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/en-GB/replace.txt");
        rule.load_data(data);
        rule
    }

    /// New Zealand specific replacements
    pub fn new_zealand_replace() -> Self {
        let mut rule = Self::new(
            "EN_NZ_SIMPLE_REPLACE",
            "New Zealand specific replacement",
            "STYLE",
            "Style",
        );

        let data = include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/en-NZ/replace.txt");
        rule.load_data(data);
        rule
    }
}

impl Rule for SimpleReplaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.name
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new(&self.category_id, &self.category_name)
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();

        // Check single tokens and 2-3 word phrases
        for i in 0..tokens.len() {
            for len in 1..=3 {
                if i + len > tokens.len() {
                    break;
                }
                let phrase: String = tokens[i..i + len]
                    .iter()
                    .map(|t| t.token().token())
                    .collect::<Vec<_>>()
                    .join(" ");
                let phrase_lower = phrase.to_lowercase();

                if let Some(entry) = self.entries.get(&phrase_lower) {
                    let match_start = tokens[i].token().start();
                    let last_idx = i + len - 1;
                    let match_end = tokens[last_idx].token().end();
                    let match_length = match_end - match_start;

                    let default_msg = format!(
                        "Did you mean {}?",
                        entry.suggestions.iter()
                            .map(|s| format!("\"{}\"", s))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    );
                    let message = entry.message.as_deref().unwrap_or(&default_msg);

                    let rm_rule = RuleMatchRule::new(&self.id, &self.name)
                        .with_category(Category::new(&self.category_id, &self.category_name));

                    let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let context_end = std::cmp::min(match_end + 40, sentence.text().len());
                    let context_text = sentence.text()[context_start..context_end].to_string();

                    let replacements: Vec<SuggestedReplacement> = entry.suggestions.iter()
                        .map(|s| SuggestedReplacement::new(s))
                        .collect();

                    matches.push(
                        RuleMatch::new(message, match_start, match_length, rm_rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start))
                            .with_replacements(replacements)
                            .with_sentence(sentence.text().to_string())
                    );
                    break; // Don't report the same start position multiple times
                }
            }
        }

        matches
    }
}

impl Clone for ReplaceEntry {
    fn clone(&self) -> Self {
        Self {
            suggestions: self.suggestions.clone(),
            message: self.message.clone(),
            url: self.url.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings, AnalyzedSentence};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        let chars: Vec<char> = text.chars().collect();
        while pos < chars.len() {
            while pos < chars.len() && chars[pos].is_whitespace() { pos += 1; }
            if pos >= chars.len() { break; }
            let start_byte = text[..text.char_indices().nth(pos).map(|(i,_)| i).unwrap_or(text.len())].len();
            if chars[pos].is_alphanumeric() || chars[pos] == '\'' || chars[pos] == '-' {
                let word_start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '\'' || chars[pos] == '-') { pos += 1; }
                let word: String = chars[word_start..pos].iter().collect();
                let end_byte = text[..text.char_indices().nth(pos).map(|(i,_)| i).unwrap_or(text.len())].len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(&word, start_byte, end_byte)));
            } else {
                let ch = chars[pos].to_string();
                pos += 1;
                let end_byte = text[..text.char_indices().nth(pos).map(|(i,_)| i).unwrap_or(text.len())].len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(&ch, start_byte, end_byte)));
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_simple_replace_basic() {
        let mut rule = SimpleReplaceRule::new(
            "TEST_REPLACE", "Test", "TYPOS", "Typos"
        );
        rule.load_data("bussines|bussiness=business\n");

        let sentence = make_sentence("bussines is hard");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "business");
    }

    #[test]
    fn test_simple_replace_case_insensitive() {
        let mut rule = SimpleReplaceRule::new(
            "TEST_REPLACE", "Test", "TYPOS", "Typos"
        );
        rule.load_data("bussines=business\n");

        let sentence = make_sentence("Bussines is hard");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_simple_replace_no_match() {
        let mut rule = SimpleReplaceRule::new(
            "TEST_REPLACE", "Test", "TYPOS", "Typos"
        );
        rule.load_data("bussines=business\n");

        let sentence = make_sentence("business is fine");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_simple_replace_with_message() {
        let mut rule = SimpleReplaceRule::new(
            "TEST_REPLACE", "Test", "TYPOS", "Typos"
        );
        rule.load_data("bussines=business\tDid you mean business?\n");

        let sentence = make_sentence("bussines");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("Did you mean business?"));
    }

    #[test]
    fn test_simple_replace_multiple_alternatives() {
        let mut rule = SimpleReplaceRule::new(
            "TEST_REPLACE", "Test", "TYPOS", "Typos"
        );
        rule.load_data("wrong=option1|option2\n");

        let sentence = make_sentence("wrong choice");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements().len(), 2);
    }
}
