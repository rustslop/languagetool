use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlFilter {
    pub class: String,
    #[serde(default)]
    pub args: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sub_id: Option<String>,
    #[serde(default)]
    pub description: String,
    pub category: XmlCategory,
    #[serde(default)]
    pub antipatterns: Vec<XmlPattern>,
    #[serde(default)]
    pub pattern: XmlPattern,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub short_message: Option<String>,
    #[serde(default)]
    pub suggestions: Vec<XmlSuggestion>,
    #[serde(default)]
    pub examples: Vec<XmlExample>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub default_on: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub filter: Option<XmlFilter>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_on: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlPattern {
    /// Pattern elements (tokens, or-groups, and-groups) in order
    pub elements: Vec<XmlPatternElement>,
    pub case_sensitive: bool,
    /// Index of first token inside <marker>, if any
    pub marker_start: Option<usize>,
    /// Index after last token inside <marker>, if any (exclusive end)
    pub marker_end: Option<usize>,
    /// Legacy: flat token list for backward compatibility
    pub tokens: Vec<XmlPatternToken>,
}

/// A single position in a pattern can be:
/// - A regular token
/// - An <or> group (any one of the alternatives matches)
/// - An <and> group (all constraints must match)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum XmlPatternElement {
    Token(XmlPatternToken),
    OrGroup(XmlOrGroup),
    AndGroup(XmlAndGroup),
}

/// Represents an <or> group - a set of alternative tokens for one pattern position
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlOrGroup {
    pub alternatives: Vec<XmlPatternToken>,
}

/// Represents an <and> group - all tokens must match at the same position
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlAndGroup {
    pub constraints: Vec<XmlPatternToken>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlPatternToken {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub regexp: Option<String>,
    #[serde(default)]
    pub postag: Option<String>,
    #[serde(default)]
    pub postag_regexp: Option<String>,
    #[serde(default)]
    pub negate: bool,
    #[serde(default)]
    pub negate_pos: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub inflected: bool,
    #[serde(default)]
    pub min: Option<i32>,
    #[serde(default)]
    pub max: Option<i32>,
    #[serde(default)]
    pub skip: i32,
    #[serde(default)]
    pub exceptions: Vec<XmlException>,
    #[serde(default)]
    pub space_before: Option<String>,
    #[serde(default)]
    pub chunk: Option<String>,
    #[serde(default)]
    pub chunk_re: Option<String>,
    /// Backreference: match the same text as pattern token N (1-based).
    /// Set by <match no="N"/> inside <token>.
    #[serde(default)]
    pub match_no: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlException {
    pub text: Option<String>,
    pub regexp: Option<String>,
    pub postag: Option<String>,
    pub postag_regexp: Option<String>,
    pub negate: bool,
    pub negate_pos: bool,
    pub inflected: bool,
    pub case_sensitive: bool,
    pub scope: Option<String>,
}

/// A match element inside a suggestion, referring to a matched token
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlMatch {
    /// Token number (1-based) to reference
    pub no: i32,
    /// Case conversion to apply
    pub case_conversion: Option<String>,
    /// Regex to apply to the matched text
    pub regexp_match: Option<String>,
    /// Replacement for the regex match
    pub regexp_replace: Option<String>,
    /// Whether to include the lemma instead of the surface form
    pub include_inflected: bool,
}

/// Part of a suggestion - either literal text or a match reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionPart {
    Text(String),
    Match(XmlMatch),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlSuggestion {
    /// Legacy: simple text suggestion (used when no match elements)
    pub text: String,
    /// Structured parts: mix of text and match references
    #[serde(default)]
    pub parts: Vec<SuggestionPart>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlExample {
    pub example_type: XmlExampleType,
    pub text: String,
    #[serde(default)]
    pub corrections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum XmlExampleType {
    Correct,
    Incorrect,
    TriggersError, // Known false positive - expected to match
}

impl Default for XmlExampleType {
    fn default() -> Self {
        XmlExampleType::Correct
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRuleGroup {
    pub id: Option<String>,
    pub name: Option<String>,
    pub default_on: Option<bool>,
    pub antipatterns: Vec<XmlPattern>,
    pub rules: Vec<XmlRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRuleFile {
    pub categories: Vec<XmlCategory>,
    pub rule_groups: Vec<XmlRuleGroup>,
    pub rules: Vec<XmlRule>,
}
