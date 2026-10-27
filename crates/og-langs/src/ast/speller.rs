use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use og_spell::Dictionary;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MORFOLOGIK_RULE_AST: &str = "MORFOLOGIK_RULE_AST";

pub const MORFOLOGIK_AST_DESCRIPTION: &str = "Posible fallu d'ortograf\u{eda}";

const AST_SPELLING_MESSAGE: &str = "Atop\u{e1}u un posible fallu d'ortograf\u{eda}";

const KNOWN_BRAND_WORDS: &[&str] = &["LanguageTool", "LanguageTooler"];

pub(crate) fn ast_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ast/src/main/resources/org/languagetool/resource/ast"))
}

fn module_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/ast/data")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AsturianSpellerSource {
    HunspellDictionary,
    TaggerApproximation,
}

fn hunspell_word_set(dir: &Path) -> Option<Arc<og_tagger::morfologik::FsaWordSet>> {
    let dict_path = dir.join("ast_ES.dict");
    let info_path = dir.join("ast_ES.info");
    if !dict_path.exists() || !info_path.exists() {
        return None;
    }
    let words = og_tagger::morfologik::FsaWordSet::load(&dict_path, &info_path)?;
    if words.is_empty() {
        return None;
    }
    Some(Arc::new(words))
}

pub fn asturian_speller_dictionary() -> Option<(Arc<Dictionary>, AsturianSpellerSource)> {
    static CACHE: og_core::releasable::Releasable<Dictionary> =
        og_core::releasable::Releasable::new();
    let dict = og_core::releasable::shared_get_or_build(&CACHE, || {
        let words = ast_resource_dir()
            .and_then(|dir| hunspell_word_set(&dir))
            .or_else(|| hunspell_word_set(&module_data_dir()));
        let (mut dict, from_hunspell) = if let Some(words) = words {
            (Dictionary::from_fsa_word_set(words), true)
        } else {
            let resource_dir = ast_resource_dir()?;
            let tagger_dict =
                og_tagger::generic_tagger::shared_dictionary(&resource_dir, "asturian")?;
            (
                Dictionary::from_shared_words(Arc::new(
                    og_spell::morfologik_view::MorfologikView::new(tagger_dict),
                )),
                false,
            )
        };
        if let Some(resource_dir) = ast_resource_dir()
            && let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt"))
        {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        if !from_hunspell {
            eprintln!(
                "Warning: ast hunspell/ast_ES.dict not found; MORFOLOGIK_RULE_AST falls back to the asturian.dict tagger dictionary"
            );
        }
        Some(std::sync::Arc::new(dict))
    })?;
    let hunspell_on_disk = ast_resource_dir()
        .is_some_and(|dir| dir.join("ast_ES.dict").is_file() && dir.join("ast_ES.info").is_file())
        || {
            let dir = module_data_dir();
            dir.join("ast_ES.dict").is_file() && dir.join("ast_ES.info").is_file()
        };
    let source = if hunspell_on_disk {
        AsturianSpellerSource::HunspellDictionary
    } else {
        AsturianSpellerSource::TaggerApproximation
    };
    Some((dict, source))
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(resource_dir) = ast_resource_dir()
        && let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_string());
            }
        }
    }
    ignore
}

fn is_mixed_case(word: &str) -> bool {
    word.chars().skip(1).any(char::is_uppercase)
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().any(char::is_alphabetic) && !word.chars().any(char::is_lowercase)
}

fn is_url(token: &str) -> bool {
    const PROTOCOLS: &[&str] = &[
        "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
        "ssh", "data", "magnet", "smb", "slack", "spotify",
    ];
    if token.starts_with("www.") {
        return true;
    }
    PROTOCOLS
        .iter()
        .any(|p| token.starts_with(&format!("{p}://")))
}

fn is_email(token: &str) -> bool {
    let Some(at) = token.find('@') else {
        return false;
    };
    token[..at]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~.-".contains(c))
        && token[at + 1..].contains('.')
}

pub struct AsturianSpellerRule {
    inner: og_spell::SpellingCheckRule,
    source: AsturianSpellerSource,
}

impl AsturianSpellerRule {
    pub fn new() -> Self {
        let (dictionary, source) = asturian_speller_dictionary().unwrap_or_else(|| {
            (
                Arc::new(Dictionary::new()),
                AsturianSpellerSource::TaggerApproximation,
            )
        });
        let inner = og_spell::SpellingCheckRule::new_shared(
            dictionary,
            MORFOLOGIK_RULE_AST,
            HashSet::new(),
        )
        .with_ignore_words(shared_ignore_words());
        Self { inner, source }
    }

    pub fn source(&self) -> AsturianSpellerSource {
        self.source
    }

