use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use og_spell::Dictionary;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MORFOLOGIK_RULE_ML_IN: &str = "MORFOLOGIK_RULE_ML_IN";

pub const MORFOLOGIK_ML_DESCRIPTION: &str = "Possible spelling mistake";

const ML_SPELLING_MESSAGE: &str = "Possible spelling mistake found";

const KNOWN_BRAND_WORDS: &[&str] = &["LanguageTool", "LanguageTooler"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MalayalamSpellerSource {
    HunspellDictionary,
    TaggerApproximation,
}

pub(crate) fn hunspell_dir() -> Option<PathBuf> {
    super::ml_resource_dir().map(|dir| dir.join("hunspell"))
}

fn hunspell_word_set(dir: &Path) -> Option<Arc<og_tagger::morfologik::FsaWordSet>> {
    let dict_path = dir.join("ml_IN.dict");
    let info_path = dir.join("ml_IN.info");
    if !dict_path.exists() || !info_path.exists() {
        return None;
    }
    let words = og_tagger::morfologik::FsaWordSet::load(&dict_path, &info_path)?;
    if words.is_empty() {
        return None;
    }
    Some(Arc::new(words))
}

pub fn malayalam_speller_dictionary() -> Option<(Arc<Dictionary>, MalayalamSpellerSource)> {
    static CACHE: og_core::releasable::Releasable<Dictionary> =
        og_core::releasable::Releasable::new();
    let dict = og_core::releasable::shared_get_or_build(&CACHE, || {
        let mut have_words = false;
        let mut dict = match hunspell_dir().and_then(|dir| hunspell_word_set(&dir)) {
            Some(words) => {
                have_words = true;
                Dictionary::from_fsa_word_set(words)
            }
            None => Dictionary::new(),
        };
        if let Some(dir) = super::ml_resource_dir()
            && let Ok(extra) = std::fs::read_to_string(dir.join("hunspell/spelling.txt"))
        {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                    have_words = true;
                }
            }
        }
        if have_words {
            return Some(std::sync::Arc::new(dict));
        }
        let resource_dir = super::ml_resource_dir()?;
        let tagger_dict = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "malayalam")?;
        eprintln!(
            "Warning: ml hunspell/ml_IN.dict not usable; MORFOLOGIK_RULE_ML_IN falls back to the malayalam.dict tagger dictionary"
        );
        Some(std::sync::Arc::new(Dictionary::from_shared_words(
            Arc::new(og_spell::morfologik_view::MorfologikView::new(tagger_dict)),
        )))
    })?;
    let source = if hunspell_dir()
        .is_some_and(|dir| dir.join("ml_IN.dict").is_file() && dir.join("ml_IN.info").is_file())
    {
        MalayalamSpellerSource::HunspellDictionary
    } else {
        MalayalamSpellerSource::TaggerApproximation
    };
    Some((dict, source))
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(dir) = hunspell_dir()
        && let Ok(words) = std::fs::read_to_string(dir.join("ignore.txt"))
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

pub struct MalayalamSpellerRule {
    inner: og_spell::SpellingCheckRule,
    source: MalayalamSpellerSource,
    ignore_words: HashSet<String>,
}

impl MalayalamSpellerRule {
    pub fn new() -> Self {
        let (dictionary, source) = malayalam_speller_dictionary().unwrap_or_else(|| {
            (
                Arc::new(Dictionary::new()),
                MalayalamSpellerSource::TaggerApproximation,
            )
        });
        let ignore_words = shared_ignore_words();
        let inner = og_spell::SpellingCheckRule::new_shared(
            dictionary,
            MORFOLOGIK_RULE_ML_IN,
            HashSet::new(),
        )
        .with_ignore_words(ignore_words.clone());
        Self {
            inner,
            source,
            ignore_words,
        }
    }

    pub fn source(&self) -> MalayalamSpellerSource {
        self.source
    }

