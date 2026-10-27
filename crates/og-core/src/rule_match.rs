use crate::Category;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedReplacement {
    value: String,
    #[serde(default, rename = "shortDescription")]
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

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
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
pub struct RuleMatchUrl {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatchRule {
    id: String,
    #[serde(default, rename = "subId", skip_serializing_if = "Option::is_none")]
    sub_id: Option<String>,
    #[serde(
        default,
        rename = "sourceFile",
        skip_serializing_if = "Option::is_none"
    )]
    source_file: Option<String>,
    description: String,
    #[serde(rename = "issueType", default)]
    issue_type: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    urls: Vec<RuleMatchUrl>,
    #[serde(default)]
    category: Category,
    #[serde(default, skip_serializing)]
    picky: bool,
}

impl RuleMatchRule {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sub_id: None,
            source_file: None,
            description: description.into(),
            issue_type: "grammar".to_string(),
            urls: Vec::new(),
            category: Category::new("MISC", "Miscellaneous"),
            picky: false,
        }
    }

    pub fn with_picky(mut self, picky: bool) -> Self {
        self.picky = picky;
        self
    }

    pub fn set_picky(&mut self, picky: bool) {
        self.picky = picky;
    }

    pub fn is_picky(&self) -> bool {
        self.picky
    }

    pub fn set_issue_type(&mut self, issue_type: impl Into<String>) {
        self.issue_type = issue_type.into();
    }

    pub fn with_issue_type(mut self, issue_type: impl Into<String>) -> Self {
        self.issue_type = issue_type.into();
        self
    }

    pub fn with_sub_id(mut self, sub_id: impl Into<String>) -> Self {
        self.sub_id = Some(sub_id.into());
        self
    }

    pub fn with_source_file(mut self, source_file: impl Into<String>) -> Self {
        self.source_file = Some(source_file.into());
        self
    }

    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    pub fn with_urls(mut self, urls: Vec<String>) -> Self {
        self.urls = urls
            .into_iter()
            .map(|value| RuleMatchUrl { value })
            .collect();
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn category(&self) -> &Category {
        &self.category
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn source_file(&self) -> Option<&str> {
        self.source_file.as_deref()
    }

    pub fn pattern_match_kind(&self) -> RuleMatchKind {
        match self.issue_type.as_str() {
            "style" | "locale-violation" | "register" => RuleMatchKind::Hint,
            _ => RuleMatchKind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleMatchKind {
    UnknownWord,
    Hint,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchType {
    #[serde(rename = "typeName")]
    kind: RuleMatchKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    message: String,
    #[serde(
        rename = "shortMessage",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    short_message: Option<String>,
    replacements: Vec<SuggestedReplacement>,
    offset: usize,
    length: usize,
    context: RuleMatchContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sentence: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    issue_type: Option<MatchType>,
    rule: RuleMatchRule,
    #[serde(rename = "ignoreForIncompleteSentence")]
    ignore_for_incomplete_sentence: bool,
    #[serde(rename = "contextForSureMatch")]
    context_for_sure_match: i32,
}

impl RuleMatch {
    pub fn new(
        message: impl Into<String>,
        offset: usize,
        length: usize,
        rule: RuleMatchRule,
        context: RuleMatchContext,
    ) -> Self {
        let issue_type = Some(MatchType {
            kind: RuleMatchKind::Other,
        });
        Self {
            message: message.into(),
            short_message: None,
            replacements: Vec::new(),
            offset,
            length,
            context,
            sentence: None,
            issue_type,
            rule,
            ignore_for_incomplete_sentence: false,
            context_for_sure_match: -1,
        }
    }

    pub fn with_short_message(mut self, msg: impl Into<String>) -> Self {
        self.short_message = Some(msg.into());
        self
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    pub fn with_match_type(mut self, kind: RuleMatchKind) -> Self {
        self.issue_type = Some(MatchType { kind });
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

    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    pub fn with_length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }

    pub fn set_offset_length(&mut self, offset: usize, length: usize) {
        self.offset = offset;
        self.length = length;
    }

    pub fn set_replacements(&mut self, replacements: Vec<SuggestedReplacement>) {
        self.replacements = replacements;
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn short_message(&self) -> Option<&str> {
        self.short_message.as_deref()
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

    pub fn replacements_mut(&mut self) -> &mut Vec<SuggestedReplacement> {
        &mut self.replacements
    }

    pub fn set_picky(&mut self, picky: bool) {
        self.rule.set_picky(picky);
    }

    pub fn context(&self) -> &RuleMatchContext {
        &self.context
    }

    pub fn rule(&self) -> &RuleMatchRule {
        &self.rule
    }

    pub fn rule_mut(&mut self) -> &mut RuleMatchRule {
        &mut self.rule
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
            api_version: 1,
            premium: false,
            premium_hint: None,
            status: Some(String::new()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Warnings {
    #[serde(default, rename = "incompleteResults")]
    pub incomplete_results: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub code: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "detectedLanguage")]
    pub detected: LanguageDetectedInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDetectedInfo {
    pub name: String,
    pub code: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedLanguageRate {
    pub language: String,
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedSentenceRange {
    #[serde(rename = "from")]
    pub from_offset: usize,
    pub to: usize,
    #[serde(default)]
    pub detected_languages: Vec<DetectedLanguageRate>,
}

#[cfg(test)]
mod json_shape_tests {
    use super::*;

    fn sample_match() -> RuleMatch {
        let rule = RuleMatchRule::new("EN_A_VS_AN", "Use of 'a' vs 'an'")
            .with_sub_id("1")
            .with_source_file("grammar.xml")
            .with_issue_type("grammar")
            .with_urls(vec!["https://example.org/rule".to_string()])
            .with_category(Category::new("MISC", "Miscellaneous"));
        let mut m = RuleMatch::new(
            "Did you mean <suggestion>an</suggestion>?",
            6,
            1,
            rule,
            RuleMatchContext::new("have a apple", 5, 1),
        )
        .with_short_message("Use 'an'");
        m.replacements = vec![SuggestedReplacement::new("an").with_short_description("article")];
        m
    }

    fn sample_result() -> CheckResult {
        CheckResult {
            software: SoftwareInfo::default(),
            warnings: Warnings::default(),
            language: LanguageInfo {
                code: "en-US".to_string(),
                name: "English (US)".to_string(),
                detected: LanguageDetectedInfo {
                    name: "English (US)".to_string(),
                    code: "en-US".to_string(),
                    confidence: 1.0,
                    source: None,
                },
            },
            matches: vec![sample_match()],
            sentence_ranges: vec![vec![0, 12]],
            extended_sentence_ranges: Vec::new(),
        }
    }

    #[test]
    fn match_serializes_with_java_shape() {
        let json = serde_json::to_value(sample_match()).unwrap();
        assert_eq!(json["message"], "Did you mean <suggestion>an</suggestion>?");
        assert_eq!(json["shortMessage"], "Use 'an'");
        assert_eq!(json["offset"], 6);
        assert_eq!(json["length"], 1);
        assert_eq!(json["replacements"][0]["value"], "an");
        assert_eq!(json["replacements"][0]["shortDescription"], "article");
        assert_eq!(json["context"]["text"], "have a apple");
        assert!(json["type"].is_object());
        assert_eq!(json["type"]["typeName"], "Other");
        assert_eq!(json["rule"]["id"], "EN_A_VS_AN");
        assert_eq!(json["rule"]["subId"], "1");
        assert_eq!(json["rule"]["sourceFile"], "grammar.xml");
        assert_eq!(json["rule"]["description"], "Use of 'a' vs 'an'");
        assert_eq!(json["rule"]["issueType"], "grammar");
        assert_eq!(json["rule"]["urls"][0]["value"], "https://example.org/rule");
        assert_eq!(json["rule"]["category"]["id"], "MISC");
        assert_eq!(json["rule"]["category"]["name"], "Miscellaneous");
        assert!(json["rule"]["category"].get("description").is_none());
        assert!(json["rule"]["category"].get("enabled").is_none());
        assert!(json["rule"]["category"].get("default_on").is_none());
        assert!(json["ignoreForIncompleteSentence"].is_boolean());
        assert!(json["contextForSureMatch"].is_number());
    }

    #[test]
    fn optional_rule_fields_are_omitted() {
        let rule = RuleMatchRule::new("X", "Y");
        let m = RuleMatch::new("m", 0, 1, rule, RuleMatchContext::new("m", 0, 1));
        let json = serde_json::to_value(&m).unwrap();
        assert!(json.get("shortMessage").is_none());
        assert!(json["rule"].get("subId").is_none());
        assert!(json["rule"].get("sourceFile").is_none());
        assert!(json["rule"].get("urls").is_none());
        assert!(json["type"].is_object());
        assert_eq!(json["type"]["typeName"], "Other");
    }

    #[test]
    fn match_type_uses_java_enum_value_space() {
        let rule = RuleMatchRule::new("X", "Y");
        let base = RuleMatch::new("m", 0, 1, rule, RuleMatchContext::new("m", 0, 1));
        assert_eq!(
            serde_json::to_value(&base).unwrap()["type"]["typeName"],
            "Other"
        );
        let spelling = base.clone().with_match_type(RuleMatchKind::UnknownWord);
        assert_eq!(
            serde_json::to_value(&spelling).unwrap()["type"]["typeName"],
            "UnknownWord"
        );
        let style = base.with_match_type(RuleMatchKind::Hint);
        assert_eq!(
            serde_json::to_value(&style).unwrap()["type"]["typeName"],
            "Hint"
        );
    }

    #[test]
    fn result_serializes_with_java_shape() {
        let json = serde_json::to_value(sample_result()).unwrap();
        assert_eq!(json["software"]["name"], "OpenGrammar");
        assert_eq!(json["software"]["apiVersion"], 1);
        assert_eq!(json["software"]["status"], "");
        assert!(json["software"]["buildDate"].is_string());
        assert!(json["software"].get("premiumHint").is_none());
        assert_eq!(json["warnings"]["incompleteResults"], false);
        assert_eq!(json["language"]["code"], "en-US");
        assert_eq!(json["language"]["name"], "English (US)");
        assert!(json["language"]["detectedLanguage"].is_object());
        assert_eq!(json["language"]["detectedLanguage"]["code"], "en-US");
        assert_eq!(json["language"]["detectedLanguage"]["name"], "English (US)");
        assert_eq!(json["language"]["detectedLanguage"]["confidence"], 1.0);
        assert!(json["language"]["detectedLanguage"].get("source").is_some());
        assert!(json["language"].get("detected").is_none());
        assert_eq!(json["sentenceRanges"][0], serde_json::json!([0, 12]));
        assert!(json["matches"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn pattern_match_kind_maps_style_issue_types_to_hint() {
        for issue in ["style", "locale-violation", "register"] {
            let rule = RuleMatchRule::new("X", "Y").with_issue_type(issue);
            assert_eq!(
                rule.pattern_match_kind(),
                RuleMatchKind::Hint,
                "issue {issue}"
            );
        }
        for issue in ["grammar", "misspelling", "typographical", "redundancy"] {
            let rule = RuleMatchRule::new("X", "Y").with_issue_type(issue);
            assert_eq!(
                rule.pattern_match_kind(),
                RuleMatchKind::Other,
                "issue {issue}"
            );
        }
    }

    #[test]
    fn rule_matches_default_to_other_regardless_of_issue_type() {
        for issue in ["style", "grammar"] {
            let rule = RuleMatchRule::new("X", "Y").with_issue_type(issue);
            let m = RuleMatch::new("m", 0, 1, rule, RuleMatchContext::new("m", 0, 1));
            assert_eq!(
                serde_json::to_value(&m).unwrap()["type"]["typeName"],
                "Other",
                "issue {issue} must not influence the default match type"
            );
        }
    }

    #[test]
    fn match_json_round_trips() {
        let json = serde_json::to_string(&sample_match()).unwrap();
        let back: RuleMatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rule().id(), "EN_A_VS_AN");
        assert_eq!(back.offset(), 6);
        assert_eq!(back.length(), 1);
    }
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    fn base() -> RuleMatch {
        RuleMatch::new(
            "msg",
            0,
            3,
            RuleMatchRule::new("R", "d"),
            RuleMatchContext::new("ctx", 0, 3),
        )
    }

    #[test]
    fn builders_adjust_offsets_lengths_and_message() {
        let m = base().with_offset(7).with_length(11);
        assert_eq!(m.offset(), 7);
        assert_eq!(m.length(), 11);
        let mut m = m;
        m.set_message("new message");
        assert_eq!(m.message(), "new message");
        m.set_offset_length(2, 5);
        assert_eq!((m.offset(), m.length()), (2, 5));
    }

    #[test]
    fn replacements_can_be_set_and_mutated() {
        let mut m = base();
        assert!(m.replacements().is_empty());
        m.set_replacements(vec![SuggestedReplacement::new("a")]);
        assert_eq!(m.replacements()[0].value(), "a");
        m.replacements_mut()
            .push(SuggestedReplacement::new("b").with_short_description("alt"));
        assert_eq!(m.replacements().len(), 2);
        assert_eq!(m.replacements()[1].short_description(), Some("alt"));

        let mut replacement = SuggestedReplacement::new("x");
        replacement.set_value("y");
        assert_eq!(replacement.value(), "y");
        assert_eq!(replacement.short_description(), None);
    }

    #[test]
    fn sentence_and_flag_builders_serialize() {
        let m = base()
            .with_sentence("the sentence")
            .with_ignore_for_incomplete_sentence(true)
            .with_context_for_sure_match(3);
        assert_eq!(m.sentence(), Some("the sentence"));
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["contextForSureMatch"], 3);
        assert_eq!(json["ignoreForIncompleteSentence"], true);
        let bare = serde_json::to_value(&base()).unwrap();
        assert_eq!(bare["contextForSureMatch"], -1);
        assert_eq!(bare["ignoreForIncompleteSentence"], false);
    }

    #[test]
    fn picky_flag_is_delegated_to_the_rule_and_stays_unserialized() {
        let mut m = base();
        assert!(!m.rule().is_picky());
        m.set_picky(true);
        assert!(m.rule().is_picky());
        let json = serde_json::to_value(&m).unwrap();
        assert!(json.get("picky").is_none());
        assert!(json["rule"].get("picky").is_none());
    }

    #[test]
    fn rule_metadata_accessors_expose_builders() {
        let rule = RuleMatchRule::new("ID", "desc")
            .with_sub_id("2")
            .with_source_file("grammar.xml")
            .with_issue_type("typographical")
            .with_urls(vec!["https://example.org".to_string()])
            .with_category(Category::new("CAT", "Cat"))
            .with_picky(true);
        assert_eq!(rule.id(), "ID");
        assert_eq!(rule.description(), "desc");
        assert_eq!(rule.source_file(), Some("grammar.xml"));
        assert_eq!(rule.category().id().as_str(), "CAT");
        assert!(rule.is_picky());

        let mut m = base();
        *m.rule_mut() = rule;
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["rule"]["subId"], "2");
        assert_eq!(json["rule"]["sourceFile"], "grammar.xml");
        assert_eq!(json["rule"]["issueType"], "typographical");
        assert_eq!(json["rule"]["urls"][0]["value"], "https://example.org");
        assert_eq!(json["rule"]["category"]["id"], "CAT");
    }

    #[test]
    fn set_issue_type_updates_through_rule_mut() {
        let mut rule = RuleMatchRule::new("ID", "desc");
        rule.set_issue_type("misspelling");
        assert_eq!(rule.pattern_match_kind(), RuleMatchKind::Other);
    }

    #[test]
    fn context_accessors_expose_text_offset_length() {
        let context = RuleMatchContext::new("abc def", 3, 3);
        assert_eq!(context.text(), "abc def");
        assert_eq!(context.offset(), 3);
        assert_eq!(context.length(), 3);
    }

    #[test]
    fn match_type_builder_round_trips_through_json() {
        let m = base().with_match_type(RuleMatchKind::UnknownWord);
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["type"]["typeName"], "UnknownWord");
        let back: RuleMatch = serde_json::from_value(json).unwrap();
        assert_eq!(back.message(), "msg");
    }
}
