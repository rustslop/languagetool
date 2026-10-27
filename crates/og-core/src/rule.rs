use crate::{AnalyzedSentence, Category, IssueType, RuleMatch};

pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    fn is_default_on(&self) -> bool {
        true
    }
    fn is_picky(&self) -> bool {
        false
    }
    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }
    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }
    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch>;
}

pub trait TextLevelRule: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    fn is_default_on(&self) -> bool {
        true
    }
    fn is_picky(&self) -> bool {
        false
    }
    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }
    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }
    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(String);

impl RuleId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MinimalRule;
    impl Rule for MinimalRule {
        fn id(&self) -> &str {
            "MINIMAL"
        }
        fn match_sentence(&self, _sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
            Vec::new()
        }
    }

    struct MinimalTextRule;
    impl TextLevelRule for MinimalTextRule {
        fn id(&self) -> &str {
            "MINIMAL_TEXT"
        }
        fn match_text(&self, _text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
            Vec::new()
        }
    }

    #[test]
    fn rule_id_displays_and_compares_by_value() {
        let id = RuleId::new("EN_A_VS_AN");
        assert_eq!(id.as_str(), "EN_A_VS_AN");
        assert_eq!(id.to_string(), "EN_A_VS_AN");
        assert_eq!(RuleId::new("X"), RuleId::new("X"));
        assert_ne!(RuleId::new("X"), RuleId::new("Y"));
    }

    #[test]
    fn rule_defaults_match_java_abstract_rule_defaults() {
        let rule = MinimalRule;
        assert_eq!(rule.id(), "MINIMAL");
        assert_eq!(rule.description(), "");
        assert!(rule.is_default_on());
        assert!(!rule.is_picky());
        assert_eq!(rule.category().id().as_str(), "MISC");
        assert_eq!(rule.category().name(), "Miscellaneous");
        assert_eq!(rule.issue_type(), IssueType::Grammar);
        assert!(
            rule.match_sentence(&AnalyzedSentence::new("", 0, 0))
                .is_empty()
        );
    }

    #[test]
    fn text_level_rule_defaults_match_rule_defaults() {
        let rule = MinimalTextRule;
        assert_eq!(rule.id(), "MINIMAL_TEXT");
        assert_eq!(rule.description(), "");
        assert!(rule.is_default_on());
        assert!(!rule.is_picky());
        assert_eq!(rule.category().id().as_str(), "MISC");
        assert_eq!(rule.issue_type(), IssueType::Grammar);
        assert!(rule.match_text("", &[]).is_empty());
    }
}
