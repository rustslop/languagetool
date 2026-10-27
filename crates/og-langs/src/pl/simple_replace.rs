use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const POLISH_SIMPLE_REPLACE_RULE: &str = "PL_SIMPLE_REPLACE";

struct ReplaceData {
    wrong_words: HashMap<String, Vec<String>>,
}

fn load_replace_data(data: &str) -> ReplaceData {
    let mut wrong_words: HashMap<String, Vec<String>> = HashMap::new();
    for line in data.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((wrong_side, replacement_side)) = line.split_once('=') else {
            continue;
        };
        if replacement_side.trim().is_empty() {
            continue;
        }
        let replacements: Vec<String> = replacement_side
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if replacements.is_empty() {
            continue;
        }
        for wrong_form in wrong_side.split('|') {
            wrong_words.insert(wrong_form.to_string(), replacements.clone());
        }
    }
    ReplaceData { wrong_words }
}

fn shared_data() -> Arc<ReplaceData> {
    static CACHE: OnceLock<Arc<ReplaceData>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            Arc::new(load_replace_data(include_str!(
                "../../../../languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/rules/pl/replace.txt"
            )))
        })
        .clone()
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

pub struct PolishSimpleReplaceRule {
    data: Arc<ReplaceData>,
    category: Category,
}

impl PolishSimpleReplaceRule {
    pub fn new() -> Self {
        Self {
            data: shared_data(),
            category: Category::new("PRAWDOPODOBNE_LITEROWKI", "Prawdopodobne literówki"),
        }
    }

    fn cleanup(&self, word: &str) -> String {
        word.to_lowercase()
    }

    fn message(&self, token_str: &str, replacements: &[String]) -> String {
        format!(
            "Wyraz „{token_str}” to najczęściej literówka; poprawnie pisze się: {}.",
            replacements.join(", ")
        )
    }

    fn find_matches(
        &self,
        token: &og_core::AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        let original = token.token().token().to_string();
        let token_string = self.cleanup(&original);
        let mut replacements = self
            .data
            .wrong_words
            .get(&original)
            .cloned()
            .or_else(|| self.data.wrong_words.get(&token_string).cloned())
            .unwrap_or_default();
        if !replacements.is_empty() {
            if is_all_uppercase(&original) {
                replacements = replacements.iter().map(|s| s.to_uppercase()).collect();
            }
            replacements.retain(|s| *s != original);
            if original.chars().next().is_some_and(char::is_uppercase) {
                replacements = replacements
                    .iter()
                    .map(|s| uppercase_first_char(s))
                    .collect();
            }
        }
        if replacements.is_empty() {
            return Vec::new();
        }
        let pos = token.token().start();
        let end = token.token().end();
        let text = sentence.text();
        let ctx_start = pos.saturating_sub(sentence.start()).saturating_sub(40);
        let ctx_end = ((end - sentence.start()) + 40).min(text.len());
        let rm_rule = RuleMatchRule::new(
            POLISH_SIMPLE_REPLACE_RULE,
            "Typowe literówki i niepoprawne wyrazy (domowi, sie, niewiadomo, duh, cie…)",
        )
        .with_category(self.category.clone())
        .with_issue_type(IssueType::Misspelling.as_str());
        vec![
            RuleMatch::new(
                self.message(&original, &replacements),
                pos,
                end - pos,
                rm_rule,
                RuleMatchContext::new(
                    text[ctx_start..ctx_end].to_string(),
                    ctx_start,
                    ctx_end - ctx_start,
                ),
            )
            .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect())
            .with_sentence(text.to_string())
            .with_short_message("Literówka"),
        ]
    }
}

impl Default for PolishSimpleReplaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PolishSimpleReplaceRule {
    fn id(&self) -> &str {
        POLISH_SIMPLE_REPLACE_RULE
    }

    fn description(&self) -> &str {
        "Typowe literówki i niepoprawne wyrazy (domowi, sie, niewiadomo, duh, cie…)"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        sentence
            .non_whitespace_tokens()
            .into_iter()
            .filter(|t| {
                !t.has_pos_tag("SENT_START") && !t.is_immunized() && !t.is_ignored_by_speller()
            })
            .flat_map(|t| self.find_matches(t, sentence))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        PolishSimpleReplaceRule::new().match_sentence(&sentence)
    }

    #[test]
    fn java_pins_wrong_word_is_flagged_with_the_replacement() {
        let matches = matches_for("Uspokój sie.");
        assert_eq!(matches.len(), 1, "Uspokój sei. example pair: {matches:?}");
        assert!(
            matches[0].replacements().iter().any(|r| r.value() == "się"),
            "{:?}",
            matches[0].replacements()
        );
        assert!(
            matches[0].message().contains("„sie”"),
            "{}",
            matches[0].message()
        );
    }

    #[test]
    fn java_pins_capitalized_surface_uppercases_the_suggestion() {
        let matches = matches_for("Uspokój Sie.");
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "Się")),
            "capitalized wrong word gets a capitalized replacement: {matches:?}"
        );
        let matches = matches_for("Uspokój SIE.");
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "SIĘ")),
            "all-uppercase wrong word gets an uppercase replacement: {matches:?}"
        );
    }

    #[test]
    fn java_pins_correct_words_do_not_fire() {
        assert!(matches_for("Uspokój się.").is_empty());
        assert!(matches_for("W tym domu.").is_empty());
    }
}
