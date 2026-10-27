use og_core::rule::{Rule, TextLevelRule};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod example_suite;
pub mod word_tokenizer;

pub use word_tokenizer::TagalogWordTokenizer;

pub const MORFOLOGIK_RULE_TL: &str = "MORFOLOGIK_RULE_TL";

const JAVA_BRAND_WORDS: &[&str] = &["LanguageTool", "LanguageTooler"];

pub fn tl_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("tl/src/main/resources/org/languagetool/resource/tl"))
}

pub fn tl_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("tl/src/main/resources/org/languagetool/rules/tl"))
}

pub fn tagalog_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        tl_resource_dir()
            .map(|dir| Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "tagalog")))
    })
}

fn tagalog_speller_dictionary() -> Option<Arc<og_spell::Dictionary>> {
    static CACHE: og_core::releasable::Releasable<og_spell::Dictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let hunspell = tl_resource_dir()?.join("hunspell");
        let mut dict = og_tagger::morfologik::FsaWordSet::load(
            &hunspell.join("tl_PH.dict"),
            &hunspell.join("tl_PH.info"),
        )
        .map(std::sync::Arc::new)
        .map(og_spell::Dictionary::from_fsa_word_set)
        .unwrap_or_else(og_spell::Dictionary::new);
        if let Ok(extra) = std::fs::read_to_string(hunspell.join("spelling.txt")) {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        (!dict.is_empty()).then_some(Arc::new(dict))
    })
}

fn tagalog_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(dir) = tl_resource_dir()
        && let Ok(words) = std::fs::read_to_string(dir.join("hunspell/ignore.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_lowercase());
            }
        }
    }
    ignore
}

pub fn tagalog_speller_rule() -> og_spell::SpellingCheckRule {
    let dictionary =
        tagalog_speller_dictionary().unwrap_or_else(|| Arc::new(og_spell::Dictionary::new()));
    let extra_words = JAVA_BRAND_WORDS
        .iter()
        .map(|word| (*word).to_string())
        .collect();
    og_spell::SpellingCheckRule::new_shared(dictionary, MORFOLOGIK_RULE_TL, extra_words)
        .with_ignore_words(tagalog_ignore_words())
}

pub fn tagalog_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "'"],
        vec!["]", ")", "}", "\"", "'"],
    )
}

pub struct TagalogNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn tagalog_native_rules() -> TagalogNativeRules {
    TagalogNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(tagalog_speller_rule()),
        ],
        text_level_rules: vec![
            Arc::new(tagalog_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = tagalog_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "UPPERCASE_SENTENCE_START",
                MORFOLOGIK_RULE_TL,
            ],
            "Tagalog.java:84-94 getRelevantRules order: CommaWhitespaceRule, \
             DoublePunctuationRule, GenericUnpairedBracketsRule (text-level here), \
             UppercaseSentenceStartRule, MultipleWhitespaceRule (text-level here), \
             MorfologikTagalogSpellerRule"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn tagger_loads_the_tagalog_pos_dictionary() {
        let tagger = tagalog_tagger().expect("tagalog.dict tagger");
        assert!(
            tagger.has_dictionary(),
            "tagalog.dict + tagalog.info must load (BaseTagger parity, TagalogTagger.java:31-33)"
        );
    }

    #[test]
    fn speller_rule_carries_the_java_id_brand_and_ignore_words() {
        let rule = tagalog_speller_rule();
        assert_eq!(rule.id(), MORFOLOGIK_RULE_TL);
        assert!(
            rule.is_known_word("LanguageTool"),
            "MorfologikSpeller.java:98-100 never treats LanguageTool as misspelled"
        );
        assert!(rule.is_known_word("LanguageTooler"));
        assert!(rule.is_known_word("talatang"), "tl_PH.dict word");
        assert!(
            rule.is_known_word("Maganda"),
            "case-insensitive via the lowercase form"
        );
        assert!(
            !rule.is_known_word("hindii"),
            "the doubled-vowel typo of the TagalogTest demo text is unknown"
        );
        assert!(
            rule.is_known_word("iskor"),
            "hunspell/ignore.txt entry - ignored by the Java spellchecker too"
        );
    }

    #[test]
    fn unpaired_brackets_carry_the_java_default_five_pairs() {
        use og_core::rule::TextLevelRule as _;
        let rule = tagalog_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        let text = "Ang (pusong hindi sarado.";
        let sentences = crate::tl::example_suite::tl_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "("),
            "the unclosed parenthesis must flag: {matches:?}"
        );
        let text = "Ang (pusong) sarado.";
        let sentences = crate::tl::example_suite::tl_pipeline().analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn grammar_rules_dir_resolves_the_java_layout() {
        let rules_dir = tl_rules_dir().expect("tl rules dir");
        assert!(rules_dir.join("grammar.xml").is_file());
        assert!(
            !rules_dir.join("style.xml").exists(),
            "the tl module ships grammar.xml only (Tagalog.java adds no extra rule files)"
        );
    }
}
