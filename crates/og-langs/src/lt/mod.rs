use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod example_suite;

pub const MORFOLOGIK_RULE_LT_LT: &str = "MORFOLOGIK_RULE_LT_LT";

const JAVA_ASCII_PUNCT_CLASS: &str = "[!-/:-@\\[-`{-~]";

pub fn normalize_lt_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    if !xml.contains("\\p{Punct}") {
        return std::borrow::Cow::Borrowed(xml);
    }
    // Java's \p{Punct} is ASCII-only; Rust regex's is Unicode P*, which would
    // also swallow the grammar's explicit – and — branches.
    std::borrow::Cow::Owned(xml.replace("\\p{Punct}", JAVA_ASCII_PUNCT_CLASS))
}

pub fn lt_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("lt/src/main/resources/org/languagetool/rules/lt"))
}

pub fn lt_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("lt/src/main/resources/org/languagetool/resource/lt"))
}

pub fn lithuanian_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "'"],
        vec!["]", ")", "}", "\"", "'"],
    )
}

pub struct LithuanianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn lithuanian_native_rules() -> LithuanianNativeRules {
    LithuanianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(lithuanian_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = lithuanian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "UPPERCASE_SENTENCE_START",
            ],
            "Lithuanian.java:72-81 getRelevantRules order: CommaWhitespace, DoublePunctuation, \
             GenericUnpairedBrackets (text-level here), MorfologikLithuanianSpellerRule \
             (data-blocked, see speller test), UppercaseSentenceStart, MultipleWhitespace \
             (text-level here)"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn unpaired_brackets_cover_the_java_default_symbol_pairs() {
        let text = "Jis pasakė \"tai yra gerai.";
        let sentences = crate::lt::example_suite::lt_pipeline().analyze_text(text);
        let matches = lithuanian_unpaired_brackets_rule().match_text(text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "Lithuanian.java:76 uses the default GenericUnpairedBracketsRule constructor, which \
             pairs [ ( {{ \" ' (GenericUnpairedBracketsRule.java:92-94)"
        );
    }

    #[test]
    fn speller_rule_stays_data_blocked_until_an_lt_dict_lands() {
        let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
        let module_root = lt_root.join("lt/src/main/resources/org/languagetool");
        let dict_files: Vec<String> = std::fs::read_dir(&module_root)
            .into_iter()
            .flatten()
            .chain(
                std::fs::read_dir(module_root.join("resource/lt"))
                    .into_iter()
                    .flatten(),
            )
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".dict"))
            .collect();
        assert!(
            dict_files.is_empty(),
            "the LT checkout shipped no lt_LT.dict (MorfologikLithuanianSpellerRule wants \
             /lt/hunspell/lt_LT.dict and Java throws 'Cannot find dictionary file' at match time); \
             landing one must unblock the speller, found {dict_files:?}"
        );
        let natives = lithuanian_native_rules();
        let speller_ids: Vec<&str> = natives
            .rules
            .iter()
            .map(|r| r.id())
            .filter(|id| *id == MORFOLOGIK_RULE_LT_LT)
            .collect();
        assert!(
            speller_ids.is_empty(),
            "with no dictionary data the lt module must install no speller rule"
        );
    }

    #[test]
    fn normalize_rewrites_the_java_ascii_punct_class() {
        assert_eq!(
            normalize_lt_rules_xml("<token>a</token>"),
            "<token>a</token>"
        );
        let normalized =
            normalize_lt_rules_xml(r#"<token negate="yes" regexp="yes">\p{Punct}|–|—</token>"#);
        assert_eq!(
            normalized,
            r#"<token negate="yes" regexp="yes">[!-/:-@\[-`{-~]|–|—</token>"#
        );
        let ascii = regex::Regex::new(JAVA_ASCII_PUNCT_CLASS).expect("ascii punct class");
        for hit in [",", ".", "!", ":", "@", "[", "`", "{", "~"] {
            assert!(ascii.is_match(hit), "{hit} must match Java's \\p{{Punct}}");
        }
        for miss in ["–", "—", "„", "“", "a", "ą", "1", " "] {
            assert!(
                !ascii.is_match(miss),
                "{miss} must not match Java's \\p{{Punct}}"
            );
        }
    }
}
