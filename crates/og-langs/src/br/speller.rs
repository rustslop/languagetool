use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule,
};
use std::collections::HashSet;
use std::sync::Arc;

pub const MORFOLOGIK_RULE_BR_FR: &str = "MORFOLOGIK_RULE_BR_FR";

pub const MORFOLOGIK_BR_DESCRIPTION: &str = "Fazi reizhskriva\u{f1} posupl";

const BR_SPELLING_MESSAGE: &str = "Fazi reizhskriva\u{f1} posupl kavet.";

const KNOWN_BRAND_WORDS: &[&str] = &["LanguageTool", "LanguageTooler"];

struct BrWordSet {
    words: Option<og_tagger::morfologik::FsaWordSet>,
}

impl BrWordSet {
    fn contains(&self, word: &str) -> bool {
        self.words.as_ref().is_some_and(|w| w.contains(word))
    }

    fn word_count(&self) -> u64 {
        self.words.as_ref().map_or(0, |w| w.word_count())
    }
}

fn breton_word_set() -> Option<Arc<BrWordSet>> {
    static CACHE: og_core::releasable::Releasable<BrWordSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = super::tagger::br_resource_dir()?;
        let dict_path = resource_dir.join("hunspell/br_FR.dict");
        let info_path = resource_dir.join("hunspell/br_FR.info");
        if !dict_path.exists() || !info_path.exists() {
            return None;
        }
        let words = og_tagger::morfologik::FsaWordSet::load(&dict_path, &info_path)?;
        Some(Arc::new(BrWordSet { words: Some(words) }))
    })
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(resource_dir) = super::tagger::br_resource_dir()
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

fn shared_extra_words() -> HashSet<String> {
    let mut extra = HashSet::new();
    if let Some(resource_dir) = super::tagger::br_resource_dir()
        && let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                extra.insert(word.to_string());
            }
        }
    }
    extra
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

pub struct BretonSpellerRule {
    words: Arc<BrWordSet>,
    extra_words: HashSet<String>,
    ignore_words: HashSet<String>,
}

impl BretonSpellerRule {
    pub fn new() -> Self {
        Self {
            words: breton_word_set().unwrap_or_else(|| Arc::new(BrWordSet { words: None })),
            extra_words: shared_extra_words(),
            ignore_words: shared_ignore_words(),
        }
    }

    pub fn shared_arc() -> Arc<BretonSpellerRule> {
        crate::weak_shared(|| Some(Arc::new(BretonSpellerRule::new())))
            .expect("breton speller rule")
    }

    pub fn dictionary_size(&self) -> usize {
        self.words.word_count() as usize
    }

    pub fn is_known_word(&self, word: &str) -> bool {
        if KNOWN_BRAND_WORDS.contains(&word) || self.extra_words.contains(word) {
            return true;
        }
        if self.words.contains(word) {
            return true;
        }
        if is_mixed_case(word) {
            return false;
        }
        let lowered = word.to_lowercase();
        if lowered != word && self.words.contains(&lowered) {
            return true;
        }
        if lowered != word && is_all_uppercase(word) {
            let mut chars = word.chars();
            let first = chars.next().unwrap_or_default();
            let initial_upper = format!("{}{}", first, chars.as_str().to_lowercase());
            if self.words.contains(&initial_upper) {
                return true;
            }
        }
        false
    }

    fn is_tagged(token: &AnalyzedTokenReadings) -> bool {
        token
            .readings()
            .iter()
            .any(|reading| !reading.pos_tags().is_empty())
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

    fn rule_match(
        &self,
        sentence: &AnalyzedSentence,
        word: &str,
        start: usize,
        end: usize,
    ) -> Option<RuleMatch> {
        if word.is_empty() || self.is_known_word(word) {
            return None;
        }
        let rule = RuleMatchRule::new(MORFOLOGIK_RULE_BR_FR, MORFOLOGIK_BR_DESCRIPTION)
            .with_category(Category::new("TYPOS", "Possible Typo"))
            .with_issue_type(IssueType::Misspelling.as_str());
        Some(
            RuleMatch::new(
                BR_SPELLING_MESSAGE.to_string(),
                start,
                end - start,
                rule,
                Self::context(sentence, start, end),
            )
            .with_sentence(sentence.text().to_string()),
        )
    }
}

impl Default for BretonSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for BretonSpellerRule {
    fn id(&self) -> &str {
        MORFOLOGIK_RULE_BR_FR
    }

