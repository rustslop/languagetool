use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchKind,
    RuleMatchRule, SuggestedReplacement,
};
use og_spell::{Dictionary, SuggestionEngine};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock};

static RUSSIAN_LETTERS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[-а-яёо́а́е́у́и́ы́э́ю́я́о̀а̀ѐу̀ѝы̀э̀ю̀я̀ʼА-ЯЁ]*$").unwrap());

const DO_NOT_SUGGEST: &[&str] = &["блоггер", "елка", "дрочим", "анальный", "орочем"];

pub struct RussianYoSpellerRule {
    resource_dir: PathBuf,
    dictionary: OnceLock<Option<Arc<Dictionary>>>,
    suggestion_engine: OnceLock<Option<SuggestionEngine>>,
    do_not_suggest: HashSet<String>,
}

impl RussianYoSpellerRule {
    pub fn from_resource_dir(resource_dir: &std::path::Path) -> Option<Self> {
        if !resource_dir.join("hunspell/ru_RU_yo.dict").exists() {
            return None;
        }
        Some(Self {
            resource_dir: resource_dir.to_path_buf(),
            dictionary: OnceLock::new(),
            suggestion_engine: OnceLock::new(),
            do_not_suggest: DO_NOT_SUGGEST.iter().map(|w| w.to_lowercase()).collect(),
        })
    }

    fn dictionary(&self) -> Option<&Arc<Dictionary>> {
        self.dictionary
            .get_or_init(|| shared_yo_dictionary(&self.resource_dir))
            .as_ref()
    }

    fn suggestions(&self, word: &str) -> Vec<String> {
        let engine = self.suggestion_engine.get_or_init(|| {
            self.dictionary()
                .map(|dict| SuggestionEngine::new(std::sync::Arc::clone(dict)))
        });
        let Some(engine) = engine else {
            return Vec::new();
        };
        engine
            .suggest(word, 5)
            .into_iter()
            .filter(|s| !self.do_not_suggest.contains(&s.to_lowercase()))
            .collect()
    }

    pub fn dictionary_len(&self) -> usize {
        self.dictionary().map_or(0, |dict| dict.len())
    }
}

fn shared_yo_dictionary(resource_dir: &std::path::Path) -> Option<Arc<Dictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Arc<Dictionary>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(resource_dir) {
        return Some(dict.clone());
    }
    let dict = og_tagger::morfologik::FsaWordSet::load(
        &resource_dir.join("hunspell/ru_RU_yo.dict"),
        &resource_dir.join("hunspell/ru_RU_yo.info"),
    )
    .map(std::sync::Arc::new)
    .map(Dictionary::from_fsa_word_set)
    .map(Arc::new)?;
    guard.insert(resource_dir.to_path_buf(), dict.clone());
    Some(dict)
}

impl Rule for RussianYoSpellerRule {
    fn id(&self) -> &str {
        "MORFOLOGIK_RULE_RU_RU_YO"
    }

    fn description(&self) -> &str {
        "Проверка орфографии. Только «Ё» (экспериментальное правило)."
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo").with_description("Possible spelling mistake")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let Some(dictionary) = self.dictionary() else {
            return matches;
        };
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
            if !RUSSIAN_LETTERS.is_match(text) {
                continue;
            }
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }
            if dictionary.contains(text) {
                continue;
            }
            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let full_text = sentence.text();
            let rel_start = start.saturating_sub(sentence.start());
            let rel_end = end.saturating_sub(sentence.start()).min(full_text.len());
            let mut context_start = rel_start.saturating_sub(40);
            while context_start < full_text.len() && !full_text.is_char_boundary(context_start) {
                context_start += 1;
            }
            let mut context_end = std::cmp::min(rel_end + 40, full_text.len());
            while context_end > context_start && !full_text.is_char_boundary(context_end) {
                context_end += 1;
                if context_end > full_text.len() {
                    context_end = full_text.len();
                    break;
                }
            }
            let context_text = full_text[context_start..context_end].to_string();
            let suggestions: Vec<SuggestedReplacement> = self
                .suggestions(text)
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect();
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    format!("Possible spelling mistake found: '{text}'"),
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(suggestions)
                .with_match_type(RuleMatchKind::UnknownWord),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
        )
    }

    #[test]
    fn loads_yo_dictionary_with_java_id_and_default_off() {
        let rule = RussianYoSpellerRule::from_resource_dir(&resource_dir()).expect("yo rule");
        assert!(rule.dictionary_len() > 100_000);
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_RU_RU_YO");
        assert!(!rule.is_default_on());
        assert!(rule.description().contains("Ё"));
    }

    #[test]
    fn stays_silent_by_default_and_flags_misspelling_when_enabled() {
        let _release_guard = crate::release_guard();
        let engine = crate::engine::LanguageEngine::shared_for_code("ru").expect("ru engine");
        let good = "Все счастливые семьи похожи друг на друга, каждая несчастливая семья несчастлива по-своему.";
        let bad = "Все счастливые семьи похожи друг на друга, каждя несчастливая семья несчастлива по-своему.";

        assert!(
            !engine
                .check(good)
                .matches
                .iter()
                .chain(engine.check(bad).matches.iter())
                .any(|m| m.rule().id() == "MORFOLOGIK_RULE_RU_RU_YO"),
            "default-off rule must not fire in a default check"
        );

        let mut request = og_core::CheckRequest::new(good, engine.language().clone());
        request.enabled_rules = vec!["MORFOLOGIK_RULE_RU_RU_YO".to_string()];
        assert!(
            engine
                .check_with_request(&request)
                .matches
                .iter()
                .all(|m| m.rule().id() != "MORFOLOGIK_RULE_RU_RU_YO"),
            "correct yo-spelled text must not be flagged"
        );

        let mut request = og_core::CheckRequest::new(bad, engine.language().clone());
        request.enabled_rules = vec!["MORFOLOGIK_RULE_RU_RU_YO".to_string()];
        let enabled = engine.check_with_request(&request);
        assert!(
            enabled
                .matches
                .iter()
                .any(|m| m.rule().id() == "MORFOLOGIK_RULE_RU_RU_YO"
                    && bad[m.offset()..].starts_with("каждя")),
            "expected a match on каждя, got {:?}",
            enabled
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset()))
                .collect::<Vec<_>>()
        );
    }
}
