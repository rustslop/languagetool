use std::collections::{HashMap, HashSet};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::TextLevelRule,
};

/// Rust port of LanguageTool's WordCoherencyRule (EN_WORD_COHERENCY).
/// Detects inconsistent spelling of variant words across a document
/// (e.g., mixing "archaeology" and "archeology").
pub struct WordCoherencyRule {
    // Bidirectional map: each word → set of its variant spellings
    word_map: HashMap<String, HashSet<String>>,
}

impl WordCoherencyRule {
    pub fn new() -> Self {
        let mut rule = Self {
            word_map: HashMap::new(),
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/coherency.txt"));
        rule
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, ';').collect();
            if parts.len() != 2 {
                continue;
            }
            let a = parts[0].trim().to_lowercase();
            let b = parts[1].trim().to_lowercase();
            self.word_map.entry(a.clone()).or_default().insert(b.clone());
            self.word_map.entry(b).or_default().insert(a);
        }
    }
}

impl Default for WordCoherencyRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for WordCoherencyRule {
    fn id(&self) -> &str {
        "EN_WORD_COHERENCY"
    }

    fn description(&self) -> &str {
        "Coherent spelling of words with two admitted variants."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Inconsistency
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        // Track which word was seen first: variant → first-seen spelling
        let mut should_not_appear: HashMap<String, String> = HashMap::new();
        let mut pos = 0usize;

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for token in &tokens {
                let text = token.token().token();
                let text_lower = text.to_lowercase();

                // Try lemma first (from readings), fall back to token text
                let lookup_word = if !token.readings().is_empty() {
                    token.readings().iter()
                        .find_map(|r| r.lemma())
                        .map(|l| l.to_lowercase())
                        .unwrap_or(text_lower.clone())
                } else {
                    text_lower.clone()
                };

                if let Some(first_spelling) = should_not_appear.get(&lookup_word) {
                    let message = format!(
                        "Do not mix variants of the same word ('{}' and '{}') within a single text.",
                        first_spelling, lookup_word
                    );
                    let from_pos = pos + token.token().start();
                    let to_pos = pos + token.token().end();
                    let length = to_pos - from_pos;

                    // Create replacement by swapping the variant
                    let mut replacement = text.replace(&lookup_word, first_spelling);
                    if text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        if let Some(first) = replacement.chars().next() {
                            replacement = format!("{}{}", first.to_uppercase(), &replacement[first.len_utf8()..]);
                        }
                    }

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category());

                    let ctx_start = if from_pos >= 40 { from_pos - 40 } else { 0 };
                    let ctx_end = std::cmp::min(to_pos + 40, pos + sentence.text().len());
                    let context_text = sentence.text()[ctx_start.saturating_sub(pos)..ctx_end.saturating_sub(pos)].to_string();

                    if !text.eq_ignore_ascii_case(&replacement) {
                        matches.push(
                            RuleMatch::new(&message, from_pos, length, rm_rule,
                                RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start))
                                .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                                .with_sentence(sentence.text().to_string())
                        );
                    }
                    break; // one match per token
                } else if let Some(variants) = self.word_map.get(&lookup_word) {
                    for variant in variants {
                        should_not_appear.entry(variant.clone()).or_insert_with(|| lookup_word.clone());
                    }
                }
            }
            pos += sentence.text().len();
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
    fn test_word_coherency_mixed_spelling() {
        let rule = WordCoherencyRule::new();
        let s1 = make_sentence("He likes archaeology");
        let s2 = make_sentence("She likes archeology too");
        let full_text = "He likes archaeology She likes archeology too";
        let matches = rule.match_text(full_text, &[s1, s2]);
        assert!(!matches.is_empty(), "Should detect inconsistent 'archeology' vs 'archaeology'");
    }

    #[test]
    fn test_word_coherency_consistent() {
        let rule = WordCoherencyRule::new();
        let s1 = make_sentence("He likes archaeology");
        let s2 = make_sentence("She also likes archaeology");
        let full_text = "He likes archaeology She also likes archaeology";
        let matches = rule.match_text(full_text, &[s1, s2]);
        assert!(matches.is_empty(), "Consistent spelling should not trigger");
    }

    #[test]
    fn test_word_coherency_no_variant() {
        let rule = WordCoherencyRule::new();
        let s1 = make_sentence("The quick brown fox.");
        let matches = rule.match_text("The quick brown fox.", &[s1]);
        assert!(matches.is_empty());
    }
}
