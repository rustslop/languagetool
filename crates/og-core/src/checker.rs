use crate::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, CheckResult, DetectedLanguageRate,
    ExtendedSentenceRange, Language, LanguageDetectedInfo, LanguageInfo, RuleMatch, SoftwareInfo,
    Warnings, match_filters::LanguageMatchFilter, rule::Rule,
};
use std::sync::Arc;

pub use crate::rule::TextLevelRule;

#[derive(Debug, Clone)]
pub struct SentenceRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct CheckRequest {
    pub text: String,
    pub language: Language,
    pub mother_tongue: Option<Language>,
    pub enabled_rules: Vec<String>,
    pub disabled_rules: Vec<String>,
    pub enabled_categories: Vec<String>,
    pub disabled_categories: Vec<String>,
    pub level: Option<String>,
    pub picky: bool,
    pub run_all_rules: bool,
}

impl CheckRequest {
    pub fn new(text: impl Into<String>, language: Language) -> Self {
        Self {
            text: text.into(),
            language,
            mother_tongue: None,
            enabled_rules: Vec::new(),
            disabled_rules: Vec::new(),
            enabled_categories: Vec::new(),
            disabled_categories: Vec::new(),
            level: None,
            picky: false,
            run_all_rules: false,
        }
    }

    fn level_is_default(&self) -> bool {
        !self.picky && matches!(self.level.as_deref(), None | Some("default"))
    }

    pub fn rule_enabled_by_id(&self, rule_id: &str, default_on: bool, picky_tagged: bool) -> bool {
        if !self.enabled_rules.is_empty() && !self.enabled_rules.iter().any(|r| r == rule_id) {
            return false;
        }
        if self.disabled_rules.iter().any(|r| r == rule_id) {
            return false;
        }
        if self.run_all_rules {
            return true;
        }
        if !default_on {
            return self.enabled_rules.iter().any(|r| r == rule_id);
        }
        !(picky_tagged && self.level_is_default())
    }

    pub fn category_enabled(&self, category_id: &str) -> bool {
        if !self.enabled_categories.is_empty()
            && !self.enabled_categories.iter().any(|c| c == category_id)
        {
            return false;
        }
        if self.disabled_categories.iter().any(|c| c == category_id) {
            return false;
        }
        true
    }

    pub fn has_category_filters(&self) -> bool {
        !self.enabled_categories.is_empty() || !self.disabled_categories.is_empty()
    }

    pub fn is_rule_enabled(
        &self,
        rule_id: &str,
        category_id: &str,
        default_on: bool,
        picky_tagged: bool,
    ) -> bool {
        if !self.enabled_rules.is_empty() && !self.enabled_rules.iter().any(|r| r == rule_id) {
            return false;
        }
        if self.disabled_rules.iter().any(|r| r == rule_id) {
            return false;
        }
        if !self.enabled_categories.is_empty()
            && !self.enabled_categories.iter().any(|c| c == category_id)
        {
            return false;
        }
        if self.disabled_categories.iter().any(|c| c == category_id) {
            return false;
        }
        if self.run_all_rules {
            return true;
        }
        if !default_on {
            return self.enabled_rules.iter().any(|r| r == rule_id);
        }
        !(picky_tagged && self.level_is_default())
    }
}

pub trait SentenceTokenizer: Send + Sync {
    fn split(&self, text: &str) -> Vec<SentenceRange>;
}

pub trait WordTokenizer: Send + Sync {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings>;
}

pub trait Tagger: Send + Sync {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]);
}

pub trait Disambiguator: Send + Sync {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence);
}

pub trait Chunker: Send + Sync {
    fn add_chunk_tags(&self, sentence: &mut AnalyzedSentence);
}

pub struct Checker {
    sentence_tokenizer: Option<Arc<dyn SentenceTokenizer>>,
    word_tokenizer: Option<Arc<dyn WordTokenizer>>,
    tagger: Option<Arc<dyn Tagger>>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    chunker: Option<Arc<dyn Chunker>>,
    post_chunker: Option<Arc<dyn Chunker>>,
    rules: Vec<Arc<dyn Rule>>,
    text_level_rules: Vec<Arc<dyn TextLevelRule>>,
    language_filter: Option<Arc<dyn LanguageMatchFilter>>,
    clean_overlapping_matches: bool,
}

impl Checker {
    pub fn new() -> Self {
        Self {
            sentence_tokenizer: None,
            word_tokenizer: None,
            tagger: None,
            disambiguator: None,
            chunker: None,
            post_chunker: None,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
            language_filter: None,
            clean_overlapping_matches: true,
        }
    }

