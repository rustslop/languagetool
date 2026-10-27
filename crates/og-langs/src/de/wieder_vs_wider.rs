use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};

pub struct WiederVsWiderRule;

impl WiederVsWiderRule {
    fn token_eq_ignore_case(token: &str, word: &str) -> bool {
        token.to_lowercase() == word.to_lowercase()
    }
}

impl Default for WiederVsWiderRule {
    fn default() -> Self {
        Self
    }
}

impl Rule for WiederVsWiderRule {
    fn id(&self) -> &str {
        "DE_WIEDER_VS_WIDER"
    }

    fn description(&self) -> &str {
        "Möglicher Tippfehler 'spiegeln ... wieder (wider)'"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Mögliche Tippfehler")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        let mut found_spiegelt = false;
        let mut found_wieder = false;
        let mut found_wider = false;
        for (i, atr) in tokens.iter().enumerate() {
            let token = atr.token().token();
            if atr.has_lemma("spiegeln") {
                found_spiegelt = true;
            } else if Self::token_eq_ignore_case(token, "wieder") && found_spiegelt {
                found_wieder = true;
            } else if Self::token_eq_ignore_case(token, "wider") && found_spiegelt {
                found_wider = true;
            }
            if found_spiegelt && found_wieder && !found_wider {
                let next1 = tokens.get(i + 1).map(|t| t.token().token());
                let next2 = tokens.get(i + 2).map(|t| t.token().token());
                if next1 == Some("wider") || next2 == Some("wider") {
                    continue;
                }
                let msg = "'wider' in 'widerspiegeln' wird mit 'i' statt mit 'ie' geschrieben, z.B. 'Das spiegelt die Situation gut wider.'";
                let short_msg = "'wider' in 'widerspiegeln' wird mit 'i' geschrieben";
                let pos = atr.token().start();
                let end = atr.token().end();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let ctx_text = sentence.text().to_string();
                rule_matches.push(
                    RuleMatch::new(
                        msg,
                        pos,
                        end - pos,
                        rm_rule,
                        RuleMatchContext::new(ctx_text.clone(), pos, end - pos),
                    )
                    .with_short_message(short_msg)
                    .with_replacements(vec![SuggestedReplacement::new("wider")])
                    .with_sentence(ctx_text),
                );
                found_spiegelt = false;
                found_wieder = false;
                found_wider = false;
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn fires_on_spiegelt_wieder() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Das spiegelt die Situation in Deutschland wieder.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_WIEDER_VS_WIDER"),
            "expected DE_WIEDER_VS_WIDER match"
        );
    }

    #[test]
    fn no_fire_on_correct_wider() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Das spiegelt die Situation in Deutschland wider.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_WIEDER_VS_WIDER")
        );
    }
}
