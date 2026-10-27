use super::helpers::{is_capitalized_word, is_tagged, lowercase_first_char};
use og_core::checker::Tagger;
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, rule::Rule,
};
use std::sync::Arc;

const MIN_TOKENS_FOR_ERROR: usize = 5;

pub struct MissingVerbRule {
    tagger: Option<Arc<dyn Tagger>>,
}

impl MissingVerbRule {
    pub fn new(tagger: Option<Arc<dyn Tagger>>) -> Self {
        Self { tagger }
    }

    fn is_real_sentence(&self, tokens: &[&AnalyzedTokenReadings]) -> bool {
        tokens.last().is_some_and(|last| {
            last.has_pos_tag("PKT") && matches!(last.token().token(), "." | "?" | "!")
        })
    }

    fn is_special_case(&self, tokens: &[&AnalyzedTokenReadings]) -> bool {
        let consecutive = |a: &str, b: &str| {
            tokens
                .windows(2)
                .any(|w| w[0].token().token() == a && w[1].token().token() == b)
        };
        consecutive("Vielen", "Dank") || consecutive("Herzlichen", "Glückwunsch")
    }

    fn verb_at_sentence_start(&self, readings: &AnalyzedTokenReadings) -> bool {
        let Some(tagger) = &self.tagger else {
            return false;
        };
        let lowercased = lowercase_first_char(readings.token().token());
        let mut atr =
            AnalyzedTokenReadings::new(AnalyzedToken::new(&lowercased, 0, lowercased.len()));
        tagger.tag(std::slice::from_mut(&mut atr));
        atr.readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER")))
    }
}

impl Rule for MissingVerbRule {
    fn id(&self) -> &str {
        "MISSING_VERB"
    }

    fn description(&self) -> &str {
        "Satz ohne Verb"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("GRAMMAR", "Grammatik")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        if !self.is_real_sentence(&tokens) || self.is_special_case(&tokens) {
            return Vec::new();
        }
        let mut verb_found = false;
        let mut last_token: Option<&AnalyzedTokenReadings> = None;
        for (i, readings) in tokens.iter().enumerate() {
            let token = readings.token().token();
            if readings
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER")))
                || (!is_tagged(readings) && !is_capitalized_word(token))
                || (i == 1 && self.verb_at_sentence_start(readings))
            {
                verb_found = true;
                break;
            }
            last_token = Some(readings);
        }
        if !verb_found
            && let Some(last) = last_token
            && tokens.len() >= MIN_TOKENS_FOR_ERROR
        {
            let from = sentence.start();
            let to = last.token().end();
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            return vec![
                RuleMatch::new(
                    "Dieser Satz scheint kein Verb zu enthalten",
                    from,
                    to - from,
                    rm_rule,
                    RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                )
                .with_sentence(sentence.text().to_string()),
            ];
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {

    fn check_enabled(text: &str) -> og_core::CheckResult {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["MISSING_VERB".to_string()];
        engine.check_with_request(&request)
    }

    #[test]
    fn no_verb_in_long_sentence() {
        let matches = check_enabled("In diesem Satz kein Wort.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "MISSING_VERB"),
            "expected MISSING_VERB match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset(), m.length()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn verb_present_no_match() {
        let matches = check_enabled("In diesem Satz fehlt kein Wort.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "MISSING_VERB")
        );
    }

    #[test]
    fn special_case_ignored() {
        let matches = check_enabled("Vielen Dank für die schönen Blumen.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "MISSING_VERB")
        );
    }
}
