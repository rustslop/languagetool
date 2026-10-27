use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};

pub struct GermanDashRule;

impl Default for GermanDashRule {
    fn default() -> Self {
        Self
    }
}

impl Rule for GermanDashRule {
    fn id(&self) -> &str {
        "DE_DASH"
    }

    fn description(&self) -> &str {
        "Keine Leerzeichen in Bindestrich-Komposita (wie z.B. in 'Diäten- Erhöhung')"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("COMPOUNDING", "Zusammenschreibung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        for i in 1..tokens.len() {
            let atr = tokens[i];
            let token = atr.token().token();
            let prev = tokens[i - 1];
            let prev_text = prev.token().token();
            let combined_prev;
            let prev_text = if prev_text == "-" && i >= 2 {
                combined_prev = format!("{}-", tokens[i - 2].token().token());
                &combined_prev
            } else {
                prev_text
            };
            {
                if prev_text.ends_with('-')
                    && prev_text != "-"
                    && !prev_text.contains("--")
                    && !prev_text.contains("–-")
                    && let Some(first_char) = token.chars().next()
                    && first_char.is_uppercase()
                    && !matches!(token, "UND" | "ODER" | "BZW")
                {
                    let msg = "Möglicherweise fehlt ein 'und' oder ein Komma, oder es wurde nach dem Wort ein überflüssiges Leerzeichen eingefügt. Eventuell haben Sie auch versehentlich einen Bindestrich statt eines Punktes eingefügt.";
                    let short_msg = "Fehlendes 'und' oder Komma oder überflüssiges Leerzeichen?";
                    let from = prev.token().start();
                    let to = atr.token().end();
                    let mut replacements =
                        vec![SuggestedReplacement::new(format!("{prev_text}{token}"))];
                    if prev_text.matches('-').count() + token.matches('-').count() <= 1 {
                        replacements
                            .push(SuggestedReplacement::new(format!("{prev_text}, {token}")));
                    }
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str())
                        .with_urls(vec![
                            "https://languagetool.org/insights/de/beitrag/grammatik-leerzeichen/#fehler-1-leerzeichen-vor-und-nach-satzzeichen"
                                .to_string(),
                        ]);
                    rule_matches.push(
                        RuleMatch::new(
                            msg,
                            from,
                            to - from,
                            rm_rule,
                            RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                        )
                        .with_short_message(short_msg)
                        .with_replacements(replacements)
                        .with_sentence(sentence.text().to_string()),
                    );
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_space_after_hyphen() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Bundestag beschließt Diäten- Erhöhung");
        assert!(
            matches.matches.iter().any(|m| m.rule().id() == "DE_DASH"),
            "expected DE_DASH match"
        );
    }
}
