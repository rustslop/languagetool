use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, rule::TextLevelRule,
};
use std::sync::LazyLock;

static QUOTES_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[’'\"„“”»«‚‘›‹()\\[\\]]$").unwrap());

fn is_article(token: &AnalyzedTokenReadings) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with("DT")))
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

fn is_sentence_start_token(token: &og_core::AnalyzedTokenReadings) -> bool {
    token.token().token() == "<S>" && token.has_pos_tag("SENT_START")
}

fn merge_whitespace_sentences(sentences: &[AnalyzedSentence]) -> Vec<AnalyzedSentence> {
    let mut merged: Vec<AnalyzedSentence> = Vec::with_capacity(sentences.len());
    let mut pending_ws: Option<AnalyzedSentence> = None;
    for sentence in sentences {
        if sentence.text().trim().is_empty() {
            match &mut pending_ws {
                Some(pending) => {
                    let extra: Vec<_> = sentence
                        .tokens()
                        .iter()
                        .filter(|t| !is_sentence_start_token(t))
                        .cloned()
                        .collect();
                    let mut tokens = pending.tokens().to_vec();
                    tokens.extend(extra);
                    pending.set_tokens(tokens);
                }
                None => pending_ws = Some(sentence.clone()),
            }
            continue;
        }
        match pending_ws.take() {
            Some(pending) => {
                let extra: Vec<_> = sentence
                    .tokens()
                    .iter()
                    .filter(|t| !is_sentence_start_token(t))
                    .cloned()
                    .collect();
                let mut tokens = pending.tokens().to_vec();
                tokens.extend(extra);
                let text = format!("{}{}", pending.text(), sentence.text());
                let start = pending.start();
                let end = sentence.end();
                let mut combined = AnalyzedSentence::new(text, start, end);
                combined.set_tokens(tokens);
                merged.push(combined);
            }
            None => merged.push(sentence.clone()),
        }
    }
    if let Some(trailing) = pending_ws {
        if let Some(last) = merged.last_mut() {
            let extra: Vec<_> = trailing
                .tokens()
                .iter()
                .filter(|t| !is_sentence_start_token(t))
                .cloned()
                .collect();
            let text = format!("{}{}", last.text(), trailing.text());
            let start = last.start();
            let end = trailing.end();
            let mut tokens = last.tokens().to_vec();
            tokens.extend(extra);
            *last = AnalyzedSentence::new(text, start, end);
            last.set_tokens(tokens);
        } else {
            merged.push(trailing);
        }
    }
    merged
}

pub struct ParagraphRepeatBeginningRule {
    description: String,
    message: String,
    category_label: String,
}

impl ParagraphRepeatBeginningRule {
    pub fn new(
        description: impl Into<String>,
        message: impl Into<String>,
        category_label: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            message: message.into(),
            category_label: category_label.into(),
        }
    }
}

impl TextLevelRule for ParagraphRepeatBeginningRule {
    fn id(&self) -> &str {
        "PARAGRAPH_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("STYLE", self.category_label.as_str())
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        if sentences.is_empty() {
            return rule_matches;
        }
        let sentences = &merge_whitespace_sentences(sentences)[..];
        let mut last_sentence = &sentences[0];
        let mut last_tokens = last_sentence.non_whitespace_tokens();
        for n in 0..sentences.len().saturating_sub(1) {
            if !crate::de::helpers::is_paragraph_end(sentences, n) {
                continue;
            }
            let next_sentence = &sentences[n + 1];
            let next_tokens = next_sentence.non_whitespace_tokens();
            let end_pos = num_char_equal_beginning(&last_tokens, &next_tokens);
            if end_pos > 0 {
                let msg = self.message.as_str();
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
                if let (Some(last_second), Some(second)) = (last_tokens.get(1), next_tokens.get(1))
                {
                    let start_pos = second.token().start();
                    let next_end = second.token().start() + (end_pos - last_second.token().start());
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
    use super::*;
    use crate::test_util::{sentence_range, sentences_split_after};
    use og_core::AnalyzedToken;

    fn rule() -> ParagraphRepeatBeginningRule {
        ParagraphRepeatBeginningRule::new("desc", "msg", "Style")
    }

    fn tag_pos(sentence: &mut AnalyzedSentence, word: &str, tag: &str) {
        for token in sentence.tokens_mut() {
            if token.token().token() == word {
                let mut reading = AnalyzedToken::new(
                    token.token().token(),
                    token.token().start(),
                    token.token().end(),
                );
                reading.set_pos_tags(vec![tag.to_string()]);
                *token.readings_mut() = vec![reading];
            }
        }
    }

    fn match_texts(sentences: &[AnalyzedSentence]) -> Vec<(usize, usize)> {
        rule()
            .match_text(sentences[0].text(), sentences)
            .into_iter()
            .map(|m| (m.offset(), m.length()))
            .collect()
    }

    #[test]
    fn same_word_at_paragraph_start_flags_both_paragraphs() {
        let text = "Der Hund läuft.\n\nDer Hund schläft.";
        let sentences = sentences_split_after(text, &["läuft.\n\n"]);
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[0].0..spans[0].0 + spans[0].1], "Der");
        assert_eq!(&text[spans[1].0..spans[1].0 + spans[1].1], "Der");
    }

    #[test]
    fn equal_article_prefix_extends_the_match_to_the_second_word() {
        let text = "Der Hund läuft.\n\nDer Hund schläft.";
        let mut sentences = sentences_split_after(text, &["läuft.\n\n"]);
        for sentence in &mut sentences {
            tag_pos(sentence, "Der", "DT");
        }
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[0].0..spans[0].0 + spans[0].1], "Der Hund");
        assert_eq!(&text[spans[1].0..spans[1].0 + spans[1].1], "Der Hund");
    }

