use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueType {
    #[serde(rename = "grammar")]
    Grammar,
    #[serde(rename = "spelling")]
    Spelling,
    #[serde(rename = "typographical")]
    Typographical,
    #[serde(rename = "style")]
    Style,
    #[serde(rename = "whitespace")]
    Whitespace,
    #[serde(rename = "punctuation")]
    Punctuation,
    #[serde(rename = "capitalization")]
    Capitalization,
    #[serde(rename = "misspelling")]
    Misspelling,
    #[serde(rename = "other")]
    Other(String),
    #[serde(rename = "semantic")]
    Semantic,
    #[serde(rename = "redundancy")]
    Redundancy,
    #[serde(rename = "structure")]
    Structure,
    #[serde(rename = "locale-violation")]
    LocaleViolation,
    #[serde(rename = "inconsistency")]
    Inconsistency,
    #[serde(rename = "uncategorized")]
    Uncategorized,
    #[serde(rename = "non-conformance")]
    NonConformance,
    #[serde(rename = "compounding")]
    Compounding,
    #[serde(rename = "colloquialism")]
    Colloquialism,
    #[serde(rename = "false-friend")]
    FalseFriend,
    #[serde(rename = "readability")]
    Readability,
    #[serde(rename = "de")]
    De,
}

impl IssueType {
    pub fn as_str(&self) -> &str {
        match self {
            IssueType::Grammar => "grammar",
            IssueType::Spelling => "spelling",
            IssueType::Typographical => "typographical",
            IssueType::Style => "style",
            IssueType::Whitespace => "whitespace",
            IssueType::Punctuation => "punctuation",
            IssueType::Capitalization => "capitalization",
            IssueType::Misspelling => "misspelling",
            IssueType::Semantic => "semantic",
            IssueType::Redundancy => "redundancy",
            IssueType::Structure => "structure",
            IssueType::LocaleViolation => "locale-violation",
            IssueType::Inconsistency => "inconsistency",
            IssueType::Uncategorized => "uncategorized",
            IssueType::NonConformance => "non-conformance",
            IssueType::Compounding => "compounding",
            IssueType::Colloquialism => "colloquialism",
            IssueType::FalseFriend => "false-friend",
            IssueType::Readability => "readability",
            IssueType::De => "de",
            IssueType::Other(s) => s.as_str(),
        }
    }
}

impl Default for IssueType {
    fn default() -> Self {
        IssueType::Grammar
    }
}
