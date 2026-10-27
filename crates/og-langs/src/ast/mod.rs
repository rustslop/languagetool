pub mod speller;

#[cfg(test)]
mod example_suite;

use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub use speller::{
    AsturianSpellerRule, AsturianSpellerSource, MORFOLOGIK_AST_DESCRIPTION, MORFOLOGIK_RULE_AST,
    asturian_speller_dictionary,
};

pub struct AsturianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn asturian_native_rules() -> AsturianNativeRules {
    AsturianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(speller::AsturianSpellerRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::GenericUnpairedBracketsRule::new()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asturian_natives_use_java_ids_in_relevant_rules_order() {
        let natives = asturian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "MORFOLOGIK_RULE_AST",
                "UPPERCASE_SENTENCE_START",
            ]
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }
}