    pub fn with_sentence_tokenizer(mut self, tokenizer: Arc<dyn SentenceTokenizer>) -> Self {
        self.sentence_tokenizer = Some(tokenizer);
        self
    }

    pub fn with_word_tokenizer(mut self, tokenizer: Arc<dyn WordTokenizer>) -> Self {
        self.word_tokenizer = Some(tokenizer);
        self
    }

    pub fn with_tagger(mut self, tagger: Arc<dyn Tagger>) -> Self {
        self.tagger = Some(tagger);
        self
    }

    pub fn with_chunker(mut self, chunker: Arc<dyn Chunker>) -> Self {
        self.chunker = Some(chunker);
        self
    }

    pub fn with_post_chunker(mut self, chunker: Arc<dyn Chunker>) -> Self {
        self.post_chunker = Some(chunker);
        self
    }

    pub fn with_disambiguator(mut self, disambiguator: Arc<dyn Disambiguator>) -> Self {
        self.disambiguator = Some(disambiguator);
        self
    }

    pub fn add_rule(mut self, rule: Arc<dyn Rule>) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn set_rules(mut self, rules: Vec<Arc<dyn Rule>>) -> Self {
        self.rules = rules;
        self
    }

    pub fn add_text_level_rule(mut self, rule: Arc<dyn TextLevelRule>) -> Self {
        self.text_level_rules.push(rule);
        self
    }

    pub fn set_text_level_rules(mut self, rules: Vec<Arc<dyn TextLevelRule>>) -> Self {
        self.text_level_rules = rules;
        self
    }

    pub fn with_language_filter(mut self, filter: Arc<dyn LanguageMatchFilter>) -> Self {
        self.language_filter = Some(filter);
        self
    }

    pub fn with_clean_overlapping_matches(mut self, clean: bool) -> Self {
        self.clean_overlapping_matches = clean;
        self
    }

    pub fn rules(&self) -> &[Arc<dyn Rule>] {
        &self.rules
    }

    pub fn text_level_rules(&self) -> &[Arc<dyn TextLevelRule>] {
        &self.text_level_rules
    }

