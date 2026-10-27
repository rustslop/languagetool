pub mod speller;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

pub use speller::{
    MORFOLOGIK_RULE_ML_IN, MalayalamSpellerRule, MalayalamSpellerSource,
    malayalam_speller_dictionary,
};
pub use word_tokenizer::MalayalamWordTokenizer;

pub fn ml_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ml/src/main/resources/org/languagetool/resource/ml"))
}

pub fn malayalam_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        ml_resource_dir().map(|dir| {
            Arc::new(og_tagger::GenericTagger::from_resource_dir(
                &dir,
                "malayalam",
            ))
        })
    })
}

pub fn malayalam_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "'"],
        vec!["]", ")", "}", "\"", "'"],
    )
}

pub struct MalayalamNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn malayalam_native_rules() -> MalayalamNativeRules {
    MalayalamNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(MalayalamSpellerRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(malayalam_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = malayalam_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "MORFOLOGIK_RULE_ML_IN",
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
            ],
            "Malayalam.java:77-87 getRelevantRules order: CommaWhitespaceRule, \
             DoublePunctuationRule, GenericUnpairedBracketsRule (text-level here), \
             MorfologikMalayalamSpellerRule, UppercaseSentenceStartRule, WordRepeatRule, \
             MultipleWhitespaceRule (text-level here)"
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn tagger_loads_the_malayalam_pos_dictionary() {
        let tagger = malayalam_tagger().expect("malayalam.dict tagger");
        assert!(
            tagger.has_dictionary(),
            "malayalam.dict + malayalam.info must load (BaseTagger parity, MalayalamTagger.java:31-33)"
        );
    }
}
