use crate::{AnalyzedSentence, Category, IssueType, RuleMatch};

pub trait Rule: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    fn is_default_on(&self) -> bool {
        true
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