    pub fn check(&self, request: &CheckRequest) -> CheckResult {
        let text = &request.text;

        let sentence_ranges = if let Some(ref tokenizer) = self.sentence_tokenizer {
            tokenizer.split(text)
        } else {
            vec![SentenceRange {
                start: 0,
                end: text.len(),
            }]
        };

        let mut all_matches: Vec<RuleMatch> = Vec::new();
        let mut analyzed_sentences = Vec::new();

        for range in &sentence_ranges {
            let sentence_text = text[range.start..range.end].to_string();
            let mut sentence = AnalyzedSentence::new(&sentence_text, range.start, range.end);

            if let Some(ref tokenizer) = self.word_tokenizer {
                let mut tokens = tokenizer.tokenize(&sentence_text, range.start);
                let start_token = AnalyzedTokenReadings::new(
                    AnalyzedToken::new("<S>", range.start, range.start)
                        .with_pos_tags(vec!["SENT_START".to_string()]),
                );
                tokens.insert(0, start_token);
                sentence.set_tokens(tokens);
            }

            if let Some(ref tagger) = self.tagger {
                let tokens = &mut sentence.tokens_mut();
                tagger.tag(tokens);
            }

            if let Some(ref chunker) = self.chunker {
                chunker.add_chunk_tags(&mut sentence);
            }

            {
                let tokens = sentence.tokens_mut();
                for i in (0..tokens.len()).rev() {
                    if !tokens[i].is_whitespace() && !tokens[i].has_pos_tag("SENT_START") {
                        if !tokens[i].has_pos_tag("SENT_END") {
                            tokens[i].add_sent_end();
                        }
                        break;
                    }
                }
            }

            if self.disambiguator.is_some() {
                sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
            }

            if let Some(ref disambiguator) = self.disambiguator {
                disambiguator.disambiguate(&mut sentence);
            }

            if let Some(ref chunker) = self.post_chunker {
                chunker.add_chunk_tags(&mut sentence);
            }

            let has_cat_filters = request.has_category_filters();
            for rule in &self.rules {
                if has_cat_filters {
                    if !request.is_rule_enabled(
                        rule.id(),
                        rule.category().id().as_str(),
                        rule.is_default_on(),
                        rule.is_picky(),
                    ) {
                        continue;
                    }
                } else if !request.rule_enabled_by_id(
                    rule.id(),
                    rule.is_default_on(),
                    rule.is_picky(),
                ) {
                    continue;
                }
                let matches = rule.match_sentence(&sentence);
                all_matches.extend(matches);
            }

            analyzed_sentences.push(sentence);
        }

        let has_cat_filters = request.has_category_filters();
        for rule in &self.text_level_rules {
            if has_cat_filters {
                if !request.is_rule_enabled(
                    rule.id(),
                    rule.category().id().as_str(),
                    rule.is_default_on(),
                    rule.is_picky(),
                ) {
                    continue;
                }
            } else if !request.rule_enabled_by_id(rule.id(), rule.is_default_on(), rule.is_picky())
            {
                continue;
            }
            let matches = rule.match_text(text, &analyzed_sentences);
            all_matches.extend(matches);
        }

        crate::match_filters::same_rule_group_filter(&mut all_matches);
        if self.clean_overlapping_matches || self.language_filter.is_some() {
            static BASE: crate::match_filters::BaseLanguageMatchFilter =
                crate::match_filters::BaseLanguageMatchFilter;
            let language_filter: &dyn LanguageMatchFilter =
                self.language_filter.as_deref().unwrap_or(&BASE);
            language_filter.filter_rule_matches(&mut all_matches, text);
            if self.clean_overlapping_matches {
                all_matches = crate::match_filters::clean_overlapping_filter(
                    all_matches,
                    text,
                    language_filter,
                );
            }
        }

        let sentence_ranges_json: Vec<Vec<usize>> = sentence_ranges
            .iter()
            .map(|r| vec![r.start, r.end])
            .collect();

        let extended_sentence_ranges: Vec<ExtendedSentenceRange> = sentence_ranges
            .iter()
            .map(|r| {
                let text = &text[r.start..r.end];
                let second_char_is_space = text
                    .char_indices()
                    .nth(1)
                    .is_some_and(|(i, c)| c == ' ' && i == 1);
                let leading = if second_char_is_space {
                    text.len() - text.trim_start().len()
                } else {
                    0
                };
                let trimmed_len = text.trim().len();
                ExtendedSentenceRange {
                    from_offset: r.start + leading,
                    to: r.start + leading + trimmed_len,
                    detected_languages: vec![DetectedLanguageRate {
                        language: request.language.short_code().to_string(),
                        rate: 1.0,
                    }],
                }
            })
            .collect();

        let language_info = LanguageInfo {
            code: request.language.code().to_string(),
            name: request.language.name().to_string(),
            detected: LanguageDetectedInfo {
                name: request.language.name().to_string(),
                code: request.language.code().to_string(),
                confidence: 1.0,
                source: None,
            },
        };

        CheckResult {
            software: SoftwareInfo::default(),
            warnings: Warnings::default(),
            language: language_info,
            matches: all_matches,
            sentence_ranges: sentence_ranges_json,
            extended_sentence_ranges,
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod rule_enabled_tests {
    use super::*;

    fn request() -> CheckRequest {
        CheckRequest::new("x", Language::English)
    }

    #[test]
    fn default_on_rules_run_without_parameters() {
        assert!(request().is_rule_enabled("SOME_RULE", "GRAMMAR", true, false));
    }

    #[test]
    fn default_off_rules_stay_off_even_at_picky_level() {
        assert!(!request().is_rule_enabled("STYLE_RULE", "STYLE", false, false));
        let mut picky = request();
        picky.picky = true;
        assert!(!picky.is_rule_enabled("STYLE_RULE", "STYLE", false, false));
        let mut leveled = request();
        leveled.level = Some("picky".to_string());
        assert!(!leveled.is_rule_enabled("STYLE_RULE", "STYLE", false, false));
    }

    #[test]
    fn default_off_rules_run_when_explicitly_enabled() {
        let mut req = request();
        req.enabled_rules = vec!["STYLE_RULE".to_string()];
        assert!(req.is_rule_enabled("STYLE_RULE", "STYLE", false, false));
    }

    #[test]
    fn picky_tagged_default_on_rules_need_picky_level() {
        assert!(!request().is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true));
        let mut picky = request();
        picky.picky = true;
        assert!(picky.is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true));
        let mut leveled = request();
        leveled.level = Some("picky".to_string());
        assert!(leveled.is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true));
        let mut enabled = request();
        enabled.enabled_rules = vec!["DASH_RULE".to_string()];
        assert!(
            !enabled.is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true),
            "JLanguageTool.isRuleActiveForLevelAndToneTags suppresses picky-tagged rules at Level.DEFAULT even when enabledRules lists them"
        );
    }

    #[test]
    fn picky_tagged_default_on_rules_run_at_named_levels() {
        let mut academic = request();
        academic.level = Some("academic".to_string());
        assert!(academic.is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true));
        let mut explicit_default = request();
        explicit_default.level = Some("default".to_string());
        assert!(!explicit_default.is_rule_enabled("DASH_RULE", "TYPOGRAPHY", true, true));
    }

    #[test]
    fn picky_tagged_default_off_rules_need_explicit_enabling() {
        let mut picky = request();
        picky.level = Some("picky".to_string());
        assert!(
            !picky.is_rule_enabled("STYLE_RULE", "STYLE", false, true),
            "Java core never auto-enables default-off rules, not even at picky level"
        );
        picky.enabled_rules = vec!["STYLE_RULE".to_string()];
        assert!(picky.is_rule_enabled("STYLE_RULE", "STYLE", false, true));
    }

    #[test]
    fn disabled_rules_suppress_matches() {
        let mut req = request();
        req.disabled_rules = vec!["SOME_RULE".to_string()];
        assert!(!req.is_rule_enabled("SOME_RULE", "GRAMMAR", true, false));
        assert!(req.is_rule_enabled("OTHER_RULE", "GRAMMAR", true, false));
    }

    #[test]
    fn enabled_rules_restrict_to_the_listed_set() {
        let mut req = request();
        req.enabled_rules = vec!["STYLE_RULE".to_string()];
        assert!(!req.is_rule_enabled("SOME_RULE", "GRAMMAR", true, false));
        assert!(req.is_rule_enabled("STYLE_RULE", "STYLE", false, false));
    }

    #[test]
    fn category_filters_apply() {
        let mut req = request();
        req.disabled_categories = vec!["STYLE".to_string()];
        assert!(!req.is_rule_enabled("STYLE_RULE", "STYLE", true, false));
        assert!(req.is_rule_enabled("SOME_RULE", "GRAMMAR", true, false));

        let mut req = request();
        req.enabled_categories = vec!["GRAMMAR".to_string()];
        assert!(!req.is_rule_enabled("STYLE_RULE", "STYLE", true, false));
        assert!(req.is_rule_enabled("SOME_RULE", "GRAMMAR", true, false));
    }
}

