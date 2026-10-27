use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, TextLevelRule,
};

pub const ROMANIAN_WORD_REPEAT_BEGINNING_RULE: &str = "ROMANIAN_WORD_REPEAT_BEGINNING_RULE";

fn is_adverb(atr: &AnalyzedTokenReadings) -> bool {
    let mut is_adverb = false;
    for reading in atr.readings() {
        let pos = reading.pos_tags().join("+");
        if pos.is_empty() {
            continue;
        }
        if pos.starts_with('G') {
            is_adverb = true;
        } else {
            return false;
        }
    }
    is_adverb
}

fn is_exception(token: &str) -> bool {
    matches!(token, ":" | "–" | "-" | "✔️" | "➡️" | "—" | "⭐️" | "⚠️")
}

pub struct RomanianWordRepeatBeginningRule;

impl RomanianWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RomanianWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for RomanianWordRepeatBeginningRule {
    fn id(&self) -> &str {
        ROMANIAN_WORD_REPEAT_BEGINNING_RULE
    }

    fn description(&self) -> &str {
        "Propoziții succesive încep cu același cuvânt."
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repetitions (Style)")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut matches = Vec::new();
        let mut prev_sentence: Option<&AnalyzedSentence> = None;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let mut token = String::new();
            if tokens.len() > 1 {
                token = tokens[1].token().token().to_string();
            }
            if tokens.len() > 3 {
                let analyzed_token = tokens[1];
                let is_word = token.chars().count() != 1
                    || token.chars().next().is_some_and(char::is_alphabetic);
                let prev_ended_like_sentence = prev_sentence.is_some_and(|prev| {
                    let trimmed = prev.text().trim();
                    let java_dot =
                        |c: char| !matches!(c, '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}');
                    trimmed.ends_with(['.', '?', '!'])
                        && trimmed[..trimmed.len() - 1].chars().any(java_dot)
                });
                if is_word
                    && last_token == token
                    && !is_exception(&token)
                    && !is_exception(tokens[2].token().token())
                    && !is_exception(tokens[3].token().token())
                    && prev_ended_like_sentence
                {
                    let short_msg = if is_adverb(analyzed_token) {
                        "Două propoziții succesive încep cu același adverb."
                    } else if before_last_token == token {
                        "Trei propoziții succesive încep cu același cuvânt."
                    } else {
                        ""
                    };
                    if !short_msg.is_empty() {
                        let msg = format!(
                            "{short_msg} Consider rewording the sentence or use a thesaurus to find a synonym."
                        );
                        let start_pos = analyzed_token.token().start();
                        let end_pos = start_pos + token.len();
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        matches.push(
                            RuleMatch::new(
                                msg,
                                start_pos,
                                end_pos - start_pos,
                                rm_rule,
                                RuleMatchContext::new(token.clone(), start_pos, token.len()),
                            )
                            .with_short_message(short_msg)
                            .with_sentence(sentence.text().to_string()),
                        );
                    }
                }
            }
            before_last_token = last_token;
            last_token = token;
            prev_sentence = Some(sentence);
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ro::example_suite::ro_pipeline;
    #[test]
    fn flags_two_sentences_beginning_with_the_same_adverb() {
        let rule = RomanianWordRepeatBeginningRule::new();
        let text = "Numai ieri am plecat. Numai azi venim.";
        let sentences = ro_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "numai carries only G-prefixed adverb readings, so the pair must flag"
        );
        assert!(matches[0].message().starts_with("Două propoziții"));
        let expected_start = text.find("Numai azi").unwrap();
        assert_eq!(
            matches[0].offset(),
            expected_start,
            "the pipeline's tokens carry absolute offsets, so the match must point at the \
             second sentence's adverb"
        );
        assert_eq!(matches[0].length(), "Numai".len());
    }

    #[test]
    fn non_adverb_pairs_wait_for_the_third_sentence() {
        let rule = RomanianWordRepeatBeginningRule::new();
        let two = ro_pipeline().analyze_text("Ion pleacă. Ion vine.");
        assert!(rule.match_text("Ion pleacă. Ion vine.", &two).is_empty());
        let three_text = "Ion pleacă. Ion vine. Ion doarme.";
        let three = ro_pipeline().analyze_text(three_text);
        let matches = rule.match_text(three_text, &three);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().starts_with("Trei propoziții"));
    }

    #[test]
    fn ambiguous_adverb_readings_do_not_flag() {
        let rule = RomanianWordRepeatBeginningRule::new();
        let text = "Și merge acolo. Și vine înapoi.";
        let sentences = ro_pipeline().analyze_text(text);
        assert!(
            rule.match_text(text, &sentences).is_empty(),
            "'Și' is the canonical ambiguous adverb (adverb + conjunction + pronoun readings, \
             RomanianWordRepeatBeginningRule.java:57-63), so allowAmbiguousAdverbs()=false vetoes"
        );
    }
}
