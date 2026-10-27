use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

pub mod tagger;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

pub use tagger::ChineseTagger;
pub use word_tokenizer::ChineseWordTokenizer;

pub fn zh_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("zh/src/main/resources/org/languagetool/rules/zh"))
}

pub fn normalize_zh_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    match xml.strip_prefix('\u{feff}') {
        Some(stripped) => std::borrow::Cow::Owned(stripped.to_string()),
        None => std::borrow::Cow::Borrowed(xml),
    }
}

pub struct ChineseNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn chinese_native_rules() -> ChineseNativeRules {
    ChineseNativeRules {
        rules: vec![Arc::new(
            og_rules::native_rules::DoublePunctuationRule::new()
                .with_description("使用两个连续的句号或逗号")
                .with_messages(
                    "两个连续的句号",
                    "两个连续的句号",
                    "两个连续的逗号",
                    "两个连续的逗号",
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
        let natives = chinese_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(ids, vec!["DOUBLE_PUNCTUATION"]);
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["WHITESPACE_RULE"]);
    }

    #[test]
    fn normalize_strips_only_the_leading_bom() {
        assert_eq!(
            normalize_zh_rules_xml("\u{feff}<rules lang='zh'/>").as_ref(),
            "<rules lang='zh'/>"
        );
        assert_eq!(
            normalize_zh_rules_xml("<a>\u{feff}</a>").as_ref(),
            "<a>\u{feff}</a>"
        );
    }
}
