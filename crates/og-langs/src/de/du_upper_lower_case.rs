use super::helpers::{
    is_all_uppercase, lowercase_first_char, starts_with_uppercase, uppercase_first_char,
};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::HashSet;
use std::sync::LazyLock;

static LOWER_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "du", "dir", "dich", "dein", "deine", "deines", "deins", "deiner", "deinen", "deinem",
        "euch", "euer", "eure", "euere", "euren", "eueren", "euern", "eurer", "euerer", "eurem",
        "euerem", "eures", "eueres",
    ]
    .into_iter()
    .collect()
});

const QUOTE_OR_SEPARATOR: [&str; 11] = ["\"", "„", "‚", ":", "»", "«", "“", "-", "–", "*", "•"];

fn is_sentence_start_token(atr: &og_core::AnalyzedTokenReadings) -> bool {
    atr.is_sentence_start()
}

pub struct DuUpperLowerCaseRule;

impl Default for DuUpperLowerCaseRule {
    fn default() -> Self {
        Self
    }
}

impl TextLevelRule for DuUpperLowerCaseRule {
    fn id(&self) -> &str {
        "DE_DU_UPPER_LOWER"
    }

    fn description(&self) -> &str {
        "Einheitliche Verwendung von Du/du, Dir/dir etc."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Groß-/Kleinschreibung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("casing".to_string())
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut first_use: Option<String> = None;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for (i, atr) in tokens.iter().enumerate() {
                if i > 0
                    && (is_sentence_start_token(tokens[i - 1])
                        || QUOTE_OR_SEPARATOR.contains(&tokens[i - 1].token().token()))
                {
                    continue;
                }
                let word = atr.token().token();
                let lc_word = word.to_lowercase();
                if !LOWER_WORDS.contains(lc_word.as_str()) {
                    continue;
                }
                if first_use.is_none() {
                    first_use = Some(word.to_string());
                    continue;
                }
                let first_use = first_use.as_deref().unwrap_or_default();
                let first_use_is_upper = starts_with_uppercase(first_use);
                let mut replacement = None;
                let mut msg = None;
                if first_use_is_upper && !starts_with_uppercase(word) {
                    replacement = Some(uppercase_first_char(word));
                    msg = Some(format!(
                        "Vorher wurde bereits '{first_use}' großgeschrieben. Aus Gründen der Einheitlichkeit '{}' hier auch großschreiben?",
                        replacement.clone().unwrap_or_default()
                    ));
                } else if !first_use_is_upper
                    && starts_with_uppercase(word)
                    && !is_all_uppercase(word)
                {
                    replacement = Some(lowercase_first_char(word));
                    msg = Some(format!(
                        "Vorher wurde bereits '{first_use}' kleingeschrieben. Aus Gründen der Einheitlichkeit '{}' hier auch kleinschreiben?",
                        replacement.clone().unwrap_or_default()
                    ));
                }
                if let (Some(msg), Some(replacement)) = (msg, replacement) {
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str())
                        .with_urls(vec![
                            "https://languagetool.org/insights/de/beitrag/duzen-grossgeschrieben/"
                                .to_string(),
                        ]);
                    let from = atr.token().start();
                    let to = atr.token().end();
                    rule_matches.push(
                        RuleMatch::new(
                            msg,
                            from,
                            to - from,
                            rm_rule,
                            RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(replacement)])
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
    fn detects_inconsistent_du() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Wie geht es Dir? Bist du wieder gesund?");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_DU_UPPER_LOWER"),
            "expected DE_DU_UPPER_LOWER match"
        );
    }
}