    fn description(&self) -> &str {
        MORFOLOGIK_BR_DESCRIPTION
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo").with_description("Possible spelling mistake")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    // Java keeps suggestions lazy over the 17.7M-form FSA via Morfologik ranking;
    // this port returns no suggestions for words outside the small extra list.
    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        for token_readings in tokens.iter() {
            let text = token_readings.token().token();
            if text.is_empty()
                || token_readings.is_sentence_start()
                || token_readings.is_immunized()
                || token_readings.is_ignored_by_speller()
                || self.ignore_words.contains(text)
                || is_url(text)
                || is_email(text)
                || Self::is_tagged(token_readings)
            {
                continue;
            }
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let mut index = 0usize;
            for (byte_idx, ch) in text.char_indices() {
                if ch == '-' {
                    if let Some(rule_match) = self.rule_match(
                        sentence,
                        &text[index..byte_idx],
                        start + index,
                        start + byte_idx,
                    ) {
                        matches.push(rule_match);
                    }
                    index = byte_idx + 1;
                }
            }
            if index == 0 {
                if let Some(rule_match) = self.rule_match(sentence, text, start, end) {
                    matches.push(rule_match);
                }
            } else if let Some(rule_match) =
                self.rule_match(sentence, &text[index..], start + index, end)
            {
                matches.push(rule_match);
            }
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
            super::super::word_tokenizer::BretonWordTokenizer::new()
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

    fn rule() -> Arc<BretonSpellerRule> {
        BretonSpellerRule::shared_arc()
    }

    #[test]
    fn speller_rule_uses_java_id_and_breton_description() {
        let rule = rule();
        assert_eq!(rule.id(), "MORFOLOGIK_RULE_BR_FR");
        assert_eq!(rule.description(), "Fazi reizhskriva\u{f1} posupl");
        assert!(rule.dictionary_size() > 1_000_000);
    }

    #[test]
    fn java_pin_correct_sentences_stay_quiet() {
        let rule = rule();
        for text in [
            "Penaos ema\u{f1} kont ganit?",
            "C'hwerc'h merc'h gwerc'h war c'hwerc'h marc'h kalloc'h",
            "C\u{2019}hwerc\u{2019}h merc\u{2019}h gwerc\u{2018}h war c\u{2018}hwerc\u{2018}h marc'h kalloc\u{2018}h",
            "Evel-just",
            "Barrek-tre eo LanguageTool",
            "LanguageTool!",
            ",",
            "123454",
            "a",
        ] {
            assert!(
                rule.match_sentence(&analyze(text)).is_empty(),
                "'{text}' must stay unflagged"
            );
        }
    }

    #[test]
    fn java_pin_evel_juste_flags_the_hyphen_part() {
        let rule = rule();
        let matches = rule.match_sentence(&analyze("Evel-juste"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 5);
        assert_eq!(matches[0].offset() + matches[0].length(), 10);
        assert_eq!(matches[0].message(), "Fazi reizhskriva\u{f1} posupl kavet.");
    }

    #[test]
    fn java_pin_c_hreizhig_don_flags_the_first_part() {
        let rule = rule();
        let matches = rule.match_sentence(&analyze("C\u{2019}hreizhig-don"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(
            matches[0].offset() + matches[0].length(),
            "C\u{2019}hreizhig".len()
        );
    }

    #[test]
    fn java_pin_aoh_is_flagged() {
        let rule = rule();
        assert_eq!(rule.match_sentence(&analyze("a\u{f5}h")).len(), 1);
    }

    #[test]
    fn case_folding_follows_the_java_speller() {
        let rule = rule();
        assert!(rule.is_known_word("evel"));
        assert!(rule.is_known_word("Evel"));
        assert!(!rule.is_known_word("EveL"));
        assert!(rule.is_known_word("Record"));
        assert!(!rule.is_known_word("juste"));
    }
}
