use crate::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings,
    CheckResult, Language, LanguageDetectedInfo, LanguageInfo,
    RuleMatch, SoftwareInfo, Warnings,
    rule::Rule,
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
        }
    }

    pub fn is_rule_enabled(&self, rule_id: &str, category_id: &str, default_on: bool) -> bool {
        // If specific rules are enabled, only those run
        if !self.enabled_rules.is_empty() && !self.enabled_rules.contains(&rule_id.to_string()) {
            return false;
        }
        // Explicitly disabled rules never run
        if self.disabled_rules.contains(&rule_id.to_string()) {
            return false;
        }
        // If specific categories are enabled, only those run
        if !self.enabled_categories.is_empty() && !self.enabled_categories.contains(&category_id.to_string()) {
            return false;
        }
        // Explicitly disabled categories never run
        if self.disabled_categories.contains(&category_id.to_string()) {
            return false;
        }
        // Rules that are default-off only run if explicitly enabled or if picky mode is on
        let picky = self.picky || self.level.as_deref() == Some("picky");
        if !default_on && !picky && !self.enabled_rules.contains(&rule_id.to_string()) && !self.enabled_categories.contains(&category_id.to_string()) {
            return false;
        }
        true
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

pub struct Checker {
    sentence_tokenizer: Option<Arc<dyn SentenceTokenizer>>,
    word_tokenizer: Option<Arc<dyn WordTokenizer>>,
    tagger: Option<Arc<dyn Tagger>>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    rules: Vec<Arc<dyn Rule>>,
    text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

impl Checker {
    pub fn new() -> Self {
        Self {
            sentence_tokenizer: None,
            word_tokenizer: None,
            tagger: None,
            disambiguator: None,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
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

    pub fn rules(&self) -> &[Arc<dyn Rule>] {
        &self.rules
    }

    pub fn text_level_rules(&self) -> &[Arc<dyn TextLevelRule>] {
        &self.text_level_rules
    }

    pub fn check(&self, request: &CheckRequest) -> CheckResult {
        let text = &request.text;

        // Step 1: Split into sentences
        let sentence_ranges = if let Some(ref tokenizer) = self.sentence_tokenizer {
            tokenizer.split(text)
        } else {
            vec![SentenceRange { start: 0, end: text.len() }]
        };

        // Step 2: For each sentence, tokenize, tag, disambiguate, then apply rules
        let mut all_matches: Vec<RuleMatch> = Vec::new();
        let mut analyzed_sentences = Vec::new();

        for range in &sentence_ranges {
            let sentence_text = text[range.start..range.end].to_string();
            let mut sentence = AnalyzedSentence::new(&sentence_text, range.start, range.end);

            // Step 2a: Tokenize
            if let Some(ref tokenizer) = self.word_tokenizer {
                let mut tokens = tokenizer.tokenize(&sentence_text, range.start);
                // Add SENTENCE_START token
                let start_token = AnalyzedTokenReadings::new(
                    AnalyzedToken::new("<S>", range.start, range.start)
                        .with_pos_tags(vec!["SENT_START".to_string()])
                );
                tokens.insert(0, start_token);
                sentence.set_tokens(tokens);
            }

            // Step 2b: Tag
            if let Some(ref tagger) = self.tagger {
                let tokens = &mut sentence.tokens_mut();
                tagger.tag(tokens);
            }

            // Step 2b2: Add SENT_END tag to last non-whitespace token (like Java's setSentEnd())
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

            // Step 2c: Disambiguate
            if let Some(ref disambiguator) = self.disambiguator {
                disambiguator.disambiguate(&mut sentence);
            }

            // Step 2d: Apply sentence-level rules
            for rule in &self.rules {
                if !request.is_rule_enabled(rule.id(), &rule.category().id().as_str().to_string(), rule.is_default_on()) {
                    continue;
                }
                let matches = rule.match_sentence(&sentence);
                all_matches.extend(matches);
            }

            analyzed_sentences.push(sentence);
        }

        // Step 3: Apply text-level rules across all sentences
        for rule in &self.text_level_rules {
            if !request.is_rule_enabled(rule.id(), &rule.category().id().as_str().to_string(), rule.is_default_on()) {
                continue;
            }
            let matches = rule.match_text(text, &analyzed_sentences);
            all_matches.extend(matches);
        }

        // Step 4: Sort matches by offset
        all_matches.sort_by_key(|m| m.offset());

        // Step 5: Build sentence ranges for response
        let sentence_ranges_json: Vec<Vec<usize>> = sentence_ranges
            .iter()
            .map(|r| vec![r.start, r.end])
            .collect();

        let language_info = LanguageInfo {
            code: request.language.code().to_string(),
            name: request.language.name().to_string(),
            detected: LanguageDetectedInfo {
                code: request.language.code().to_string(),
                name: request.language.name().to_string(),
                preferred: None,
                detected_by: None,
                confidence: None,
            },
        };

        CheckResult {
            software: SoftwareInfo::default(),
            warnings: Warnings::default(),
            language: language_info,
            matches: all_matches,
            sentence_ranges: sentence_ranges_json,
            extended_sentence_ranges: Vec::new(),
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export trait names that don't conflict with lang-module traits