    pub fn is_known_word(&self, word: &str) -> bool {
        match self.source {
            AsturianSpellerSource::HunspellDictionary => self.is_known_word_exact(word),
            AsturianSpellerSource::TaggerApproximation => self.inner.is_known_word(word),
        }
    }

    fn is_known_word_exact(&self, word: &str) -> bool {
        if KNOWN_BRAND_WORDS.contains(&word) {
            return true;
        }
        let dict = self.inner.shared_dictionary();
        if dict.contains_exact(word) {
            return true;
        }
        if is_mixed_case(word) {
            return false;
        }
        let lowered = word.to_lowercase();
        if lowered != word && dict.contains_exact(&lowered) {
            return true;
        }
        if lowered != word && is_all_uppercase(word) {
            let mut chars = word.chars();
            let first = chars.next().unwrap_or_default();
            let initial_upper = format!("{}{}", first, chars.as_str().to_lowercase());
            if dict.contains_exact(&initial_upper) {
                return true;
            }
        }
        false
    }

    pub fn get_suggestions(&self, word: &str, max: usize) -> Vec<String> {
        self.inner.get_suggestions(word, max)
    }

    fn context(sentence: &AnalyzedSentence, start: usize, end: usize) -> RuleMatchContext {
        let full_text = sentence.text();
        let rel_start = start.saturating_sub(sentence.start());
        let rel_end = (rel_start + (end - start)).min(full_text.len());
        let mut context_start = rel_start.saturating_sub(40);
        while context_start < full_text.len() && !full_text.is_char_boundary(context_start) {
            context_start += 1;
        }
        let mut context_end = std::cmp::min(rel_end + 40, full_text.len());
        while context_end > context_start && !full_text.is_char_boundary(context_end) {
            context_end -= 1;
        }
        RuleMatchContext::new(
            full_text[context_start..context_end].to_string(),
            context_start,
            context_end - context_start,
        )
    }
}

impl Default for AsturianSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for AsturianSpellerRule {
    fn id(&self) -> &str {
        MORFOLOGIK_RULE_AST
    }

    fn description(&self) -> &str {
        MORFOLOGIK_AST_DESCRIPTION
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo").with_description("Possible spelling mistake")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        if self.source == AsturianSpellerSource::TaggerApproximation {
            return self.inner.match_sentence(sentence);
        }
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let mut is_first_word = true;
        for (idx, token_readings) in tokens.iter().enumerate() {
            let text = token_readings.token().token();
            if text.is_empty()
                || token_readings.is_sentence_start()
                || token_readings.is_immunized()
                || token_readings.is_ignored_by_speller()
                || is_url(text)
                || is_email(text)
            {
                continue;
            }
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            let is_word_first = is_first_word && idx > 0;
            if idx > 0 {
                is_first_word = false;
            }
            if self.is_known_word_exact(text) {
                continue;
            }
            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let mut suggestions: Vec<SuggestedReplacement> = self
                .get_suggestions(text, 5)
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect();
            if is_word_first && idx < tokens.len() - 1 {
                let mut capitalized: Vec<SuggestedReplacement> = Vec::new();
                for suggestion in suggestions {
                    let replacement = suggestion.value().to_string();
                    if replacement == replacement.to_lowercase() {
                        let mut chars = replacement.chars();
                        let upper = chars
                            .next()
                            .map(|c| c.to_uppercase().collect::<String>())
                            .unwrap_or_default();
                        let capped = format!("{}{}", upper, chars.as_str());
                        if !capitalized.iter().any(|s| s.value() == capped.as_str()) {
                            capitalized.push(SuggestedReplacement::new(capped));
                        }
                    } else if !capitalized.iter().any(|s| s.value() == suggestion.value()) {
                        capitalized.push(suggestion);
                    }
                }
                suggestions = capitalized;
            }
            let rule = RuleMatchRule::new(MORFOLOGIK_RULE_AST, MORFOLOGIK_AST_DESCRIPTION)
                .with_category(self.category())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    AST_SPELLING_MESSAGE.to_string(),
                    start,
                    end - start,
                    rule,
                    Self::context(sentence, start, end),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(suggestions),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        use og_tokenizer::WordTokenizer as _;
        let mut tokens = vec![og_core::AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(
            og_tokenizer::DefaultWordTokenizer::new_generic()
                .tokenize(text)
                .into_iter()
                .map(|t| {
                    og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                        t.text(),
                        t.start(),
                        t.end(),
                    ))
                }),
        );
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn speller_rule_uses_java_id_and_ast_description() {
        let rule = AsturianSpellerRule::new();
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_AST");
        assert_eq!(rule.description(), "Posible fallu d'ortograf\u{eda}");
    }

