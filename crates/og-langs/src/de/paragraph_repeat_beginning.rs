use super::helpers::{has_pos_tag_starting_with, is_paragraph_end};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, rule::TextLevelRule,
};
use std::sync::LazyLock;

static QUOTES_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[’'\"„“”»«‚‘›‹()\\[\\]]$").unwrap());

fn is_article(token: &AnalyzedTokenReadings) -> bool {
    has_pos_tag_starting_with(token, "ART")
}

fn num_char_equal_beginning(
    last_tokens: &[&AnalyzedTokenReadings],
    next_tokens: &[&AnalyzedTokenReadings],
) -> usize {
    if last_tokens.len() < 2 || next_tokens.len() < 2 {
        return 0;
    }
    let mut n_token = 1;
    let mut last_token = last_tokens[n_token].token().token();
    let mut next_token = next_tokens[n_token].token().token();
    if QUOTES_REGEX.is_match(last_token) && last_token == next_token {
        if last_tokens.len() <= n_token + 1 || next_tokens.len() <= n_token + 1 {
            return 0;
        }
        n_token += 1;
        last_token = last_tokens[n_token].token().token();
        next_token = next_tokens[n_token].token().token();
    }
    if !last_token.chars().next().is_some_and(char::is_alphabetic) {
        return 0;
    }
    if last_tokens.len() > n_token + 1
        && is_article(last_tokens[n_token])
        && last_token == next_token
    {
        if next_tokens.len() <= n_token + 1 {
            return 0;
        }
        n_token += 1;
        last_token = last_tokens[n_token].token().token();
        next_token = next_tokens[n_token].token().token();
    }
    if !last_token.chars().next().is_some_and(char::is_alphabetic) {
        return 0;
    }
    if last_token == next_token {
        return last_tokens[n_token].token().end();
    }
    0
}

pub struct GermanParagraphRepeatBeginningRule;

impl Default for GermanParagraphRepeatBeginningRule {
    fn default() -> Self {
        Self
    }
}

impl TextLevelRule for GermanParagraphRepeatBeginningRule {
    fn id(&self) -> &str {
        "GERMAN_PARAGRAPH_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        "Gleicher Anfang von aufeinanderfolgenden Absätzen"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Stil")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        if sentences.is_empty() {
            return rule_matches;
        }
        let mut last_sentence = &sentences[0];
        let mut last_tokens = last_sentence.non_whitespace_tokens();
        for n in 0..sentences.len().saturating_sub(1) {
            if !is_paragraph_end(sentences, n) {
                continue;
            }
            let next_sentence = &sentences[n + 1];
            let next_tokens = next_sentence.non_whitespace_tokens();
            let end_pos = num_char_equal_beginning(&last_tokens, &next_tokens);
            if end_pos > 0 {
                let msg = "Der Absatz beginnt so wie der vorhergehende Absatz";
                if let Some(second) = last_tokens.get(1) {
                    let start_pos = second.token().start();
                    if start_pos < end_pos {
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        rule_matches.push(
                            RuleMatch::new(
                                msg,
                                start_pos,
                                end_pos - start_pos,
                                rm_rule,
                                RuleMatchContext::new(
                                    last_sentence.text().to_string(),
                                    start_pos,
                                    end_pos - start_pos,
                                ),
                            )
                            .with_sentence(last_sentence.text().to_string()),
                        );
                    }
                }
                if let Some(second) = next_tokens.get(1) {
                    let start_pos = second.token().start();
                    let next_end = next_sentence.start() + (end_pos - last_sentence.start());
                    if start_pos < next_end {
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        rule_matches.push(
                            RuleMatch::new(
                                msg,
                                start_pos,
                                next_end - start_pos,
                                rm_rule,
                                RuleMatchContext::new(
                                    next_sentence.text().to_string(),
                                    start_pos,
                                    next_end - start_pos,
                                ),
                            )
                            .with_sentence(next_sentence.text().to_string()),
                        );
                    }
                }
            }
            last_sentence = next_sentence;
            last_tokens = next_tokens;
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    fn repeat_hits(text: &str) -> Vec<(usize, usize)> {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["GERMAN_PARAGRAPH_REPEAT_BEGINNING_RULE".to_string()];
        engine
            .check_with_request(&request)
            .matches
            .into_iter()
            .filter(|m| m.rule().id() == "GERMAN_PARAGRAPH_REPEAT_BEGINNING_RULE")
            .map(|m| (m.offset(), m.length()))
            .collect()
    }

    #[test]
    fn detects_repeated_paragraph_beginning() {
        let text = "Der Hund ist schwarz.\n\nDer Hund ist braun.\n\nDer Hund ist weiß.";
        let hits = repeat_hits(text);
        assert!(!hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn opening_quote_prefix_extends_the_match_into_the_words() {
        let text = "\u{201E}Der Hund ist schwarz.\n\n\u{201E}Der Hund ist braun.";
        let hits = repeat_hits(text);
        let covered: Vec<&str> = hits.iter().map(|(o, l)| &text[*o..*o + *l]).collect();
        assert_eq!(covered, vec!["\u{201E}Der Hund", "\u{201E}Der Hund"]);
    }

    #[test]
    fn short_next_paragraph_yields_no_match() {
        assert!(repeat_hits("Der Hund ist schwarz.\n\nDer").is_empty());
    }

    #[test]
    fn non_alphabetic_word_after_the_article_yields_no_match() {
        assert!(repeat_hits("Der 100 Jahre dauern.\n\nDer 100 Tage gehen.").is_empty());
    }

    #[test]
    fn quote_then_number_yields_no_match() {
        assert!(repeat_hits("\u{201E}100 Jahre dauern.\n\n\u{201E}100 Tage gehen.").is_empty());
    }

    #[test]
    fn different_second_word_after_the_article_yields_no_match() {
        assert!(repeat_hits("Der Hund ist schwarz.\n\nDer Baum ist gr\u{fc}n.").is_empty());
    }

    #[test]
    fn same_paragraph_sentences_do_not_reset_the_comparison() {
        let text = "Der Hund ist schwarz. Der Hund ist braun.\n\nDer Hund ist wei\u{df}.";
        let hits = repeat_hits(text);
        let covered: Vec<&str> = hits.iter().map(|(o, l)| &text[*o..*o + *l]).collect();
        assert_eq!(covered, vec!["Der Hund", "Der Hund"]);
    }

    #[test]
    fn degenerate_inputs_return_no_matches() {
        use og_core::rule::TextLevelRule as _;
        use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

        let rule = super::GermanParagraphRepeatBeginningRule::default();
        assert!(rule.match_text("", &[]).is_empty());

        let mut first = AnalyzedSentence::new("Hallo.\n\n", 0, 8);
        first.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Hallo", 0, 5)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(".", 5, 6)),
        ]);
        let mut second = AnalyzedSentence::new("", 8, 8);
        second.set_tokens(vec![AnalyzedTokenReadings::new(AnalyzedToken::new(
            "<S>", 8, 8,
        ))]);
        assert!(rule.match_text("Hallo.\n\n", &[first, second]).is_empty());
    }

    #[test]
    fn metadata_pins_the_registration() {
        use og_core::rule::TextLevelRule as _;

        let rule = super::GermanParagraphRepeatBeginningRule::default();
        assert_eq!(rule.id(), "GERMAN_PARAGRAPH_REPEAT_BEGINNING_RULE");
        assert_eq!(
            rule.description(),
            "Gleicher Anfang von aufeinanderfolgenden Abs\u{e4}tzen"
        );
        assert!(!rule.is_default_on());
        assert_eq!(rule.category().id().as_str(), "STYLE");
        assert!(matches!(rule.issue_type(), og_core::IssueType::Style));
    }
}
