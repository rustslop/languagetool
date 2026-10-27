use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod example_suite;
pub mod word_tokenizer;

pub use word_tokenizer::TamilWordTokenizer;

pub fn ta_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ta/src/main/resources/org/languagetool/resource/ta"))
}

pub fn ta_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ta/src/main/resources/org/languagetool/rules/ta"))
}

pub fn tamil_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        ta_resource_dir()
            .map(|dir| Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "tamil")))
    })
}

pub struct TamilNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn tamil_native_rules() -> TamilNativeRules {
    TamilNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = tamil_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec!["COMMA_PARENTHESIS_WHITESPACE", "DOUBLE_PUNCTUATION"],
            "Tamil.java:66-74 getRelevantRules starts with CommaWhitespaceRule and \
             DoublePunctuationRule; there is no speller, no UppercaseSentenceStart and no \
             unpaired-brackets rule because Tamil carries no hunspell data and no case system"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_ids,
            vec![
                "WHITESPACE_RULE",
                "TOO_LONG_SENTENCE",
                "SENTENCE_WHITESPACE"
            ],
            "Tamil.java:70-72 continues with MultipleWhitespaceRule, LongSentenceRule(50) and \
             SentenceWhitespaceRule, all TextLevelRule in Java (MultipleWhitespaceRule.java:37, \
             LongSentenceRule.java:36, SentenceWhitespaceRule.java:43)"
        );
    }

    #[test]
    fn long_sentence_rule_uses_the_java_word_limit() {
        let natives = tamil_native_rules();
        let long = natives
            .text_level_rules
            .iter()
            .find(|r| r.id() == "TOO_LONG_SENTENCE")
            .expect("LongSentenceRule present");
        let text = "வார்த்தை ".repeat(51);
        let sentences = example_suite::ta_pipeline().analyze_text(text.as_str());
        assert!(
            !long.match_text(&text, &sentences).is_empty(),
            "Tamil.java:71 constructs LongSentenceRule with 50 words, so a 51-word sentence flags"
        );
        let text = "வார்த்தை ".repeat(50);
        let sentences = example_suite::ta_pipeline().analyze_text(text.as_str());
        assert!(
            !long.match_text(&text, &sentences).is_empty(),
            "the 50-word sentence flags too: the shared LongSentenceRule counts the '<S>' \
             SENT_START pseudo token as a word ('S' is alphabetic) where Java's isWordCount skips \
             it because '<' is not a word character (LongSentenceRule.java:74-79, +1 divergence \
             that belongs to og-rules, not this module)"
        );
        let text = "வார்த்தை ".repeat(49);
        let sentences = example_suite::ta_pipeline().analyze_text(text.as_str());
        assert!(
            long.match_text(&text, &sentences).is_empty(),
            "a 49-word sentence (50 counted tokens incl. <S>) stays unflagged"
        );
    }

    #[test]
    fn tagger_loads_the_tamil_pos_dictionary() {
        let tagger = tamil_tagger().expect("tamil.dict tagger");
        assert!(
            tagger.has_dictionary(),
            "tamil.dict + tamil.info must load (BaseTagger parity, TamilTagger.java:33-35)"
        );
    }
}
