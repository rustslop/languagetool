use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

/// Rust port of LanguageTool's ContractionSpellingRule.
/// Detects missing apostrophes in English contractions (e.g., "dont" → "don't").
/// Case-sensitive: separate entries for lower/title/upper case.
pub struct ContractionSpellingRule {
    entries: HashMap<String, Vec<String>>,
}

impl ContractionSpellingRule {
    pub fn new() -> Self {
        let mut rule = Self {
            entries: HashMap::new(),
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/contractions.txt"));
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
            let wrong = parts[0].trim();
            let replacements: Vec<String> = parts[1].split('|').map(String::from).collect();
            self.entries.insert(wrong.to_string(), replacements);
        }
    }
}

impl Default for ContractionSpellingRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ContractionSpellingRule {
    fn id(&self) -> &str {
        "EN_CONTRACTION_SPELLING"
    }

    fn description(&self) -> &str {
        "Possible spelling mistake in contraction"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();

        for token in tokens {
            let text = token.token().token();
            if text.is_empty() {
                continue;
            }

            if let Some(replacements) = self.entries.get(text) {
                let match_start = token.token().start();
                let match_length = token.token().end() - match_start;

                let message = format!(
                    "Did you mean {}?",
                    replacements.iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(" or ")
                );

                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category());

                let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                let context_end = std::cmp::min(match_start + match_length + 40, sentence.text().len());
                let context_text = sentence.text()[context_start..context_end].to_string();

                let suggested: Vec<SuggestedReplacement> = replacements.iter()
                    .map(|s| SuggestedReplacement::new(s))
                    .collect();

                matches.push(
                    RuleMatch::new(&message, match_start, match_length, rm_rule,
                        RuleMatchContext::new(context_text, context_start, context_end - context_start))
                        .with_replacements(suggested)
                        .with_sentence(sentence.text().to_string())
                );
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
        for word in text.split_whitespace() {
            let start = text[word.as_ptr() as usize - text.as_ptr() as usize..].as_ptr();
            // Simple approach: find the word in the text
            if let Some(offset) = text[byte_pos..].find(word) {
                let word_start = byte_pos + offset;
                let word_end = word_start + word.len();
                tokens.push(AnalyzedTokenReadings::new(
                    AnalyzedToken::new(word, word_start, word_end)
                ));
                byte_pos = word_end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_contraction_dont() {
        let rule = ContractionSpellingRule::new();
        let sentence = make_sentence("I dont know");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "don't");
    }

    #[test]
    fn test_contraction_case_sensitive() {
        let rule = ContractionSpellingRule::new();
        let sentence = make_sentence("Dont worry");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "Don't");
    }

    #[test]
    fn test_contraction_no_false_positive() {
        let rule = ContractionSpellingRule::new();
        let sentence = make_sentence("I don't know");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_contraction_couldnt() {
        let rule = ContractionSpellingRule::new();
        let sentence = make_sentence("I couldnt go");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "couldn't");
    }
}
