use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::TextLevelRule,
};

const TYPEWRITER_APOS: char = '\'';
const TYPOGRAPHIC_APOS: char = '\u{2019}';

/// Rust port of LanguageTool's ConsistentApostrophesRule.
/// Detects mixed use of typewriter (`'`) and typographic (`'`) apostrophes.
/// Default-off style rule.
pub struct ConsistentApostrophesRule;

impl ConsistentApostrophesRule {
    pub fn new() -> Self {
        Self
    }

    fn has_typewriter_apos(text: &str) -> bool {
        text.contains(TYPEWRITER_APOS)
    }

    fn has_typographic_apos(text: &str) -> bool {
        text.contains(TYPOGRAPHIC_APOS)
    }

    fn token_has_typographic(&self, token_text: &str) -> bool {
        token_text.contains(TYPOGRAPHIC_APOS)
    }

    fn token_has_typewriter(&self, token_text: &str) -> bool {
        token_text.contains(TYPEWRITER_APOS)
    }
}

impl Default for ConsistentApostrophesRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for ConsistentApostrophesRule {
    fn id(&self) -> &str {
        "EN_CONSISTENT_APOSTROPHES"
    }

    fn description(&self) -> &str {
        "Inconsistent apostrophe style"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Style")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        // First pass: check if both apostrophe types appear in the text
        let has_tw = Self::has_typewriter_apos(text);
        let has_tg = Self::has_typographic_apos(text);

        if !has_tw || !has_tg {
            return Vec::new();
        }

        let mut matches = Vec::new();
        let rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category());

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for token in tokens {
                let token_text = token.token().token();
                let has_typographic = self.token_has_typographic(token_text);
                let has_typewriter = self.token_has_typewriter(token_text);

                if has_typewriter && !has_typographic {
                    let match_start = token.token().start();
                    let match_length = token.token().end() - match_start;
                    let replacement = token_text.replace(TYPEWRITER_APOS, &TYPOGRAPHIC_APOS.to_string());

                    let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let context_end = std::cmp::min(match_start + match_length + 40, text.len());
                    let context_text = text[context_start..context_end].to_string();

                    matches.push(
                        RuleMatch::new(
                            "You used a typewriter-style apostrophe here, but a typographic apostrophe elsewhere. Consider using the same type everywhere.",
                            match_start, match_length,
                            rm_rule.clone(),
                            RuleMatchContext::new(context_text, context_start, context_end - context_start),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                        .with_sentence(sentence.text().to_string())
                    );
                } else if has_typographic {
                    let match_start = token.token().start();
                    let match_length = token.token().end() - match_start;
                    let replacement = token_text.replace(TYPOGRAPHIC_APOS, &TYPEWRITER_APOS.to_string());

                    let context_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let context_end = std::cmp::min(match_start + match_length + 40, text.len());
                    let context_text = text[context_start..context_end].to_string();

                    matches.push(
                        RuleMatch::new(
                            "You used a typographic apostrophe here, but a typewriter-style apostrophe elsewhere. Consider using the same type everywhere.",
                            match_start, match_length,
                            rm_rule.clone(),
                            RuleMatchContext::new(context_text, context_start, context_end - context_start),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                        .with_sentence(sentence.text().to_string())
                    );
                }
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;

    fn make_sentences(text: &str) -> (String, Vec<AnalyzedSentence>) {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() { continue; }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(og_core::AnalyzedTokenReadings::new(
                    AnalyzedToken::new(part, start, end)
                ));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        (text.to_string(), vec![sentence])
    }

    #[test]
    fn test_mixed_apostrophes_typewriter() {
        let rule = ConsistentApostrophesRule::new();
        let text = "it\u{2019}s a don't";
        let (text, sentences) = make_sentences(text);
        let matches = rule.match_text(&text, &sentences);
        assert!(!matches.is_empty(), "Should detect typewriter apostrophe in mixed text");
        assert!(matches.iter().any(|m| m.replacements().iter().any(|r| r.value().contains('\u{2019}'))));
    }

    #[test]
    fn test_consistent_typographic() {
        let rule = ConsistentApostrophesRule::new();
        let text = "it\u{2019}s a don\u{2019}t";
        let (text, sentences) = make_sentences(text);
        let matches = rule.match_text(&text, &sentences);
        assert!(matches.is_empty(), "No match when all apostrophes are consistent");
    }

    #[test]
    fn test_consistent_typewriter() {
        let rule = ConsistentApostrophesRule::new();
        let text = "it's a don't";
        let (text, sentences) = make_sentences(text);
        let matches = rule.match_text(&text, &sentences);
        assert!(matches.is_empty(), "No match when all apostrophes are consistent");
    }

    #[test]
    fn test_no_apostrophes() {
        let rule = ConsistentApostrophesRule::new();
        let text = "no apostrophes here";
        let (text, sentences) = make_sentences(text);
        let matches = rule.match_text(&text, &sentences);
        assert!(matches.is_empty());
    }
}
