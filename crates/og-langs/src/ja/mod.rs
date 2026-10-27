use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub mod lexicon;
pub mod tagger;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

pub use tagger::JapaneseTagger;
pub use word_tokenizer::JapaneseWordTokenizer;

pub struct JapaneseNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn japanese_native_rules() -> JapaneseNativeRules {
    JapaneseNativeRules {
        rules: vec![Arc::new(
            og_rules::native_rules::DoublePunctuationRule::new().with_messages(
                "連続する２つのドット",
                "連続する２つのドット",
                "連続する２つのコンマ",
                "連続する２つのコンマ",
            ),
        )],
        text_level_rules: vec![Arc::new(
            og_rules::text_level_rules::MultipleWhitespaceRule::new(),
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = japanese_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(ids, vec!["DOUBLE_PUNCTUATION"]);
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["WHITESPACE_RULE"]);
    }
}