    #[test]
    fn equal_article_with_different_next_word_yields_no_match() {
        let text = "Der Hund läuft.\n\nDer schläft.";
        let mut sentences = sentences_split_after(text, &["läuft.\n\n"]);
        for sentence in &mut sentences {
            tag_pos(sentence, "Der", "DT");
        }
        assert!(match_texts(&sentences).is_empty());
    }

    #[test]
    fn different_opening_words_do_not_match() {
        let text = "Der Hund läuft.\n\nEine Katze schläft.";
        let sentences = sentences_split_after(text, &["läuft.\n\n"]);
        assert!(match_texts(&sentences).is_empty());
    }

    #[test]
    fn same_word_without_a_paragraph_break_is_not_flagged() {
        let text = "Der Hund läuft. Der Hund schläft.";
        let sentences = sentences_split_after(text, &["läuft. "]);
        assert!(match_texts(&sentences).is_empty());
    }

    #[test]
    fn opening_quote_pair_is_skipped_before_comparing_words() {
        let text = "\"Guten Morgen.\" sagt er.\n\n\"Guten Tag.\" sagt sie.";
        let sentences = sentences_split_after(text, &["sagt er.\n\n"]);
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[0].0..spans[0].0 + spans[0].1], "\"Guten");
        assert_eq!(&text[spans[1].0..spans[1].0 + spans[1].1], "\"Guten");
    }

    #[test]
    fn quote_only_on_one_side_is_no_match() {
        let text = "\"Guten Morgen.\" sagt er.\n\nGuten Tag. sagt sie.";
        let sentences = sentences_split_after(text, &["sagt er.\n\n"]);
        assert!(match_texts(&sentences).is_empty());
    }

    #[test]
    fn whitespace_only_sentence_is_merged_into_the_next_paragraph() {
        let text = "Der Hund läuft.\n\nDer Hund schläft.";
        let mut sentences = Vec::new();
        let a_end = "Der Hund läuft.".len();
        sentences.push(sentence_range(text, 0, a_end));
        sentences.push(sentence_range(text, a_end, a_end + "\n\n".len()));
        sentences.push(sentence_range(text, a_end + "\n\n".len(), text.len()));
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[1].0..spans[1].0 + spans[1].1], "Der");
    }

    #[test]
    fn trailing_whitespace_sentence_attaches_to_the_last_paragraph() {
        let text = "Der Hund läuft.\n\nDer Hund schläft.\n\n";
        let sentences = sentences_split_after(text, &["läuft.\n\n", "schläft.\n\n"]);
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn short_sentences_and_empty_input_are_ignored() {
        assert!(rule().match_text("", &[]).is_empty());
        let text = "Hi.\n\nHi.";
        let sentences = sentences_split_after(text, &["Hi.\n\n"]);
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 2);
        assert_eq!(&text[spans[0].0..spans[0].0 + spans[0].1], "Hi");
    }

    #[test]
    fn repeated_beginning_across_three_paragraphs_flags_each_pair() {
        let text = "Wir starten.\n\nWir laufen.\n\nWir ruhen.";
        let sentences = sentences_split_after(text, &["starten.\n\n", "laufen.\n\n"]);
        let spans = match_texts(&sentences);
        assert_eq!(spans.len(), 4);
    }
}
