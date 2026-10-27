use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, TextLevelRule,
};

fn is_punctuation_mark(token: &str) -> bool {
    token.chars().count() == 1
        && token
            .chars()
            .all(|c| c.is_ascii_punctuation() || "…–—«»¿¡".contains(c))
}

fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

struct NoWhitespaceView<'a> {
    tokens: Vec<&'a AnalyzedTokenReadings>,
    ws_before: Vec<bool>,
    sentence_end: Vec<bool>,
}

fn no_whitespace_view(sentence: &AnalyzedSentence) -> NoWhitespaceView<'_> {
    let mut tokens = Vec::new();
    let mut ws_before = Vec::new();
    let mut sentence_end = Vec::new();
    let mut pending_ws = false;
    for atr in sentence.tokens().iter().skip(1) {
        if atr.is_whitespace() {
            pending_ws = true;
            continue;
        }
        tokens.push(atr);
        ws_before.push(pending_ws);
        sentence_end.push(atr.is_sentence_end());
        pending_ws = false;
    }
    NoWhitespaceView {
        tokens,
        ws_before,
        sentence_end,
    }
}

pub struct QuestionMarkRule;

impl QuestionMarkRule {
    pub fn new() -> Self {
        Self
    }

    fn has_token_at_pos(ch: &str, view: &NoWhitespaceView) -> i64 {
        let mut i = view.tokens.len() as i64 - 1;
        while i > 0 {
            if view.tokens[i as usize].token().token() == ch {
                let next = (i + 1) as usize;
                if next < view.tokens.len()
                    && !view.ws_before[next]
                    && !is_punctuation_mark(view.tokens[next].token().token())
                    && !view.tokens[next].is_whitespace()
                {
                } else {
                    return i;
                }
            }
            i -= 1;
        }
        -1
    }
}

impl Default for QuestionMarkRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for QuestionMarkRule {
    fn id(&self) -> &str {
        "ES_QUESTION_MARK"
    }

