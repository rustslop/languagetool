use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};

const NAME_REPETITIONS: [&str; 9] = [
    "Phi", "Li", "Xiao", "Duran", "Wagga", "Abdullah", "Nwe", "Pago", "Cao",
];

fn is_word(token: &str) -> bool {
    if token
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace())
    {
        return false;
    }
    if token.chars().count() == 1 {
        return token.chars().next().is_some_and(char::is_alphabetic);
    }
    true
}

pub struct SpanishWordRepeatRule;

impl SpanishWordRepeatRule {
    pub fn new() -> Self {
        Self
    }

    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        if position > 0
            && (tokens[position].has_pos_tag("_allow_repeat")
                || tokens[position - 1].has_pos_tag("_allow_repeat"))
        {
            return true;
        }
        let token = tokens[position].token().token();
        NAME_REPETITIONS.iter().any(|word| {
            position > 0 && tokens[position - 1].token().token() == *word && token == *word
        })
    }
}

impl Default for SpanishWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SpanishWordRepeatRule {
    fn id(&self) -> &str {
        "SPANISH_WORD_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        "Palabra repetida"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Diversos")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("duplication".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let mut prev_token: &str = "";
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if tokens[i].is_immunized() {
                prev_token = "";
                continue;
            }
            if is_word(token)
                && prev_token.to_lowercase() == token.to_lowercase()
                && !self.ignore(&tokens, i)
            {
                let prev_pos = tokens[i - 1].token().start();
                let pos = tokens[i].token().start();
                let length = pos + prev_token.len() - prev_pos;
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let ctx_start = prev_pos.saturating_sub(40);
                let ctx_end = std::cmp::min(prev_pos + length + 40, sentence.text().len());
                matches.push(
                    RuleMatch::new(
                        "Posible error tecleado: la misma palabra aparece dos veces.",
                        prev_pos,
                        length,
                        rm_rule,
                        RuleMatchContext::new(
                            sentence.text()[ctx_start..ctx_end].to_string(),
                            ctx_start,
                            ctx_end - ctx_start,
                        ),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(prev_token)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
            prev_token = token;
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish;

    #[test]
    fn flags_repeated_word() {
        let rule = SpanishWordRepeatRule::new();
        let sentences = analyze_spanish("Voy a a Paris.");
        let matches = rule.match_sentence(&sentences[0]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "a");
    }

    #[test]
    fn allows_multiword_chunk_repeats() {
        let rule = SpanishWordRepeatRule::new();
        let sentences = analyze_spanish("Era muy muy difícil.");
        let matches = rule.match_sentence(&sentences[0]);
        assert!(matches.is_empty());
    }
}
