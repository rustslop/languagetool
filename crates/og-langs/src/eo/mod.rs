use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub mod speller;

#[cfg(test)]
mod example_suite;

pub use speller::{EsperantoSpellerRule, HUNSPELL_RULE_EO};

pub struct EsperantoNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn esperanto_native_rules() -> EsperantoNativeRules {
    EsperantoNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(EsperantoSpellerRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::GenericUnpairedBracketsRule::new()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = esperanto_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "HUNSPELL_RULE",
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
            ],
            "Esperanto.java getRelevantRules order (Esperanto.java:81-92): CommaWhitespace, \
             DoublePunctuation, GenericUnpairedBrackets (text-level here), HunspellRule, \
             UppercaseSentenceStart, WordRepeat"
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_level_ids,
            vec![
                "UNPAIRED_BRACKETS",
                "WHITESPACE_RULE",
                "SENTENCE_WHITESPACE"
            ]
        );
    }

    #[test]
    fn word_repeat_rule_flags_the_java_demo_repetition() {
        let pipeline = crate::eo::example_suite::eo_pipeline();
        let text = "\u{108}u vi vi rimarkis, ke estas gramatikaj eraro en tiu frazo?";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        let rule = og_rules::native_rules::WordRepeatRule::new();
        let matches = rule.match_sentence(&sentence);
        let spans: Vec<&str> = matches
            .iter()
            .map(|m| &sentence.text()[m.offset()..m.offset() + m.length()])
            .collect();
        assert_eq!(spans, vec!["vi"], "the second 'vi' of 'vi vi' is flagged");
    }
}
