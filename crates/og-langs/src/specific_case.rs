use std::collections::HashMap;

use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};

pub struct SpecificCaseRule {
    id: String,
    description: String,
    category_label: String,
    initial_capital_message: String,
    other_capitalization_message: String,
    lc_to_proper_spelling: HashMap<String, String>,
    max_len: usize,
}

impl SpecificCaseRule {
    pub fn from_data(
        id: impl Into<String>,
        description: impl Into<String>,
        category_label: impl Into<String>,
        initial_capital_message: impl Into<String>,
        other_capitalization_message: impl Into<String>,
        data: &str,
    ) -> Self {
        let mut lc_to_proper_spelling = HashMap::new();
        let mut max_len = 0usize;
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let phrase = line.trim();
            if phrase.is_empty() {
                continue;
            }
            max_len = max_len.max(phrase.split(' ').count());
            lc_to_proper_spelling.insert(phrase.to_lowercase(), phrase.to_string());
        }
        Self {
            id: id.into(),
            description: description.into(),
            category_label: category_label.into(),
            initial_capital_message: initial_capital_message.into(),
            other_capitalization_message: other_capitalization_message.into(),
            lc_to_proper_spelling,
            max_len,
        }
    }

    fn all_words_uppercase(s: &str) -> bool {
        s.split(' ')
            .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
    }
}

impl Rule for SpecificCaseRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CASING", self.category_label.as_str())
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        for i in 0..tokens.len() {
            let mut words: Vec<&str> = Vec::new();
            let mut j = 0usize;
            while words.len() < self.max_len && i + j < tokens.len() {
                words.push(tokens[i + j].token().token());
                j += 1;
                let phrase = words.join(" ");
                let lc_phrase = phrase.to_lowercase();
                let Some(proper_spelling) = self.lc_to_proper_spelling.get(&lc_phrase) else {
                    continue;
                };
                let is_all_uppercase = !phrase.chars().any(|c| c.is_lowercase())
                    && phrase.chars().any(|c| c.is_uppercase());
                if is_all_uppercase || phrase == *proper_spelling {
                    continue;
                }
                if i > 0
                    && tokens[i - 1].is_sentence_start()
                    && !proper_spelling
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase())
                {
                    continue;
                }
                let message = if Self::all_words_uppercase(proper_spelling) {
                    self.initial_capital_message.as_str()
                } else {
                    self.other_capitalization_message.as_str()
                };
                let start = tokens[i].token().start();
                let end = tokens[i + j - 1].token().end();
                let text_len = sentence.text().len();
                let mut ctx_start = start.saturating_sub(40);
                while !sentence.text().is_char_boundary(ctx_start) {
                    ctx_start -= 1;
                }
                let mut ctx_end = std::cmp::min(end + 40, text_len);
                while !sentence.text().is_char_boundary(ctx_end) {
                    ctx_end -= 1;
                }
                let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                matches.push(
                    RuleMatch::new(
                        message,
                        start,
                        end - start,
                        rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(proper_spelling)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::sentence;

    const DATA: &str = "\
# comment line that must be ignored

Vitamin C
New York
iPhone
";

    fn rule() -> SpecificCaseRule {
        SpecificCaseRule::from_data(
            "TEST_SPECIFIC_CASE",
            "Checks upper/lower case expressions.",
            "Casing",
            "initial capital message",
            "other capitalization message",
            DATA,
        )
    }

    #[test]
    fn flags_wrong_cased_phrase_with_proper_replacement() {
        let rule = rule();
        let text = "I live in new york now.";
        let matches = rule.match_sentence(&sentence(text));
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "new york");
        assert_eq!(m.replacements()[0].value(), "New York");
        assert_eq!(m.message(), "initial capital message");
    }

    #[test]
    fn proper_spelling_and_all_uppercase_pass_like_java() {
        let rule = rule();
        assert!(
            rule.match_sentence(&sentence("New York is big."))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&sentence("I live in NEW YORK now."))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&sentence("Vitamin C helps."))
                .is_empty()
        );
    }

    #[test]
    fn second_word_only_is_still_flagged() {
        let rule = rule();
        let text = "I like New york.";
        let matches = rule.match_sentence(&sentence(text));
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "New york");
        assert_eq!(m.replacements()[0].value(), "New York");
    }

    #[test]
    fn all_caps_initial_word_selects_the_initial_capital_message() {
        let rule = rule();
        let matches = rule.match_sentence(&sentence("She takes vitamin c daily."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "initial capital message");
    }

    #[test]
    fn lowercase_proper_spelling_uses_other_capitalization_message() {
        let rule = rule();
        let matches = rule.match_sentence(&sentence("He bought an Iphone today."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "other capitalization message");
        assert_eq!(matches[0].replacements()[0].value(), "iPhone");
    }

    #[test]
    fn lowercase_initial_proper_spelling_is_skipped_at_sentence_start() {
        let rule = rule();
        assert!(
            rule.match_sentence(&sentence("Iphone is a phone."))
                .is_empty()
        );
        let matches = rule.match_sentence(&sentence("I bought an Iphone."));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn context_window_snaps_to_char_boundaries_around_multibyte_text() {
        let rule = rule();
        let prefix = "Δ".repeat(30);
        let text = format!("{prefix} και new york");
        let matches = rule.match_sentence(&sentence(&text));
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        let ctx = m.context();
        assert!(ctx.offset() <= m.offset());
        assert_eq!(
            &text[ctx.offset()..ctx.offset() + ctx.length()],
            ctx.text(),
            "context span must land on char boundaries inside multibyte text"
        );
    }

    #[test]
    fn from_data_skips_comments_and_blank_lines_and_picks_max_len() {
        let rule = rule();
        assert_eq!(rule.lc_to_proper_spelling.len(), 3);
        assert_eq!(rule.max_len, 2);
        assert_eq!(
            rule.lc_to_proper_spelling
                .get("new york")
                .map(String::as_str),
            Some("New York")
        );
    }
}
