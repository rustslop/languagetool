use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub mod speller;

#[cfg(test)]
mod example_suite;

pub use speller::{HUNSPELL_NO_SUGGEST_RULE, IcelandicSpellerRule};

pub fn icelandic_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "'"],
        vec!["]", ")", "}", "\"", "'"],
    )
}

pub struct IcelandicNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn icelandic_native_rules() -> IcelandicNativeRules {
    IcelandicNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(IcelandicSpellerRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(icelandic_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = icelandic_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "HUNSPELL_NO_SUGGEST_RULE",
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
            ],
            "Icelandic.java:74-82 getRelevantRules order: CommaWhitespace, DoublePunctuation, \
             GenericUnpairedBrackets (text-level here), HunspellNoSuggestion, \
             UppercaseSentenceStart, WordRepeat, MultipleWhitespace (text-level here)"
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn unpaired_brackets_cover_the_java_default_symbol_pairs() {
        let text = "Hann sagði \"þetta er gott.";
        let sentences = crate::is::example_suite::is_pipeline().analyze_text(text);
        let matches = icelandic_unpaired_brackets_rule().match_text(text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "the plain GenericUnpairedBracketsRule::new() tracks only [ ( {{, but Java's default \
             constructor also pairs \" and ' (GenericUnpairedBracketsRule.java:93)"
        );
    }
}
