use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

struct WordinessEntry {
    suggestions: Vec<String>,
    message: Option<String>,
}

/// Rust port of LanguageTool's EnglishPlainEnglishRule.
/// Suggests simpler alternatives for wordy phrases (e.g., "in order to" → "to").
pub struct PlainEnglishRule {
    entries: HashMap<String, WordinessEntry>,
    max_words: usize,
}

impl PlainEnglishRule {
    pub fn new() -> Self {
        let mut rule = Self {
            entries: HashMap::new(),
            max_words: 0,
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/wordiness.txt"));
        rule
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                continue;
            }
            let phrase = parts[0].trim();
            let right = parts[1].trim();

            let (suggestions_str, message) = if let Some(tab_pos) = right.find('\t') {
                (&right[..tab_pos], Some(right[tab_pos + 1..].to_string()))
            } else {
                (right, None)
            };

            let suggestions: Vec<String> = suggestions_str.split('|').map(String::from).collect();
            let word_count = phrase.split_whitespace().count();
            if word_count > self.max_words {
                self.max_words = word_count;
            }
            self.entries.insert(phrase.to_lowercase(), WordinessEntry { suggestions, message });
        }
    }
}

impl Default for PlainEnglishRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PlainEnglishRule {
    fn id(&self) -> &str {
        "EN_PLAIN_ENGLISH"
    }

    fn description(&self) -> &str {
        "Suggest simpler alternatives for wordy phrases"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("PLAIN_ENGLISH", "Plain English")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("style".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let n = tokens.len();

        for i in 0..n {
            for len in 1..=self.max_words {
                if i + len > n {
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
                    let match_end = tokens[i + len - 1].token().end();
                    let match_length = match_end - match_start;

                    let default_msg = format!(
                        "Consider using {} instead of \"{}\".",
                        entry.suggestions.iter()
                            .map(|s| format!("\"{}\"", s))
                            .collect::<Vec<_>>()
                            .join(" or "),
                        phrase
                    );
                    let message = entry.message.as_deref().unwrap_or(&default_msg);

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category());

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
                    break;
                }
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings, AnalyzedSentence};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() { continue; }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(
                    AnalyzedToken::new(part, start, end)
                ));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_plain_english_in_order_to() {
        let rule = PlainEnglishRule::new();
        let sentence = make_sentence("I went in order to see");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'in order to'");
    }

    #[test]
    fn test_plain_english_no_match() {
        let rule = PlainEnglishRule::new();
        let sentence = make_sentence("I went to see");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_plain_english_i_myself() {
        let rule = PlainEnglishRule::new();
        let sentence = make_sentence("I myself went there");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'I myself'");
        assert_eq!(matches[0].replacements()[0].value(), "I");
    }
}
