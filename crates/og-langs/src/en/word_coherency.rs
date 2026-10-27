use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::{HashMap, HashSet};

pub struct WordCoherencyRule {
    id: String,
    description: String,
    message_template: String,
    word_map: HashMap<String, HashSet<String>>,
    default_on: bool,
}

impl WordCoherencyRule {
    pub fn new() -> Self {
        Self::for_language(
            "EN_WORD_COHERENCY",
            "Coherent spelling of words with two admitted variants.",
            include_str!(
                "../../../../languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/coherency.txt"
            ),
        )
    }

    pub fn for_language(id: &str, description: &str, data: &str) -> Self {
        let mut rule = Self {
            id: id.to_string(),
            description: description.to_string(),
            message_template: "Do not mix variants of the same word ('{word1}' and '{word2}') within a single text.".to_string(),
            word_map: HashMap::new(),
            default_on: true,
        };
        rule.load_data(data);
        rule
    }

    pub fn with_message_template(mut self, template: &str) -> Self {
        self.message_template = template.to_string();
        self
    }

    pub fn with_default_off(mut self) -> Self {
        self.default_on = false;
        self
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
            self.word_map
                .entry(a.clone())
                .or_default()
                .insert(b.clone());
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
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        self.default_on
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Inconsistency
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut should_not_appear: HashMap<String, String> = HashMap::new();

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for token in &tokens {
                let text = token.token().token();
                let text_lower = text.to_lowercase();

                let lookup_word = if !token.readings().is_empty() {
                    token
                        .readings()
                        .iter()
                        .find_map(|r| r.lemma())
                        .map(|l| l.to_lowercase())
                        .unwrap_or(text_lower.clone())
                } else {
                    text_lower.clone()
                };

                if let Some(first_spelling) = should_not_appear.get(&lookup_word) {
                    let message = self
                        .message_template
                        .replace("{word1}", first_spelling)
                        .replace("{word2}", &lookup_word);
                    let from_pos = token.token().start();
                    let to_pos = token.token().end();
                    let length = to_pos - from_pos;

                    let mut replacement = text.replace(&lookup_word, first_spelling);
                    if text
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                        && let Some(first) = replacement.chars().next()
                    {
                        replacement = format!(
                            "{}{}",
                            first.to_uppercase(),
                            &replacement[first.len_utf8()..]
                        );
                    }

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());

                    let ctx_start = from_pos.saturating_sub(40).max(sentence.start());
                    let ctx_end = std::cmp::min(to_pos + 40, sentence.end());
                    let context_text = sentence.text()
                        [ctx_start - sentence.start()..ctx_end - sentence.start()]
                        .to_string();

                    if !text.eq_ignore_ascii_case(&replacement) {
                        matches.push(
                            RuleMatch::new(
                                &message,
                                from_pos,
                                length,
                                rm_rule,
                                RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                            )
                            .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                            .with_sentence(sentence.text().to_string()),
                        );
                    }
                    break;
                } else if let Some(variants) = self.word_map.get(&lookup_word) {
                    for variant in variants {
                        should_not_appear
                            .entry(variant.clone())
                            .or_insert_with(|| lookup_word.clone());
                    }
                }
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() {
                continue;
            }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    part, start, end,
                )));
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
        assert!(
            !matches.is_empty(),
            "Should detect inconsistent 'archeology' vs 'archaeology'"
        );
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