    #[test]
    fn morfologik_asturian_speller_rule_test_parity() {
        let rule = AsturianSpellerRule::new();
        assert_eq!(rule.source(), AsturianSpellerSource::HunspellDictionary);
        assert!(
            rule.match_sentence(&analyze(
                "\u{bf}Tas avez\u{e1}u colos correutores gramaticales?"
            ))
            .is_empty()
        );
        let matches = rule.match_sentence(&analyze(
            "\u{bf}Tas avec\u{e1}u colos correutores gramaticales?",
        ));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 6);
        assert_eq!(matches[0].offset() + matches[0].length(), 13);
    }

    #[test]
    fn avecadu_typo_suggests_avezu() {
        let rule = AsturianSpellerRule::new();
        let suggestions = rule.get_suggestions("avec\u{e1}u", 20);
        assert!(
            suggestions.contains(&"avez\u{e1}u".to_string()),
            "expected 'avez\u{e1}u' in {suggestions:?}"
        );
    }

    #[test]
    fn morfologik_case_folding_semantics() {
        let rule = AsturianSpellerRule::new();
        assert!(rule.is_known_word("apega"));
        assert!(rule.is_known_word("Apega"));
        assert!(rule.is_known_word("Tas"));
        assert!(!rule.is_known_word("ApegA"));
        assert!(rule.is_known_word("LanguageTool"));
        assert!(rule.is_known_word("LanguageTooler"));
        assert!(!rule.is_known_word("problemis"));
        assert!(!rule.is_known_word("deteutar"));
        assert!(!rule.is_known_word("avec\u{e1}u"));
    }

    #[test]
    fn sentence_start_word_flagged_with_capitalized_suggestion() {
        let rule = AsturianSpellerRule::new();
        let matches = rule.match_sentence(&analyze("Problemis testu equ\u{ed}."));
        assert_eq!(matches.len(), 1, "sentence-start unknown is flagged");
        assert_eq!(matches[0].offset(), 0);
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|s| s.value())
            .collect();
        assert!(
            replacements
                .iter()
                .any(|s| s.starts_with('P') && s.chars().skip(1).any(char::is_alphabetic)),
            "expected a capitalized suggestion in {replacements:?}"
        );

        let matches = rule.match_sentence(&analyze("problemis testu equ\u{ed}."));
        assert_eq!(matches.len(), 1, "{matches:?}");
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|s| s.value())
            .collect();
        assert!(
            replacements
                .iter()
                .all(|s| s.starts_with(|c: char| c.is_uppercase())),
            "lowercase suggestions at sentence start must be capitalized: {replacements:?}"
        );
        assert_eq!(replacements[0], "Problemes");
    }

    #[test]
    fn www_url_and_email_tokens_are_skipped() {
        let rule = AsturianSpellerRule::new();
        assert!(
            rule.match_sentence(&analyze("Visita www.exemplu.com g\u{fc}ei."))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&analyze(
                "Manda un corr\u{e9}u a usuari@domini.com g\u{fc}ei."
            ))
            .is_empty()
        );
    }

    #[test]
    fn an_at_token_without_a_dot_is_not_an_email() {
        let rule = AsturianSpellerRule::new();
        let matches =
            rule.match_sentence(&analyze("Manda un corr\u{e9}u a usuari@domini g\u{fc}ei."));
        assert_eq!(matches.len(), 2, "{matches:?}");
        assert_eq!(matches[0].offset(), 19);
        assert_eq!(matches[0].offset() + matches[0].length(), 25);
        assert_eq!(matches[1].offset(), 26);
        assert_eq!(matches[1].offset() + matches[1].length(), 32);
    }

    #[test]
    fn default_rule_reports_the_misspelling_issue_type() {
        let rule = AsturianSpellerRule::default();
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_AST");
        assert_eq!(rule.issue_type(), IssueType::Misspelling);
        assert_eq!(rule.category().id().as_str(), "TYPOS");
    }

    #[test]
    fn context_snips_forty_bytes_on_each_side_on_multibyte_text() {
        let rule = AsturianSpellerRule::new();
        let accented = "\u{e1}\u{e9}\u{ed}\u{f3}".repeat(5);
        let text = format!("{accented} avec\u{e1}u {accented}.");
        let matches = rule.match_sentence(&analyze(&text));
        assert_eq!(matches.len(), 3, "{matches:?}");
        let word_start = accented.len() + 1;
        let avecadu = &matches[1];
        assert_eq!(avecadu.offset(), word_start);
        assert_eq!(avecadu.length(), "avec\u{e1}u".len());
        assert_eq!(
            avecadu.context().offset(),
            2,
            "the leading window must walk forward to a char boundary"
        );
        assert_eq!(avecadu.context().text().len(), 85);
        for m in &matches {
            assert!(text.contains(m.context().text()));
            assert!(text.is_char_boundary(m.context().offset()));
            assert!(text.is_char_boundary(m.context().offset() + m.context().text().len()));
        }
    }
}