    pub fn is_known_word(&self, word: &str) -> bool {
        KNOWN_BRAND_WORDS.contains(&word)
            || self.ignore_words.contains(word)
            || self.inner.shared_dictionary().contains_exact(word)
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

impl Default for MalayalamSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for MalayalamSpellerRule {
    fn id(&self) -> &str {
        MORFOLOGIK_RULE_ML_IN
    }

    fn description(&self) -> &str {
        MORFOLOGIK_ML_DESCRIPTION
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo").with_description(MORFOLOGIK_ML_DESCRIPTION)
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        if self.source == MalayalamSpellerSource::TaggerApproximation {
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
            if self.is_known_word(text) {
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
            let rule = RuleMatchRule::new(MORFOLOGIK_RULE_ML_IN, MORFOLOGIK_ML_DESCRIPTION)
                .with_category(self.category())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    format!("{ML_SPELLING_MESSAGE}: '{text}'"),
                    start,
                    end - start,
                    rule,
                    Self::context(sentence, start, end),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(suggestions)
                .with_match_type(og_core::RuleMatchKind::UnknownWord),
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
            super::super::MalayalamWordTokenizer::new()
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
    fn speller_rule_uses_java_id_and_root_bundle_description() {
        let rule = MalayalamSpellerRule::new();
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_ML_IN");
        assert_eq!(rule.description(), "Possible spelling mistake");
    }

    #[test]
    fn speller_loads_the_hunspell_dictionary() {
        let rule = MalayalamSpellerRule::new();
        assert_eq!(
            rule.source(),
            MalayalamSpellerSource::HunspellDictionary,
            "hunspell/ml_IN.dict + ml_IN.info must drive the speller"
        );
    }

    #[test]
    fn morfologik_malayalam_speller_rule_test_parity() {
        let rule = MalayalamSpellerRule::new();
        for text in [
            "എന്തുകൊണ്ട്\u{200c} അംഗത്വം",
            "എങ്ങനെ അംഗമാകാം?",
            "LanguageTool",
            ",",
            "123454",
        ] {
            assert!(
                rule.match_sentence(&analyze(text)).is_empty(),
                "expected no match for '{text}'"
            );
        }
        let matches = rule.match_sentence(&analyze("Aagohw"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].offset() + matches[0].length(), "Aagohw".len());
        let mixed = "എaങ്ങനെ";
        let matches = rule.match_sentence(&analyze(mixed));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(
            matches[0].offset() + matches[0].length(),
            mixed.len(),
            "Java's toPos 7 is UTF-16 units; this port's offsets are bytes"
        );
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            replacements.contains(&"എങ്ങനെ"),
            "Java expects 'എങ്ങനെ' as a suggestion, got {replacements:?}"
        );
        assert_eq!(rule.match_sentence(&analyze("a\u{f5}h")).len(), 1);
        assert_eq!(rule.match_sentence(&analyze("a")).len(), 1);
    }

    #[test]
    fn ignore_txt_words_are_never_flagged() {
        let rule = MalayalamSpellerRule::new();
        for word in ["മീരയെ", "രാജനോട്"] {
            assert!(rule.is_known_word(word), "ignore.txt covers '{word}'");
        }
    }

    fn built_sentence(text: &str, words: &[&str]) -> AnalyzedSentence {
        let mut tokens = vec![og_core::AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        let mut parts = Vec::new();
        for word in words {
            parts.push((*word, offset, offset + word.len()));
            offset += word.len();
        }
        for (surface, start, end) in parts {
            tokens.push(og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new(surface, start, end),
            ));
        }
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn sentence_start_mixed_word_keeps_lowercase_suggestions() {
        let rule = MalayalamSpellerRule::new();
        let matches = rule.match_sentence(&analyze("എaങ്ങനെ ഒരു പരീക്ഷണം."));
        assert_eq!(matches.len(), 1, "{matches:?}");
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|s| s.value())
            .collect();
        assert_eq!(
            replacements,
            vec!["എങ്ങനെ", "അങ്ങനെ", "അതങ്ങനെ", "ഇങ്ങനെ", "എങ്ങിനെ"],
            "the dictionary's similar-word order must stay pinned"
        );
    }

    #[test]
    fn url_and_email_tokens_are_skipped_but_bare_at_is_flagged() {
        let rule = MalayalamSpellerRule::new();
        let url_text = "ഇവിടെ http://exemple.com ഒരു വാക്ക്.";
        let url_sentence = built_sentence(
            url_text,
            &[
                "ഇവിടെ",
                " ",
                "http://exemple.com",
                " ",
                "ഒരു",
                " ",
                "വാക്ക്",
                ".",
            ],
        );
        assert!(
            rule.match_sentence(&url_sentence).is_empty(),
            "a http:// URL token must never be flagged"
        );

        let www_sentence = built_sentence("www.exemple.com", &["www.exemple.com"]);
        assert!(
            rule.match_sentence(&www_sentence).is_empty(),
            "a www. URL token must never be flagged"
        );

        let email_text = "വാക്ക് a@b.com ഇവിടെ.";
        let email_sentence =
            built_sentence(email_text, &["വാക്ക്", " ", "a@b.com", " ", "ഇവിടെ", "."]);
        assert!(
            rule.match_sentence(&email_sentence).is_empty(),
            "an email token must never be flagged"
        );

        let bare_text = "വാക്ക് qqzzxxy@bb ഇവിടെ.";
        let bare_sentence =
            built_sentence(bare_text, &["വാക്ക്", " ", "qqzzxxy@bb", " ", "ഇവിടെ", "."]);
        let matches = rule.match_sentence(&bare_sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(
            matches[0].offset(),
            "വാക്ക് ".len(),
            "a bare user@host with no dot is not an email"
        );
    }

    #[test]
    fn default_rule_reports_the_misspelling_issue_type() {
        let rule = MalayalamSpellerRule::default();
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_ML_IN");
        assert_eq!(rule.issue_type(), IssueType::Misspelling);
        assert_eq!(rule.category().id().as_str(), "TYPOS");
    }

    #[test]
    fn context_snips_forty_bytes_on_each_side_on_multibyte_text() {
        let rule = MalayalamSpellerRule::new();
        let prefix = "അതിന്റെ ഒരു നീണ്ട ബാക്കി ഭാഗം ഇവിടെ ഉണ്ട് ";
        let suffix = "അതിന്റെ ഒരു നീണ്ട ബാക്കി ഭാഗം ഇവിടെ ഉണ്ട്.";
        let text = format!("{prefix}എaങ്ങനെ {suffix}");
        let matches = rule.match_sentence(&analyze(&text));
        assert_eq!(matches.len(), 1, "{matches:?}");
        let word_start = prefix.len();
        let word = "എaങ്ങനെ";
        assert_eq!(matches[0].offset(), word_start);
        assert_eq!(matches[0].length(), word.len());
        let context = matches[0].context().text();
        assert!(
            text.contains(context),
            "context must come from the sentence"
        );
        let context_offset = matches[0].context().offset();
        assert!(
            text.is_char_boundary(context_offset),
            "the leading byte window must stop on a char boundary"
        );
        assert!(
            text.is_char_boundary(context_offset + context.len()),
            "the trailing byte window must stop on a char boundary"
        );
    }
}
