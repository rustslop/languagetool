pub mod natives;
pub mod simple_replace;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

use og_core::rule::{Rule, TextLevelRule};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use natives::{
    UkrainianCommaWhitespaceRule, UkrainianHiddenCharacterRule, UkrainianMixedAlphabetsRule,
    UkrainianTypographyRule, UkrainianUppercaseSentenceStartRule, UkrainianWordRepeatRule,
};
pub use simple_replace::{UkrainianSimpleReplaceRenamedRule, UkrainianSimpleReplaceRule};
pub use word_tokenizer::UkrainianWordTokenizer;

pub const UK_RULE_FILES: [&str; 5] = [
    "grammar-spelling.xml",
    "grammar-grammar.xml",
    "grammar-barbarism.xml",
    "grammar-style.xml",
    "grammar-punctuation.xml",
];

pub const MORFOLOGIK_RULE_UK_UA: &str = "MORFOLOGIK_RULE_UK_UA";

pub fn uk_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("uk/src/main/resources/org/languagetool/resource/uk"))
}

pub fn uk_extra_rule_paths(lt_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for file in UK_RULE_FILES {
        let path = lt_root.join(format!(
            "uk/src/main/resources/org/languagetool/rules/uk/{file}"
        ));
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

pub fn normalize_uk_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    if !xml.contains("<!--") {
        return std::borrow::Cow::Borrowed(xml);
    }
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

pub struct UkrainianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn ukrainian_native_rules() -> UkrainianNativeRules {
    UkrainianNativeRules {
        rules: vec![
            Arc::new(UkrainianCommaWhitespaceRule::new()),
            Arc::new(UkrainianWordRepeatRule::new()),
            Arc::new(UkrainianTypographyRule::new()),
            Arc::new(UkrainianHiddenCharacterRule::new()),
            Arc::new(UkrainianMixedAlphabetsRule::new()),
            Arc::new(UkrainianSimpleReplaceRule::soft()),
            Arc::new(UkrainianSimpleReplaceRenamedRule::new()),
            Arc::new(UkrainianSimpleReplaceRule::plain()),
        ],
        text_level_rules: vec![
            Arc::new(UkrainianUppercaseSentenceStartRule::new()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = ukrainian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "UKRAINIAN_WORD_REPEAT_RULE",
                "DASH",
                "UK_HIDDEN_CHARS",
                "UK_MIXED_ALPHABETS",
                "UK_SIMPLE_REPLACE_SOFT",
                "UK_SIMPLE_REPLACE_RENAMED",
                "UK_SIMPLE_REPLACE",
            ],
            "Ukrainian.java:138-179 getRelevantRules order, minus the dictionary-gated \
             entries: MORFOLOGIK_RULE_UK_UA (needs hunspell/uk_UA.dict from the external \
             ua.net.nlp:morfologik-ukrainian-lt artifact, absent in this tree), \
             MissingHyphenRule and the five TokenAgreement* rules (all of them read \
             UkrainianTagger readings)"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_ids,
            vec!["UPPERCASE_SENTENCE_START", "WHITESPACE_RULE"],
            "UkrainianUppercaseSentenceStartRule is a TextLevelRule in Java \
             (UppercaseSentenceStartRule extends TextLevelRule); MultipleWhitespaceRule \
             sits between it and UkrainianWordRepeatRule in getRelevantRules, both text-level"
        );
    }

    #[test]
    fn extra_rule_paths_cover_the_java_file_list() {
        let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
        let extra = uk_extra_rule_paths(&lt_root);
        let names: Vec<String> = extra
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "grammar-spelling.xml",
                "grammar-grammar.xml",
                "grammar-barbarism.xml",
                "grammar-style.xml",
                "grammar-punctuation.xml",
            ],
            "Ukrainian.java:44-50 RULE_FILES; grammar.xml itself loads through the generic \
             grammar.xml loop and holds only unification/phrases blocks"
        );
    }
}
