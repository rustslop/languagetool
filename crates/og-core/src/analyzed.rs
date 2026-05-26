use serde::{Deserialize, Serialize};
use crate::Token;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedToken {
    token: String,
    lemma: Option<String>,
    pos_tags: Vec<String>,
    start: usize,
    end: usize,
}

impl AnalyzedToken {
    pub fn new(token: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            token: token.into(),
            lemma: None,
            pos_tags: Vec::new(),
            start,
            end,
        }
    }

    pub fn from_token(token: &Token) -> Self {
        Self {
            token: token.text().to_string(),
            lemma: None,
            pos_tags: Vec::new(),
            start: token.start(),
            end: token.end(),
        }
    }

    pub fn with_lemma(mut self, lemma: impl Into<String>) -> Self {
        self.lemma = Some(lemma.into());
        self
    }

    pub fn with_pos_tags(mut self, tags: Vec<String>) -> Self {
        self.pos_tags = tags;
        self
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn lemma(&self) -> Option<&str> {
        self.lemma.as_deref()
    }

    pub fn pos_tags(&self) -> &[String] {
        &self.pos_tags
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn has_pos_tag(&self, tag: &str) -> bool {
        self.pos_tags.iter().any(|t| t == tag)
    }

    pub fn has_pos_tag_matching(&self, pattern: &str) -> bool {
        // Anchor the pattern to match the full string, like Java's String.matches()
        let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
            pattern.to_string()
        } else {
            format!("^(?:{})$", pattern)
        };
        let re = regex::Regex::new(&anchored).ok();
        match re {
            Some(re) => self.pos_tags.iter().any(|t| re.is_match(t)),
            None => self.pos_tags.iter().any(|t| t == pattern),
        }
    }

    pub fn set_pos_tags(&mut self, tags: Vec<String>) {
        self.pos_tags = tags;
    }

    pub fn set_lemma(&mut self, lemma: Option<String>) {
        self.lemma = lemma;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedTokenReadings {
    token: AnalyzedToken,
    readings: Vec<AnalyzedToken>,
    is_whitespace: bool,
    chunk: Option<String>,
}

impl AnalyzedTokenReadings {
    pub fn new(token: AnalyzedToken) -> Self {
        let is_whitespace = token.token.chars().all(|c| c.is_whitespace());
        Self {
            token: token.clone(),
            readings: vec![token],
            is_whitespace,
            chunk: None,
        }
    }

    pub fn with_readings(mut self, readings: Vec<AnalyzedToken>) -> Self {
        self.readings = readings;
        self
    }

    pub fn token(&self) -> &AnalyzedToken {
        &self.token
    }

    pub fn readings(&self) -> &[AnalyzedToken] {
        &self.readings
    }

    pub fn readings_mut(&mut self) -> &mut Vec<AnalyzedToken> {
        &mut self.readings
    }

    pub fn token_mut(&mut self) -> &mut AnalyzedToken {
        &mut self.token
    }

    pub fn is_whitespace(&self) -> bool {
        self.is_whitespace
    }

    pub fn first_reading(&self) -> Option<&AnalyzedToken> {
        self.readings.first()
    }

    pub fn has_lemma(&self, lemma: &str) -> bool {
        self.readings.iter().any(|r| r.lemma.as_deref() == Some(lemma))
    }

    pub fn has_pos_tag(&self, tag: &str) -> bool {
        self.readings
            .iter()
            .any(|r| r.pos_tags.iter().any(|t| t == tag))
    }

    pub fn has_pos_tag_matching(&self, pattern: &str) -> bool {
        // Anchor the pattern to match the full string, like java's String.matches()
        let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
            pattern.to_string()
        } else {
            format!("^(?:{})$", pattern)
        };
        let re = regex::Regex::new(&anchored).ok();
        self.readings.iter().any(|r| match &re {
            Some(re) => r.pos_tags.iter().any(|t| re.is_match(t)),
            None => r.pos_tags.iter().any(|t| t == pattern),
        })
    }

    pub fn chunk(&self) -> Option<&str> {
        self.chunk.as_deref()
    }

    pub fn set_chunk(&mut self, chunk: Option<String>) {
        self.chunk = chunk;
    }

    /// Add a SENT_END reading to this token (like Java's setSentEnd()).
    /// Adds the tag as both a primary POS tag and as a new reading.
    pub fn add_sent_end(&mut self) {
        let lemma = self.token.lemma.clone().unwrap_or_else(|| self.token.token.to_string());
        let sent_end_token = AnalyzedToken::new(self.token.token.as_str(), self.token.start, self.token.end)
            .with_pos_tags(vec!["SENT_END".to_string()])
            .with_lemma(lemma);
        self.readings.push(sent_end_token);
        if !self.token.pos_tags.contains(&"SENT_END".to_string()) {
            self.token.pos_tags.push("SENT_END".to_string());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedSentence {
    text: String,
    start: usize,
    end: usize,
    tokens: Vec<AnalyzedTokenReadings>,
}

impl AnalyzedSentence {
    pub fn new(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            text: text.into(),
            start,
            end,
            tokens: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn tokens(&self) -> &[AnalyzedTokenReadings] {
        &self.tokens
    }

    pub fn set_tokens(&mut self, tokens: Vec<AnalyzedTokenReadings>) {
        self.tokens = tokens;
    }

    pub fn tokens_mut(&mut self) -> &mut Vec<AnalyzedTokenReadings> {
        &mut self.tokens
    }

    pub fn non_whitespace_tokens(&self) -> Vec<&AnalyzedTokenReadings> {
        self.tokens.iter().filter(|t| !t.is_whitespace()).collect()
    }
}
