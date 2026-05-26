use serde::{Deserialize, Serialize};
use crate::Category;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedReplacement {
    value: String,
    #[serde(default)]
    short_description: Option<String>,
}

impl SuggestedReplacement {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            short_description: None,
        }
    }

    pub fn with_short_description(mut self, desc: impl Into<String>) -> Self {
        self.short_description = Some(desc.into());
        self
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn short_description(&self) -> Option<&str> {
        self.short_description.as_deref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatchContext {
    text: String,
    offset: usize,
    length: usize,
}

impl RuleMatchContext {
    pub fn new(text: impl Into<String>, offset: usize, length: usize) -> Self {
        Self {
            text: text.into(),
            offset,
            length,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatchRule {
    id: String,
    description: String,
    #[serde(default)]
    issue_type: String,
    #[serde(default)]
    sub_id: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    category: Category,
}

impl RuleMatchRule {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            issue_type: "grammar".to_string(),
            sub_id: None,
            urls: Vec::new(),
            category: Category::new("MISC", "Miscellaneous"),
        }
    }

    pub fn with_issue_type(mut self, issue_type: impl Into<String>) -> Self {
        self.issue_type = issue_type.into();
        self
    }

    pub fn with_sub_id(mut self, sub_id: impl Into<String>) -> Self {
        self.sub_id = Some(sub_id.into());
        self
    }

    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    pub fn with_urls(mut self, urls: Vec<String>) -> Self {
        self.urls = urls;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    message: String,
    #[serde(rename = "shortMessage")]
    short_message: String,
    offset: usize,
    length: usize,
    replacements: Vec<SuggestedReplacement>,
    context: RuleMatchContext,
    rule: RuleMatchRule,
    #[serde(rename = "ignoreForIncompleteSentence")]
    ignore_for_incomplete_sentence: bool,
    #[serde(rename = "contextForSureMatch")]
    context_for_sure_match: i32,
    #[serde(default)]
    sentence: Option<String>,
}

impl RuleMatch {
    pub fn new(
        message: impl Into<String>,
        offset: usize,
        length: usize,
        rule: RuleMatchRule,
        context: RuleMatchContext,
    ) -> Self {
        Self {
            message: message.into(),
            short_message: String::new(),
            offset,
            length,
            replacements: Vec::new(),
            context,
            rule,
            ignore_for_incomplete_sentence: false,
            context_for_sure_match: -1,
            sentence: None,
        }
    }

    pub fn with_short_message(mut self, msg: impl Into<String>) -> Self {
        self.short_message = msg.into();
        self
    }

    pub fn with_replacements(mut self, replacements: Vec<SuggestedReplacement>) -> Self {
        self.replacements = replacements;
        self
    }

    pub fn with_sentence(mut self, sentence: impl Into<String>) -> Self {
        self.sentence = Some(sentence.into());
        self
    }

    pub fn with_ignore_for_incomplete_sentence(mut self, ignore: bool) -> Self {
        self.ignore_for_incomplete_sentence = ignore;
        self
    }

    pub fn with_context_for_sure_match(mut self, val: i32) -> Self {
        self.context_for_sure_match = val;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn short_message(&self) -> &str {
        &self.short_message
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn replacements(&self) -> &[SuggestedReplacement] {
        &self.replacements
    }

    pub fn context(&self) -> &RuleMatchContext {
        &self.context
    }

    pub fn rule(&self) -> &RuleMatchRule {
        &self.rule
    }

    pub fn sentence(&self) -> Option<&str> {
        self.sentence.as_deref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub software: SoftwareInfo,
    #[serde(default)]
    pub warnings: Warnings,
    pub language: LanguageInfo,
    pub matches: Vec<RuleMatch>,
    #[serde(rename = "sentenceRanges", default)]
    pub sentence_ranges: Vec<Vec<usize>>,
    #[serde(rename = "extendedSentenceRanges", default)]
    pub extended_sentence_ranges: Vec<ExtendedSentenceRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareInfo {
    pub name: String,
    pub version: String,
    #[serde(rename = "buildDate")]
    pub build_date: String,
    #[serde(rename = "apiVersion")]
    pub api_version: i32,
    #[serde(default)]
    pub premium: bool,
    #[serde(default)]
    pub premium_hint: Option<String>,
    #[serde(rename = "status", default)]
    pub status: Option<String>,
}

impl Default for SoftwareInfo {
    fn default() -> Self {
        Self {
            name: "OpenGrammar".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_date: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            api_version: 2,
            premium: false,
            premium_hint: None,
            status: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Warnings {
    #[serde(default)]
    pub incomplete_results: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub code: String,
    pub detected: LanguageDetectedInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDetectedInfo {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub preferred: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedSentenceRange {
    #[serde(rename = "from")]
    pub from_offset: usize,
    pub to: usize,
    #[serde(default)]
    pub type_: Option<String>,
}