#[cfg(test)]
mod pipeline_order_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    struct RecordingTagger(usize);
    impl Tagger for RecordingTagger {
        fn tag(&self, _tokens: &mut [AnalyzedTokenReadings]) {
            assert_eq!(
                SEQUENCE.fetch_add(1, Ordering::SeqCst),
                self.0,
                "tagger must run at its pipeline position"
            );
        }
    }

    struct RecordingDisambiguator(usize);
    impl Disambiguator for RecordingDisambiguator {
        fn disambiguate(&self, _sentence: &mut AnalyzedSentence) {
            assert_eq!(
                SEQUENCE.fetch_add(1, Ordering::SeqCst),
                self.0,
                "disambiguator must run after the chunker"
            );
        }
    }

    struct RecordingChunker(usize);
    impl Chunker for RecordingChunker {
        fn add_chunk_tags(&self, _sentence: &mut AnalyzedSentence) {
            assert_eq!(
                SEQUENCE.fetch_add(1, Ordering::SeqCst),
                self.0,
                "chunker must run before the disambiguator (JLanguageTool.java:1806-1808)"
            );
        }
    }

    struct NoSplit;
    impl SentenceTokenizer for NoSplit {
        fn split(&self, text: &str) -> Vec<SentenceRange> {
            vec![SentenceRange {
                start: 0,
                end: text.len(),
            }]
        }
    }

    struct NoTokenize;
    impl WordTokenizer for NoTokenize {
        fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
            vec![AnalyzedTokenReadings::new(AnalyzedToken::new(
                text,
                offset,
                offset + text.len(),
            ))]
        }
    }

    struct RecordingPostChunker(usize);
    impl Chunker for RecordingPostChunker {
        fn add_chunk_tags(&self, _sentence: &mut AnalyzedSentence) {
            assert_eq!(
                SEQUENCE.fetch_add(1, Ordering::SeqCst),
                self.0,
                "post chunker must run after the disambiguator"
            );
        }
    }

    #[test]
    fn checker_runs_tag_chunk_disambiguate_then_post_chunk() {
        SEQUENCE.store(0, Ordering::SeqCst);
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(NoSplit))
            .with_word_tokenizer(Arc::new(NoTokenize))
            .with_tagger(Arc::new(RecordingTagger(0)))
            .with_chunker(Arc::new(RecordingChunker(1)))
            .with_disambiguator(Arc::new(RecordingDisambiguator(2)))
            .with_post_chunker(Arc::new(RecordingPostChunker(3)));
        let request = CheckRequest::new("hi", Language::English);
        checker.check(&request);
        assert_eq!(SEQUENCE.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn checker_works_without_chunker() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(NoSplit))
            .with_word_tokenizer(Arc::new(NoTokenize));
        let request = CheckRequest::new("hi", Language::English);
        let result = checker.check(&request);
        assert!(result.matches.is_empty());
    }
}

