pub mod speller;

#[cfg(test)]
mod example_suite;

use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

pub use speller::{MORFOLOGIK_RULE_SL_SI, slovenian_speller_dictionary, slovenian_speller_rule};

pub fn sl_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("sl/src/main/resources/org/languagetool/resource/sl"))
}

pub fn sl_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("sl/src/main/resources/org/languagetool/rules/sl"))
}

pub fn slovenian_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\u{201e}", "\u{bb}", "\u{ab}", "\""],
        vec!["]", ")", "}", "\u{201c}", "\u{ab}", "\u{bb}", "\""],
    )
}

pub struct SlovenianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn slovenian_native_rules() -> SlovenianNativeRules {
    SlovenianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(slovenian_speller_rule()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(slovenian_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = slovenian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                MORFOLOGIK_RULE_SL_SI,
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
            ],
            "Slovenian.java:65-76 getRelevantRules order: CommaWhitespaceRule, \
             DoublePunctuationRule, GenericUnpairedBracketsRule (text-level here), \
             MorfologikSlovenianSpellerRule, UppercaseSentenceStartRule, WordRepeatRule, \
             MultipleWhitespaceRule (text-level here)"
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn the_module_shares_the_default_tagger_null_model() {
        let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
        let resource_dir = lt_root.join("sl/src/main/resources/org/languagetool/resource/sl");
        let dict_files: Vec<String> = std::fs::read_dir(&resource_dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".dict") && !name.contains('/'))
            .collect();
        assert!(
            dict_files.is_empty(),
            "the sl module ships no POS .dict (Slovenian.java overrides no tagger, so Java falls \
             back to the null-tagging DemoTagger; the only dictionary data is \
             hunspell/sl_SI.dict, which drives the speller), found {dict_files:?}"
        );
    }

    #[test]
    fn unpaired_brackets_cover_the_java_symbol_pairs() {
        let rule = slovenian_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        let text = "To je stavek z \u{201e}odprtim narekovajem.";
        let sentences = crate::sl::example_suite::sl_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "\u{201e}"),
            "the unclosed Slovenian low quote must flag: {matches:?}"
        );
        let text = "To je stavek z \u{201e}zaprtim\u{201c} narekovajem.";
        let sentences = crate::sl::example_suite::sl_pipeline().analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn grammar_rules_dir_resolves_the_java_layout() {
        let rules_dir = sl_rules_dir().expect("sl rules dir");
        assert!(rules_dir.join("grammar.xml").is_file());
        assert!(
            !rules_dir.join("style.xml").exists(),
            "the sl module ships grammar.xml only (Slovenian.java adds no extra rule files)"
        );
    }
}
