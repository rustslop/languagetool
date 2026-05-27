use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

/// Rust port of LanguageTool's EnglishDiacriticsRule.
/// Detects missing diacritical marks in loanwords (e.g., "cafe" → "café").
pub struct DiacriticsRule {
    entries: HashMap<String, String>,
    max_words: usize,
}

impl DiacriticsRule {
    pub fn new() -> Self {
        let mut rule = Self {
            entries: HashMap::new(),
            max_words: 0,
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/diacritics.txt"));
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
            let wrong = parts[0].trim().to_lowercase();
            let correct = parts[1].trim().to_string();
            let word_count = wrong.split_whitespace().count();
            if word_count > self.max_words {
                self.max_words = word_count;
            }
            self.entries.insert(wrong, correct);
        }
    }
}

impl Default for DiacriticsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DiacriticsRule {
    fn id(&self) -> &str {
        "EN_DIACRITICS"
    }

    fn description(&self) -> &str {
        "Words with diacritics"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
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

                if let Some(correct) = self.entries.get(&phrase_lower) {
                    // Skip if already correct (case-sensitive exact match)
                    if &phrase == correct {
                        continue;
                    }

                    let match_start = tokens[i].token().start();
                    let match_end = tokens[i + len - 1].token().end();
                    let match_length = match_end - match_start;

                    let message = format!("Did you mean '{}'?", correct);
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category());

                    let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let context_end = std::cmp::min(match_end + 40, sentence.text().len());
                    let context_text = sentence.text()[context_start..context_end].to_string();

                    matches.push(
                        RuleMatch::new(&message, match_start, match_length, rm_rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start))
                            .with_replacements(vec![SuggestedReplacement::new(correct)])
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
    fn test_diacritics_aperitif() {
        let rule = DiacriticsRule::new();
        let sentence = make_sentence("I ordered an aperitif");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'aperitif' → 'apéritif'");
        assert_eq!(matches[0].replacements()[0].value(), "apéritif");
    }

    #[test]
    fn test_diacritics_correct_spelling() {
        let rule = DiacriticsRule::new();
        let sentence = make_sentence("I ordered an apéritif");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty() || !matches.iter().any(|m| m.message().contains("apéritif")));
    }

    #[test]
    fn test_diacritics_multiword() {
        let rule = DiacriticsRule::new();
        let sentence = make_sentence("a la carte menu");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'a la carte'");
    }

    #[test]
    fn test_diacritics_no_match() {
        let rule = DiacriticsRule::new();
        let sentence = make_sentence("the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }
}
