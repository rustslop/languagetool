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
    pub rule_regexp: Option<XmlRuleRegexp>,
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
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub suppress_misspelled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRuleRegexp {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub exact: bool,
    #[serde(default)]
    pub mark: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlCategory {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_on: bool,
    #[serde(default)]
    pub issue_type: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlPattern {
    pub elements: Vec<XmlPatternElement>,
    pub case_sensitive: bool,
    pub marker_start: Option<usize>,
    pub marker_end: Option<usize>,
    #[serde(skip)]
    pub marker_elem_start: Option<usize>,
    #[serde(skip)]
    pub marker_elem_end: Option<usize>,
    pub tokens: Vec<XmlPatternToken>,
    #[serde(skip)]
    pub unify_groups: Vec<XmlUnifyGroup>,
    pub raw_pos: bool,
}

#[derive(Debug, Clone, Default)]
pub struct XmlUnifyGroup {
    pub negate: bool,
    pub features: Vec<(String, Vec<String>)>,
    pub token_start: usize,
    pub token_end: usize,
    pub elem_start: usize,
    pub elem_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum XmlPatternElement {
    Token(XmlPatternToken),
    OrGroup(XmlOrGroup),
    AndGroup(XmlAndGroup),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlOrGroup {
    pub alternatives: Vec<XmlPatternToken>,
}

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
    pub case_sensitive_set: bool,
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
    #[serde(skip)]
    pub unify_neutral: bool,
    #[serde(default)]
    pub space_before: Option<String>,
    #[serde(default)]
    pub chunk: Option<String>,
    #[serde(default)]
    pub chunk_re: Option<String>,
    #[serde(default)]
    pub match_no: Option<i32>,
    pub match_no_case_conversion: Option<String>,
    #[serde(default)]
    pub match_no_postag: Option<String>,
    #[serde(default)]
    pub match_no_postag_regexp: bool,
    #[serde(default)]
    pub match_no_postag_replace: Option<String>,
    #[serde(default)]
    pub match_no_setpos: bool,
    #[serde(default)]
    pub match_no_regexp_match: Option<String>,
    #[serde(default)]
    pub match_no_regexp_replace: Option<String>,
}

impl XmlPatternToken {
    pub fn min_expansion(mut self) -> Vec<XmlPatternToken> {
        match self.min {
            Some(m) if m >= 2 => {
                self.min = None;
                vec![self.clone(); m as usize]
            }
            _ => vec![self],
        }
    }
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
    #[serde(default)]
    pub case_sensitive_set: bool,
    pub scope: Option<String>,
    #[serde(default)]
    pub space_before: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlMatch {
    pub no: i32,
    pub case_conversion: Option<String>,
    pub regexp_match: Option<String>,
    pub regexp_replace: Option<String>,
    pub include_inflected: bool,
    pub include_skipped: Option<String>,
    #[serde(default)]
    pub postag: Option<String>,
    #[serde(default)]
    pub postag_regexp: bool,
    #[serde(default)]
    pub postag_replace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestionPart {
    Text(String),
    Match(XmlMatch),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlSuggestion {
    pub text: String,
    #[serde(default)]
    pub parts: Vec<SuggestionPart>,
    #[serde(default)]
    pub suppress_misspelled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlExample {
    pub example_type: XmlExampleType,
    pub text: String,
    #[serde(default)]
    pub corrections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum XmlExampleType {
    #[default]
    Correct,
    Incorrect,
    TriggersError,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRuleGroup {
    pub id: Option<String>,
    pub name: Option<String>,
    pub default_on: Option<bool>,
    pub antipatterns: Vec<XmlPattern>,
    pub rules: Vec<XmlRule>,
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmlRuleFile {
    #[serde(skip)]
    pub unifications: Vec<crate::disambig_types::UnificationFeature>,
    pub categories: Vec<XmlCategory>,
    pub rule_groups: Vec<XmlRuleGroup>,
    pub rules: Vec<XmlRule>,
}
