use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

/// Rust port of LanguageTool's EnglishSpecificCaseRule.
/// Detects improper capitalization of known proper nouns and multi-word expressions.
/// Uses a sliding window over tokens to match phrases against a lookup table.
pub struct SpecificCaseRule {
    entries: HashMap<String, String>,
    max_words: usize,
}

impl SpecificCaseRule {
    pub fn new() -> Self {
        let mut rule = Self {
            entries: HashMap::new(),
            max_words: 0,
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/specific_case.txt"));
        rule
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Strip inline comments
            let phrase = if let Some(pos) = line.find('#') {
                line[..pos].trim()
            } else {
                line
            };
            if phrase.is_empty() {
                continue;
            }

            let word_count = phrase.split_whitespace().count();
            if word_count > self.max_words {
                self.max_words = word_count;
            }
            self.entries.insert(phrase.to_lowercase(), phrase.to_string());
        }
    }

    fn is_all_uppercase(s: &str) -> bool {
        let mut has_letter = false;
        for c in s.chars() {
            if c.is_alphabetic() {
                has_letter = true;
                if c.is_lowercase() {
                    return false;
                }
            }
        }
        has_letter
    }

    fn all_words_start_uppercase(s: &str) -> bool {
        s.split_whitespace().all(|word| {
            word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        })
    }
}

impl Default for SpecificCaseRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SpecificCaseRule {
    fn id(&self) -> &str {
        "EN_SPECIFIC_CASE"
    }

    fn description(&self) -> &str {
        "Checks upper/lower case spelling of some proper nouns"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Capitalization")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();

        if tokens.is_empty() || self.entries.is_empty() {
            return matches;
        }

        let n = tokens.len();
        for i in 0..n {
            for j in 1..=self.max_words {
                if i + j > n {
                    break;
                }
                let phrase: String = tokens[i..i + j]
                    .iter()
                    .map(|t| t.token().token())
                    .collect::<Vec<_>>()
                    .join(" ");

                let lc_phrase = phrase.to_lowercase();
                if let Some(proper) = self.entries.get(&lc_phrase) {
                    // Skip if already correct
                    if &phrase == proper {
                        continue;
                    }
                    // Skip ALL CAPS (stylistic choice)
                    if Self::is_all_uppercase(&phrase) {
                        continue;
                    }
                    // Sentence-start guard: don't suggest lowercase-starting proper nouns
                    // when the word is at sentence start (capitalization is mandatory there)
                    if i > 0 {
                        let prev = &tokens[i - 1];
                        if prev.has_pos_tag("SENT_START") || prev.token().token() == "." || prev.token().token() == "!" || prev.token().token() == "?" {
                            if !proper.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                continue;
                            }
                        }
                    }

                    let match_start = tokens[i].token().start();
                    let match_end = tokens[i + j - 1].token().end();
                    let match_length = match_end - match_start;

                    let message = if Self::all_words_start_uppercase(proper) {
                        "If the term is a proper noun, use initial capitals."
                    } else {
                        "If the term is a proper noun, use the suggested capitalization."
                    };

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_urls(vec!["https://languagetool.org/insights/post/spelling-capital-letters/".to_string()]);

                    let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let context_end = std::cmp::min(match_end + 40, sentence.text().len());
                    let context_text = sentence.text()[context_start..context_end].to_string();

                    matches.push(
                        RuleMatch::new(message, match_start, match_length, rm_rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start))
                            .with_replacements(vec![SuggestedReplacement::new(proper)])
                            .with_sentence(sentence.text().to_string())
                    );
                    break; // Don't report the same start position multiple times
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
    fn test_specific_case_harry_potter() {
        let rule = SpecificCaseRule::new();
        let sentence = make_sentence("I like Harry potter");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'Harry potter'");
        assert_eq!(matches[0].replacements()[0].value(), "Harry Potter");
    }

    #[test]
    fn test_specific_case_correct() {
        let rule = SpecificCaseRule::new();
        let sentence = make_sentence("I like Harry Potter");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "No match when already correct");
    }

    #[test]
    fn test_specific_case_all_caps_ok() {
        let rule = SpecificCaseRule::new();
        let sentence = make_sentence("I like HARRY POTTER");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "ALL CAPS should be skipped");
    }

    #[test]
    fn test_specific_case_multiword() {
        let rule = SpecificCaseRule::new();
        let sentence = make_sentence("Visit the statue of liberty");
        let matches = rule.match_sentence(&sentence);
        // "statue of liberty" should match "Statue of Liberty"
        assert!(!matches.is_empty(), "Should detect multi-word phrase");
    }

    #[test]
    fn test_specific_case_no_match() {
        let rule = SpecificCaseRule::new();
        let sentence = make_sentence("the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }
}
