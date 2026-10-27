use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, rule::Rule,
};
use regex::Regex;
use std::sync::LazyLock;

static PRONOUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("PNN:(.*):Nom:(.*)").expect("valid regex"));
static FUT_REAL_VERB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("VB:(Fut|Real):(.*):(.*):(.*):(.*)").expect("valid regex"));
static PAST_VERB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("VB:Past:(.*):(.*):(.*)").expect("valid regex"));

pub struct RussianVerbConjugationRule;

impl RussianVerbConjugationRule {
    pub fn new() -> Self {
        Self
    }

    fn is_conjugation_in_present_or_future_wrong(
        pronoun: (&str, &str),
        verb: (&str, &str),
    ) -> bool {
        if pronoun.1 != verb.1 {
            return true;
        }
        if matches!(pronoun.0, "Masc" | "Fem" | "Neut") {
            return verb.0 == "PL";
        }
        pronoun.0 != verb.0
    }

    fn is_conjugation_in_past_wrong(pronoun: &str, verb: &str) -> bool {
        if pronoun == "Sin" {
            return verb == "PL" || verb == "Neut";
        }
        pronoun != verb
    }
}

impl Default for RussianVerbConjugationRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for RussianVerbConjugationRule {
    fn id(&self) -> &str {
        "RU_VERB_CONJUGATION"
    }

    fn description(&self) -> &str {
        "Согласование личных местоимений с глаголами"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("GRAMMAR", "Grammar")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        if tokens.len() < 3 {
            return rule_matches;
        }
        for i in 1..tokens.len() - 1 {
            let previous = tokens[i - 1];
            let current = tokens[i];
            let next = tokens[i + 1];
            let current_token = current.token().token();
            let current_pos = current
                .readings()
                .first()
                .and_then(|r| r.pos_tags().first())
                .map(String::as_str)
                .unwrap_or("");
            if current_token.is_empty() || current_pos.is_empty() {
                continue;
            }
            let Some(pronoun_caps) = PRONOUN.captures(current_pos) else {
                continue;
            };
            if previous.token().token() == "и" {
                continue;
            }
            let pronoun_pair = (
                pronoun_caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                pronoun_caps.get(2).map(|m| m.as_str()).unwrap_or(""),
            );

            let next_token = next.token().token();
            let next_pos = next
                .readings()
                .first()
                .and_then(|r| r.pos_tags().first())
                .map(String::as_str)
                .unwrap_or("");
            if next_pos.is_empty() {
                continue;
            }
            let next2_token = if i < tokens.len() - 2 {
                tokens[i + 2].token().token()
            } else {
                ""
            };
            if next2_token == "быть" && next_token == "может" {
                continue;
            }
            if next_token == "целую" {
                continue;
            }

            let mut wrong = false;
            if let Some(verb_caps) = FUT_REAL_VERB.captures(next_pos) {
                let verb_pair = (
                    verb_caps.get(4).map(|m| m.as_str()).unwrap_or(""),
                    verb_caps.get(5).map(|m| m.as_str()).unwrap_or(""),
                );
                wrong = Self::is_conjugation_in_present_or_future_wrong(pronoun_pair, verb_pair);
            } else if let Some(verb_caps) = PAST_VERB.captures(next_pos) {
                wrong = Self::is_conjugation_in_past_wrong(
                    pronoun_pair.0,
                    verb_caps.get(3).map(|m| m.as_str()).unwrap_or(""),
                );
            }

            if wrong {
                let start = current.token().start();
                let end = next.token().end();
                let ctx_start = start.saturating_sub(40);
                let ctx_end = std::cmp::min(end + 40, sentence.text().len());
                let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                rule_matches.push(
                    RuleMatch::new(
                        "Неверное спряжение глагола или неверное местоимение",
                        start,
                        end - start,
                        rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_short_message("Неверное спряжение глагола")
                    .with_sentence(sentence.text().to_string()),
                );
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};

    fn sentence(pairs: &[(&str, &str)]) -> AnalyzedSentence {
        let text = pairs.iter().map(|p| p.0).collect::<Vec<_>>().join(" ");
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (word, pos) in pairs {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(AnalyzedTokenReadings::new(
                AnalyzedToken::new(*word, start, end)
                    .with_lemma(word.to_lowercase())
                    .with_pos_tags(vec![pos.to_string()]),
            ));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn flags_wrong_person_agreement() {
        let s = sentence(&[
            ("Я", "PNN:1Sin:Nom:Sin"),
            ("идёт", "VB:Real:3:Sin:Neut:__"),
            (".", "."),
        ]);
        let matches = RussianVerbConjugationRule::new().match_sentence(&s);
        assert_eq!(matches.len(), 1, "'Я идёт' must be flagged");
    }

    #[test]
    fn accepts_matching_agreement() {
        let s = sentence(&[
            ("Я", "PNN:1Sin:Nom:Sin"),
            ("иду", "VB:Real:1:Sin:__"),
            (".", "."),
        ]);
        let matches = RussianVerbConjugationRule::new().match_sentence(&s);
        assert!(matches.is_empty(), "correct agreement must pass");
    }
}
