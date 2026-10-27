use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchKind,
    RuleMatchRule,
};
use og_spell::hunspell::HunspellChecker;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

pub const HUNSPELL_NO_SUGGEST_RULE: &str = "HUNSPELL_NO_SUGGEST_RULE";

pub struct IcelandicSpellerRule {
    hunspell: Option<Arc<HunspellChecker>>,
    ignore_words: HashSet<String>,
    category: Category,
}

pub fn is_resource_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("is/src/main/resources/org/languagetool/resource/is"))
}

fn shared_hunspell() -> Option<Arc<HunspellChecker>> {
    static CACHE: og_core::releasable::Releasable<HunspellChecker> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let dir = is_resource_dir()?;
        HunspellChecker::load(
            &dir.join("hunspell/is_IS.aff"),
            &dir.join("hunspell/is_IS.dic"),
        )
        .map(Arc::new)
    })
}

fn is_ignore_words(resource_dir: &Path) -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt")) {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_lowercase());
            }
        }
    }
    ignore
}

impl IcelandicSpellerRule {
    pub fn new() -> Self {
        let resource_dir = is_resource_dir().unwrap_or_default();
        Self {
            hunspell: shared_hunspell(),
            ignore_words: is_ignore_words(&resource_dir),
            category: Category::new("TYPOS", "Hugsanleg ritvilla"),
        }
    }

    fn is_known_word(&self, word: &str) -> bool {
        if !word.chars().any(|c| c.is_alphabetic()) {
            return true;
        }
        if self.ignore_words.contains(word) || self.ignore_words.contains(&word.to_lowercase()) {
            return true;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if word.contains('/')
            || word.contains('@')
            || word.starts_with("http")
            || word.starts_with("www.")
        {
            return true;
        }
        if word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        let lowercase = word.to_lowercase();
        match &self.hunspell {
            Some(hunspell) => hunspell.spell(word) || hunspell.spell(&lowercase),
            None => false,
        }
    }
}

impl Default for IcelandicSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for IcelandicSpellerRule {
    fn id(&self) -> &str {
        HUNSPELL_NO_SUGGEST_RULE
    }

    fn description(&self) -> &str {
        "Possible spelling mistake (without suggestions)"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        for token_readings in sentence.tokens() {
            if token_readings.is_whitespace() {
                continue;
            }
            let text = token_readings.token().token();
            if text == "<S>" || token_readings.is_sentence_start() {
                continue;
            }
            if token_readings.is_ignored_by_speller() || token_readings.is_immunized() {
                continue;
            }
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }
            if self.is_known_word(text) {
                continue;
            }
            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    "Possible spelling mistake found.",
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(sentence.text().to_string(), 0, sentence.text().len()),
                )
                .with_short_message("Ritvilla")
                .with_sentence(sentence.text().to_string())
                .with_match_type(RuleMatchKind::UnknownWord),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources_present() -> bool {
        is_resource_dir().is_some_and(|d| d.join("hunspell/is_IS.dic").exists())
    }

    fn rule_matches(text: &str) -> Vec<(String, String, Vec<String>)> {
        let rule = IcelandicSpellerRule::new();
        let sentence = crate::is::example_suite::is_pipeline().analyze_range(text, 0, text.len());
        rule.match_sentence(&sentence)
            .into_iter()
            .map(|m| {
                (
                    sentence.text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.message().to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn hunspell_dictionary_loads() {
        if !resources_present() {
            return;
        }
        let hunspell = shared_hunspell();
        assert!(hunspell.is_some(), "is_IS.aff + is_IS.dic must load");
    }

    #[test]
    fn hunspell_accepts_plain_dictionary_stems() {
        if !resources_present() {
            return;
        }
        let hunspell = shared_hunspell().unwrap();
        for word in ["Þetta", "er", "setning", "tungumál", "Íslandi", "bók"] {
            assert!(hunspell.spell(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn everyday_icelandic_sentence_stays_unflagged() {
        if !resources_present() {
            return;
        }
        assert!(rule_matches("Þetta er setning.").is_empty());
    }

    #[test]
    fn ignore_txt_word_is_never_flagged() {
        if !resources_present() {
            return;
        }
        assert!(
            rule_matches("Við fengum okkur hafragraut í árdegisverður.").is_empty(),
            "árdegisverður sits in hunspell/ignore.txt"
        );
    }

    #[test]
    fn fires_without_suggestions_like_hunspell_no_suggestion_rule() {
        if !resources_present() {
            return;
        }
        let matches = rule_matches("Þetta er qvist setning.");
        assert_eq!(
            matches,
            vec![(
                "qvist".to_string(),
                "Possible spelling mistake found.".to_string(),
                Vec::new()
            )],
            "HunspellNoSuggestionRule.java:73-76 overrides getSuggestions to return an empty list"
        );
    }

    #[test]
    fn rule_carries_the_java_id_description_and_category() {
        let rule = IcelandicSpellerRule::new();
        assert_eq!(rule.id(), "HUNSPELL_NO_SUGGEST_RULE");
        assert_eq!(
            rule.description(),
            "Possible spelling mistake (without suggestions)"
        );
        assert_eq!(rule.category().name(), "Hugsanleg ritvilla");
    }
}