    fn description(&self) -> &str {
        "Signos de exclamación / interrogación desparejados"
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typografía")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut pos = 0usize;
        for sentence in sentences {
            let view = no_whitespace_view(sentence);
            if view.tokens.is_empty() {
                pos += sentence.text().len();
                continue;
            }
            let needs_inv_question_mark_at = Self::has_token_at_pos("?", &view);
            let needs_inv_excl_mark_at = Self::has_token_at_pos("!", &view);
            if needs_inv_question_mark_at > 1 || needs_inv_excl_mark_at > 1 {
                let mut has_inv_question_mark = false;
                let mut has_inv_excl_mark = false;
                let mut first_token: Option<&AnalyzedTokenReadings> = None;
                for i in 0..view.tokens.len() {
                    let token_text = view.tokens[i].token().token();
                    if first_token.is_none()
                        && !view.tokens[i].is_sentence_start()
                        && !is_punctuation_mark(token_text)
                    {
                        first_token = Some(view.tokens[i]);
                    }
                    if token_text == "¿" && (i as i64) < needs_inv_question_mark_at {
                        has_inv_question_mark = true;
                    } else if token_text == "¡" && (i as i64) < needs_inv_excl_mark_at {
                        has_inv_excl_mark = true;
                    }
                    if !view.sentence_end[i]
                        && ((token_text == "?" && (i as i64) > needs_inv_question_mark_at)
                            || (token_text == "!" && (i as i64) > needs_inv_excl_mark_at))
                    {
                        first_token = None;
                    }
                    if i > 2 && i + 2 < view.tokens.len() {
                        let prev_tok = view.tokens[i - 1].token().token();
                        let next1 = view.tokens[i + 1].token().token();
                        if prev_tok == ","
                            && view.tokens[i].has_pos_tag("CC")
                            && view.tokens[i + 1].has_pos_tag("SPS00")
                            && (has_pos_tag_starting_with(view.tokens[i + 2], "PT")
                                || has_pos_tag_starting_with(view.tokens[i + 2], "DT"))
                        {
                            first_token = Some(view.tokens[i]);
                        }
                        if prev_tok == ","
                            && view.tokens[i].has_pos_tag("SPS00")
                            && (has_pos_tag_starting_with(view.tokens[i + 1], "PT")
                                || has_pos_tag_starting_with(view.tokens[i + 1], "DT"))
                        {
                            first_token = Some(view.tokens[i]);
                        }
                        if prev_tok == ","
                            && view.tokens[i].has_pos_tag("CC")
                            && (next1 == "no" || next1 == "sí")
                        {
                            first_token = Some(view.tokens[i]);
                        }
                    }
                    if i > 2 && i < view.tokens.len() {
                        let prev_tok = view.tokens[i - 1].token().token();
                        let tok = view.tokens[i].token().token();
                        if prev_tok == "," && matches!(tok, "no" | "sí" | "eh") {
                            first_token = Some(view.tokens[i]);
                        }
                    }
                }
                if let Some(first) = first_token
                    && !first.has_pos_tag("_english_ignore_")
                {
                    let s = if needs_inv_question_mark_at > 1 && needs_inv_excl_mark_at > 1 {
                        None
                    } else if needs_inv_question_mark_at > 1 && !has_inv_question_mark {
                        Some("¿")
                    } else if needs_inv_excl_mark_at > 1 && !has_inv_excl_mark {
                        Some("¡")
                    } else {
                        None
                    };
                    if let Some(s) = s {
                        let message = format!("Símbolo desparejado: Parece que falta un '{s}'");
                        let start = pos + first.token().start();
                        let length = first.token().end() - first.token().start();
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        let context = first.token().token().to_string();
                        matches.push(
                            RuleMatch::new(
                                message,
                                start,
                                length,
                                rm_rule,
                                RuleMatchContext::new(context, start, length),
                            )
                            .with_replacements(vec![og_core::SuggestedReplacement::new(format!(
                                "{s}{}",
                                first.token().token()
                            ))])
                            .with_sentence(sentence.text().to_string()),
                        );
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
    use crate::es::test_support::analyze_spanish;

    #[test]
    fn flags_missing_inverted_question_mark() {
        let rule = QuestionMarkRule::new();
        let sentences = analyze_spanish("Qué pasa?");
        let matches = rule.match_text("Qué pasa?", &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].replacements()[0].value(), "¿Qué");
    }

    #[test]
    fn accepts_paired_marks() {
        let rule = QuestionMarkRule::new();
        let sentences = analyze_spanish("¿Qué pasa?");
        let matches = rule.match_text("¿Qué pasa?", &sentences);
        assert!(matches.is_empty());
    }

    #[test]
    fn flags_missing_inverted_exclamation_mark() {
        let rule = QuestionMarkRule::default();
        let sentences = analyze_spanish("Qué pasa!");
        let matches = rule.match_text("Qué pasa!", &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].replacements()[0].value(), "¡Qué");
        assert!(matches[0].message().contains("falta un '¡'"));
    }

    #[test]
    fn accepts_paired_exclamations() {
        let rule = QuestionMarkRule::new();
        let sentences = analyze_spanish("¡Qué pasa!");
        let matches = rule.match_text("¡Qué pasa!", &sentences);
        assert!(matches.is_empty());
    }

    #[test]
    fn skips_sentences_ending_in_both_marks() {
        let rule = QuestionMarkRule::new();
        let sentences = analyze_spanish("Nunca tienes clases o qué?!");
        let matches = rule.match_text("Nunca tienes clases o qué?!", &sentences);
        assert!(matches.is_empty());
    }

    #[test]
    fn ignores_marks_joined_to_the_following_word() {
        let rule = QuestionMarkRule::new();
        let text = "see?no";
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let start = AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".into()]),
        );
        let question = AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("?", 3, 4));
        let joined = AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("no", 4, 6));
        sentence.set_tokens(vec![start, question, joined]);
        let matches = rule.match_text(text, &[sentence]);
        assert!(matches.is_empty());
    }

    #[test]
    fn skips_whitespace_only_sentences() {
        let rule = QuestionMarkRule::new();
        let text = "Qué pasa?   ";
        let sentences = analyze_spanish(text);
        let mut whitespace_only = AnalyzedSentence::new("   ", 0, 3);
        let start = AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".into()]),
        );
        whitespace_only.set_tokens(vec![
            start,
            AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(" ", 0, 3)),
        ]);
        let matches = rule.match_text(text, &[whitespace_only, sentences[0].clone()]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 3);
    }

    #[test]
    fn keeps_global_offsets_across_sentences() {
        let rule = QuestionMarkRule::new();
        let text = "Qué pasa? Qué tal?";
        let sentences = analyze_spanish(text);
        assert_eq!(sentences.len(), 2);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[1].offset(), sentences[0].text().len());
    }

    #[test]
    fn moves_the_mark_onto_the_conjunction_after_a_comma() {
        let rule = QuestionMarkRule::new();
        let text = "No sé aún, y de qué hablas?";
        let sentences = analyze_spanish(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "¿y");
        assert!(text[matches[0].offset()..].starts_with("y"));
    }

    #[test]
    fn moves_the_mark_onto_no_after_a_comma() {
        let rule = QuestionMarkRule::new();
        let text = "Vendrás mañana, no?";
        let sentences = analyze_spanish(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "¿no");
        assert!(text[matches[0].offset()..].starts_with("no"));
    }

    #[test]
    fn guillemets_are_not_treated_as_the_first_word() {
        let rule = QuestionMarkRule::new();
        let text = "«Qué pasa?»";
        let sentences = analyze_spanish(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "¿Qué");
        assert!(text[matches[0].offset()..].starts_with("Qué"));
    }
}