#[cfg(test)]
mod check_flow_tests {
    use super::*;
    use crate::{
        Category, IssueType, RuleMatchContext, RuleMatchRule, match_filters::base_priority_for_id,
    };

    struct SplitAt(usize);
    impl SentenceTokenizer for SplitAt {
        fn split(&self, text: &str) -> Vec<SentenceRange> {
            let mid = self.0.min(text.len());
            vec![
                SentenceRange { start: 0, end: mid },
                SentenceRange {
                    start: mid,
                    end: text.len(),
                },
            ]
        }
    }

    struct FixedRanges(Vec<SentenceRange>);
    impl SentenceTokenizer for FixedRanges {
        fn split(&self, _text: &str) -> Vec<SentenceRange> {
            self.0.clone()
        }
    }

    struct WordSplit;
    impl WordTokenizer for WordSplit {
        fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
            let mut tokens = Vec::new();
            let mut pos = 0;
            for word in text.split(' ') {
                if !word.is_empty() {
                    tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                        word,
                        offset + pos,
                        offset + pos + word.len(),
                    )));
                }
                pos += word.len() + 1;
            }
            tokens
        }
    }

    struct FixedRule {
        id: &'static str,
        category: Category,
        default_on: bool,
    }

    impl FixedRule {
        fn on(id: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id,
                category: Category::new("GRAMMAR", "Grammar"),
                default_on: true,
            })
        }

        fn off(id: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id,
                category: Category::new("STYLE", "Style"),
                default_on: false,
            })
        }

        fn in_category(id: &'static str, category: (&'static str, &'static str)) -> Arc<Self> {
            Arc::new(Self {
                id,
                category: Category::new(category.0, category.1),
                default_on: true,
            })
        }
    }

    impl Rule for FixedRule {
        fn id(&self) -> &str {
            self.id
        }

        fn is_default_on(&self) -> bool {
            self.default_on
        }

        fn category(&self) -> Category {
            self.category.clone()
        }

        fn issue_type(&self) -> IssueType {
            IssueType::Grammar
        }

        fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
            sentence
                .non_whitespace_tokens()
                .iter()
                .filter(|t| t.token().token() != "<S>")
                .map(|t| {
                    let start = t.token().start();
                    let length = t.token().token().len();
                    RuleMatch::new(
                        self.id,
                        start,
                        length,
                        RuleMatchRule::new(self.id, "fixed"),
                        RuleMatchContext::new(t.token().token(), start, length),
                    )
                })
                .collect()
        }
    }

    struct SentenceShapeInspector;
    impl Rule for SentenceShapeInspector {
        fn id(&self) -> &str {
            "SHAPE"
        }

        fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
            let tokens = sentence.tokens();
            assert!(!tokens.is_empty(), "checker must insert the <S> token");
            assert_eq!(tokens[0].token().token(), "<S>");
            assert!(tokens[0].is_sentence_start());
            assert_eq!(tokens[0].token().start(), sentence.start());
            assert!(
                tokens
                    .iter()
                    .filter(|t| t.is_whitespace())
                    .all(|t| !t.is_sentence_end())
            );
            let last_word = tokens
                .iter()
                .filter(|t| !t.is_whitespace() && t.token().token() != "<S>")
                .next_back()
                .expect("at least one word token");
            assert!(last_word.is_sentence_end());
            Vec::new()
        }
    }

    struct WhitespaceOnlyInspector;
    impl Rule for WhitespaceOnlyInspector {
        fn id(&self) -> &str {
            "WS"
        }

        fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
            assert_eq!(sentence.tokens().len(), 1, "only the <S> token remains");
            assert!(!sentence.tokens()[0].is_sentence_end());
            Vec::new()
        }
    }

    struct AssertingTextRule {
        id: &'static str,
        category: Category,
        expected_text: &'static str,
        expected_sentences: usize,
        inspect_snapshot: bool,
    }

    impl TextLevelRule for AssertingTextRule {
        fn id(&self) -> &str {
            self.id
        }

        fn category(&self) -> Category {
            self.category.clone()
        }

        fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
            assert_eq!(text, self.expected_text);
            assert_eq!(sentences.len(), self.expected_sentences);
            if self.inspect_snapshot {
                let sentence = &sentences[0];
                assert_eq!(sentence.tokens()[0].token().token(), "CHANGED");
                assert_eq!(
                    sentence.pre_disambiguation_tokens()[1].token().token(),
                    "one"
                );
            }
            vec![RuleMatch::new(
                self.id,
                0,
                4,
                RuleMatchRule::new(self.id, "text"),
                RuleMatchContext::new(text.get(0..4).unwrap_or(""), 0, 4),
            )]
        }
    }

    struct ReplaceWithChanged;
    impl Disambiguator for ReplaceWithChanged {
        fn disambiguate(&self, sentence: &mut AnalyzedSentence) {
            let start = sentence.start();
            sentence.set_tokens(vec![AnalyzedTokenReadings::new(AnalyzedToken::new(
                "CHANGED",
                start,
                start + 7,
            ))]);
        }
    }

    struct PriorityFilter;
    impl LanguageMatchFilter for PriorityFilter {
        fn priority_for_id(&self, id: &str) -> i32 {
            match id {
                "HIGH" => 5,
                _ => base_priority_for_id(id),
            }
        }
    }

    struct SuffixFilter;
    impl LanguageMatchFilter for SuffixFilter {
        fn filter_rule_matches(&self, matches: &mut [RuleMatch], _text: &str) {
            for m in matches {
                m.set_message(format!("{}-filtered", m.message()));
            }
        }
    }

    fn ids(result: &CheckResult) -> Vec<&str> {
        let mut ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn check_request_defaults_and_category_helpers() {
        let request = CheckRequest::new("text", Language::EnglishUs);
        assert_eq!(request.text, "text");
        assert_eq!(request.language, Language::EnglishUs);
        assert!(request.mother_tongue.is_none());
        assert!(request.enabled_rules.is_empty());
        assert!(request.disabled_rules.is_empty());
        assert!(request.enabled_categories.is_empty());
        assert!(request.disabled_categories.is_empty());
        assert!(request.level.is_none());
        assert!(!request.picky);
        assert!(!request.run_all_rules);
        assert!(request.category_enabled("ANY"));
        assert!(!request.has_category_filters());

        let mut request = CheckRequest::new("text", Language::EnglishUs);
        request.disabled_categories = vec!["STYLE".to_string()];
        assert!(!request.category_enabled("STYLE"));
        assert!(request.category_enabled("GRAMMAR"));
        assert!(request.has_category_filters());

        let mut request = CheckRequest::new("text", Language::EnglishUs);
        request.enabled_categories = vec!["GRAMMAR".to_string()];
        assert!(!request.category_enabled("STYLE"));
        assert!(request.category_enabled("GRAMMAR"));
        assert!(request.has_category_filters());
    }

    #[test]
    fn rule_enabled_by_id_direct_flags() {
        let mut run_all = CheckRequest::new("t", Language::English);
        run_all.run_all_rules = true;
        assert!(run_all.rule_enabled_by_id("ANY", false, true));
        run_all.disabled_rules = vec!["ANY".to_string()];
        assert!(
            !run_all.rule_enabled_by_id("ANY", false, true),
            "disabled rules win over run_all_rules"
        );
        let plain = CheckRequest::new("t", Language::English);
        assert!(plain.rule_enabled_by_id("ON", true, false));
        assert!(!plain.rule_enabled_by_id("OFF", false, false));
        assert!(!plain.rule_enabled_by_id("PICKY", true, true));
    }

    #[test]
    fn check_reports_one_full_range_without_a_sentence_tokenizer() {
        let checker = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .add_rule(FixedRule::on("R"));
        let result = checker.check(&CheckRequest::new("hello", Language::English));
        assert_eq!(result.sentence_ranges, vec![vec![0, 5]]);
        assert_eq!(ids(&result), vec!["R"]);
        assert_eq!(result.matches[0].offset(), 0);
        assert_eq!(result.matches[0].length(), 5);
    }

    #[test]
    fn check_splits_sentences_and_offsets_matches_absolutely() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SplitAt(8)))
            .with_word_tokenizer(Arc::new(WordSplit))
            .add_rule(FixedRule::on("R"));
        let result = checker.check(&CheckRequest::new("one two three", Language::English));
        assert_eq!(result.sentence_ranges, vec![vec![0, 8], vec![8, 13]]);
        let mut spans: Vec<(usize, usize)> = result
            .matches
            .iter()
            .map(|m| (m.offset(), m.length()))
            .collect();
        spans.sort_unstable();
        assert_eq!(spans, vec![(0, 3), (4, 3), (8, 5)]);
    }

    #[test]
    fn check_marks_sentence_start_and_end() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SplitAt(10)))
            .with_word_tokenizer(Arc::new(WordSplit))
            .add_rule(Arc::new(SentenceShapeInspector));
        checker.check(&CheckRequest::new("hi there. bye now.", Language::English));
    }

    #[test]
    fn check_skips_sent_end_for_whitespace_only_input() {
        let checker = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .add_rule(Arc::new(WhitespaceOnlyInspector));
        checker.check(&CheckRequest::new("   ", Language::English));
    }

    #[test]
    fn check_filters_rules_by_enabled_and_disabled_lists() {
        let checker = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .with_clean_overlapping_matches(false)
            .add_rule(FixedRule::on("ON"))
            .add_rule(FixedRule::off("OFF"));

        let plain = checker.check(&CheckRequest::new("word", Language::English));
        assert_eq!(ids(&plain), vec!["ON"]);

        let mut run_all = CheckRequest::new("word", Language::English);
        run_all.run_all_rules = true;
        assert_eq!(ids(&checker.check(&run_all)), vec!["OFF", "ON"]);

        let mut only_off = CheckRequest::new("word", Language::English);
        only_off.enabled_rules = vec!["OFF".to_string()];
        assert_eq!(ids(&checker.check(&only_off)), vec!["OFF"]);

        let mut disabled = CheckRequest::new("word", Language::English);
        disabled.disabled_rules = vec!["ON".to_string()];
        assert_eq!(ids(&checker.check(&disabled)), Vec::<&str>::new());
    }

    #[test]
    fn check_applies_category_filters_to_rules() {
        let checker = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .with_clean_overlapping_matches(false)
            .add_rule(FixedRule::in_category("G", ("GRAMMAR", "Grammar")))
            .add_rule(FixedRule::in_category("S", ("STYLE", "Style")));

        let mut allow_grammar = CheckRequest::new("word", Language::English);
        allow_grammar.enabled_categories = vec!["GRAMMAR".to_string()];
        assert_eq!(ids(&checker.check(&allow_grammar)), vec!["G"]);

        let mut deny_style = CheckRequest::new("word", Language::English);
        deny_style.disabled_categories = vec!["STYLE".to_string()];
        assert_eq!(ids(&checker.check(&deny_style)), vec!["G"]);
    }

    #[test]
    fn check_runs_text_level_rules_over_the_whole_text() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SplitAt(9)))
            .add_text_level_rule(Arc::new(AssertingTextRule {
                id: "TL",
                category: Category::new("GRAMMAR", "Grammar"),
                expected_text: "one two. three.",
                expected_sentences: 2,
                inspect_snapshot: false,
            }));
        let result = checker.check(&CheckRequest::new("one two. three.", Language::English));
        assert_eq!(ids(&result), vec!["TL"]);
        assert_eq!(result.matches[0].offset(), 0);
        assert_eq!(result.matches[0].length(), 4);
    }

    #[test]
    fn check_applies_category_filters_to_text_level_rules() {
        let checker = Checker::new().add_text_level_rule(Arc::new(AssertingTextRule {
            id: "TL_STYLE",
            category: Category::new("STYLE", "Style"),
            expected_text: "word",
            expected_sentences: 1,
            inspect_snapshot: false,
        }));

        let mut allow_grammar = CheckRequest::new("word", Language::English);
        allow_grammar.enabled_categories = vec!["GRAMMAR".to_string()];
        assert_eq!(ids(&checker.check(&allow_grammar)), Vec::<&str>::new());

        assert_eq!(
            ids(&checker.check(&CheckRequest::new("word", Language::English))),
            vec!["TL_STYLE"]
        );
    }

    #[test]
    fn check_snapshots_tokens_before_disambiguation() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SplitAt(8)))
            .with_word_tokenizer(Arc::new(WordSplit))
            .with_disambiguator(Arc::new(ReplaceWithChanged))
            .add_text_level_rule(Arc::new(AssertingTextRule {
                id: "SNAP",
                category: Category::new("GRAMMAR", "Grammar"),
                expected_text: "one two three",
                expected_sentences: 2,
                inspect_snapshot: true,
            }));
        checker.check(&CheckRequest::new("one two three", Language::English));
    }

    #[test]
    fn check_prefers_higher_priority_overlaps_regardless_of_order() {
        let build = |a: &'static str, b: &'static str| {
            Checker::new()
                .with_word_tokenizer(Arc::new(WordSplit))
                .with_language_filter(Arc::new(PriorityFilter))
                .add_rule(FixedRule::on(a))
                .add_rule(FixedRule::on(b))
        };
        let result = build("HIGH", "LOW").check(&CheckRequest::new("word", Language::English));
        assert_eq!(ids(&result), vec!["HIGH"]);
        let result = build("LOW", "HIGH").check(&CheckRequest::new("word", Language::English));
        assert_eq!(ids(&result), vec!["HIGH"]);
    }

    #[test]
    fn check_runs_custom_language_filter_on_matches() {
        let checker = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .with_clean_overlapping_matches(false)
            .with_language_filter(Arc::new(SuffixFilter))
            .add_rule(FixedRule::on("R"));
        let result = checker.check(&CheckRequest::new("word", Language::English));
        assert!(
            result
                .matches
                .iter()
                .all(|m| m.message().ends_with("-filtered"))
        );
    }

    #[test]
    fn check_keeps_overlapping_matches_without_clean_filter() {
        let dirty = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .with_clean_overlapping_matches(false)
            .add_rule(FixedRule::on("A"))
            .add_rule(FixedRule::on("B"));
        let result = dirty.check(&CheckRequest::new("word", Language::English));
        assert_eq!(ids(&result), vec!["A", "B"]);

        let clean = Checker::new()
            .with_word_tokenizer(Arc::new(WordSplit))
            .add_rule(FixedRule::on("A"))
            .add_rule(FixedRule::on("B"));
        let result = clean.check(&CheckRequest::new("word", Language::English));
        assert_eq!(ids(&result), vec!["B"]);
    }

    #[test]
    fn check_reports_request_language_metadata() {
        let checker = Checker::new();
        let result = checker.check(&CheckRequest::new("salut", Language::French));
        assert_eq!(result.language.code, "fr");
        assert_eq!(result.language.name, "French");
        assert_eq!(result.language.detected.code, "fr");
        assert_eq!(result.language.detected.name, "French");
        assert_eq!(result.language.detected.confidence, 1.0);
        assert!(result.matches.is_empty());
    }

    #[test]
    fn extended_ranges_trim_whitespace_edges() {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(FixedRanges(vec![
                SentenceRange { start: 0, end: 7 },
                SentenceRange { start: 7, end: 11 },
            ])))
            .with_word_tokenizer(Arc::new(WordSplit));
        let result = checker.check(&CheckRequest::new("  Hi.  Bye.", Language::German));
        assert_eq!(result.sentence_ranges, vec![vec![0, 7], vec![7, 11]]);
        let first = &result.extended_sentence_ranges[0];
        assert_eq!(first.from_offset, 2);
        assert_eq!(
            first.to, 5,
            "Java adds whitespaceFix to the trimmed length (JLanguageTool.java:2195)"
        );
        assert_eq!(first.detected_languages[0].language, "de");
        assert_eq!(first.detected_languages[0].rate, 1.0);
        let second = &result.extended_sentence_ranges[1];
        assert_eq!(second.from_offset, 7);
        assert_eq!(second.to, 11);
    }

    #[test]
    fn rules_and_text_level_rules_accessors_track_registration() {
        let checker = Checker::new()
            .add_rule(FixedRule::on("A"))
            .add_rule(FixedRule::on("B"));
        assert_eq!(checker.rules().len(), 2);
        assert_eq!(checker.rules()[0].id(), "A");

        let checker = checker.set_rules(vec![FixedRule::on("C")]);
        assert_eq!(checker.rules().len(), 1);
        assert_eq!(checker.rules()[0].id(), "C");

        let text_rule: Arc<dyn TextLevelRule> = Arc::new(AssertingTextRule {
            id: "TL",
            category: Category::new("GRAMMAR", "Grammar"),
            expected_text: "t",
            expected_sentences: 1,
            inspect_snapshot: false,
        });
        let checker = checker.add_text_level_rule(text_rule);
        assert_eq!(checker.text_level_rules().len(), 1);
        assert_eq!(checker.text_level_rules()[0].id(), "TL");

        let checker = checker.set_text_level_rules(Vec::new());
        assert!(checker.text_level_rules().is_empty());
    }
}
