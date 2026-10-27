use crate::Token;
use serde::{Deserialize, Serialize};

pub const TYPOGRAPHIC_APOSTROPHE_CHUNK: &str = "containsTypographicApostrophe";

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
    #[serde(default)]
    chunks: Vec<String>,
    #[serde(default)]
    is_ignored_by_speller: bool,
    #[serde(default)]
    is_immunized: bool,
}

impl AnalyzedTokenReadings {
    pub fn new(token: AnalyzedToken) -> Self {
        let is_whitespace = token.token.chars().all(|c| c.is_whitespace());
        Self {
            token: token.clone(),
            readings: vec![token],
            is_whitespace,
            chunks: Vec::new(),
            is_ignored_by_speller: false,
            is_immunized: false,
        }
    }

    pub fn with_readings(mut self, readings: Vec<AnalyzedToken>) -> Self {
        self.readings = readings;
        self
    }

    pub fn token(&self) -> &AnalyzedToken {
        &self.token
    }

    pub fn into_readings(self) -> Vec<AnalyzedToken> {
        self.readings
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

    pub fn ignore_spelling(&mut self) {
        self.is_ignored_by_speller = true;
    }

    pub fn is_ignored_by_speller(&self) -> bool {
        self.is_ignored_by_speller
    }

    pub fn immunize(&mut self) {
        self.is_immunized = true;
    }

    pub fn is_immunized(&self) -> bool {
        self.is_immunized
    }

    pub fn first_reading(&self) -> Option<&AnalyzedToken> {
        self.readings.first()
    }

    pub fn has_lemma(&self, lemma: &str) -> bool {
        self.readings
            .iter()
            .any(|r| r.lemma.as_deref() == Some(lemma))
    }

    pub fn has_pos_tag(&self, tag: &str) -> bool {
        self.readings
            .iter()
            .any(|r| r.pos_tags.iter().any(|t| t == tag))
    }

    pub fn has_pos_tag_matching(&self, pattern: &str) -> bool {
        use std::collections::HashMap;
        use std::sync::LazyLock;
        use std::sync::Mutex;

        static REGEX_CACHE: LazyLock<Mutex<HashMap<String, regex::Regex>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));

        let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
            pattern.to_string()
        } else {
            format!("^(?:{})$", pattern)
        };

        {
            let cache = REGEX_CACHE.lock().unwrap();
            if let Some(re) = cache.get(&anchored) {
                return self
                    .readings
                    .iter()
                    .any(|r| r.pos_tags.iter().any(|t| re.is_match(t)));
            }
        }

        match regex::Regex::new(&anchored) {
            Ok(re) => {
                let result = self
                    .readings
                    .iter()
                    .any(|r| r.pos_tags.iter().any(|t| re.is_match(t)));
                let mut cache = REGEX_CACHE.lock().unwrap();
                cache.entry(anchored).or_insert(re);
                result
            }
            Err(_) => self
                .readings
                .iter()
                .any(|r| r.pos_tags.iter().any(|t| t == pattern)),
        }
    }

    pub fn has_pos_tag_matching_regex(&self, re: &regex::Regex) -> bool {
        self.readings
            .iter()
            .any(|r| r.pos_tags.iter().any(|t| re.is_match(t)))
    }

    pub fn chunk(&self) -> Option<&str> {
        self.chunks.first().map(String::as_str)
    }

    pub fn chunks(&self) -> &[String] {
        &self.chunks
    }

    pub fn has_chunk(&self, chunk: &str) -> bool {
        self.chunks.iter().any(|c| c == chunk)
    }

    pub fn has_chunk_matching(&self, re: &regex::Regex) -> bool {
        self.chunks.iter().any(|c| re.is_match(c))
    }

    pub fn set_chunk(&mut self, chunk: Option<String>) {
        self.chunks = chunk.into_iter().collect();
    }

    pub fn set_chunks(&mut self, chunks: Vec<String>) {
        self.chunks = chunks;
    }

    // Java keeps the typographic-apostrophe marker as a dedicated ATR flag; we encode it as a chunk tag.
    pub fn mark_typographic_apostrophe(&mut self) {
        if !self.has_chunk(TYPOGRAPHIC_APOSTROPHE_CHUNK) {
            self.chunks.push(TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string());
        }
    }

    pub fn has_typographic_apostrophe(&self) -> bool {
        self.has_chunk(TYPOGRAPHIC_APOSTROPHE_CHUNK)
    }

    pub fn replace_chunk_tags_preserving_apostrophe_flag(&mut self, chunks: Vec<String>) {
        let keep = self.has_typographic_apostrophe()
            && !chunks.iter().any(|c| c == TYPOGRAPHIC_APOSTROPHE_CHUNK);
        self.chunks = chunks;
        if keep {
            self.chunks.push(TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string());
        }
    }

    pub fn add_sent_end(&mut self) {
        let lemma = self
            .token
            .lemma
            .clone()
            .unwrap_or_else(|| self.token.token.to_string());
        let sent_end_token =
            AnalyzedToken::new(self.token.token.as_str(), self.token.start, self.token.end)
                .with_pos_tags(vec!["SENT_END".to_string()])
                .with_lemma(lemma);
        if self
            .readings
            .last()
            .is_some_and(|last| last.pos_tags.is_empty())
        {
            self.readings.pop();
        }
        self.readings.push(sent_end_token);
        if !self.token.pos_tags.contains(&"SENT_END".to_string()) {
            self.token.pos_tags.push("SENT_END".to_string());
        }
    }

    pub fn is_sentence_start(&self) -> bool {
        self.token.pos_tags.iter().any(|t| t == "SENT_START")
            || self
                .readings
                .iter()
                .any(|r| r.pos_tags.iter().any(|t| t == "SENT_START"))
    }

    pub fn is_sentence_end(&self) -> bool {
        self.token.pos_tags.iter().any(|t| t == "SENT_END")
            || self
                .readings
                .iter()
                .any(|r| r.pos_tags.iter().any(|t| t == "SENT_END"))
    }

    pub fn all_pos_tags(&self) -> Vec<&str> {
        let mut tags: Vec<&str> = Vec::new();
        for r in &self.readings {
            for t in &r.pos_tags {
                if !tags.contains(&t.as_str()) {
                    tags.push(t);
                }
            }
        }
        tags
    }

    pub fn all_lemmas(&self) -> Vec<&str> {
        self.readings
            .iter()
            .filter_map(|r| r.lemma.as_deref())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedSentence {
    text: String,
    start: usize,
    end: usize,
    tokens: Vec<AnalyzedTokenReadings>,
    pre_disambiguation_tokens: Option<Vec<AnalyzedTokenReadings>>,
}

impl AnalyzedSentence {
    pub fn new(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            text: text.into(),
            start,
            end,
            tokens: Vec::new(),
            pre_disambiguation_tokens: None,
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

    pub fn set_pre_disambiguation_tokens(&mut self, tokens: Vec<AnalyzedTokenReadings>) {
        self.pre_disambiguation_tokens = Some(tokens);
    }

    pub fn pre_disambiguation_tokens(&self) -> &[AnalyzedTokenReadings] {
        self.pre_disambiguation_tokens
            .as_ref()
            .unwrap_or(&self.tokens)
    }

    pub fn pre_disambiguation_non_whitespace_tokens(&self) -> Vec<&AnalyzedTokenReadings> {
        self.pre_disambiguation_tokens()
            .iter()
            .filter(|t| !t.is_whitespace())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare(token: &str) -> AnalyzedToken {
        AnalyzedToken::new(token, 0, token.len())
    }

    fn reading(token: &str, lemma: Option<&str>, pos_tags: &[&str]) -> AnalyzedTokenReadings {
        let mut readings = AnalyzedTokenReadings::new(bare(token));
        let tag_strings: Vec<String> = pos_tags.iter().map(|t| t.to_string()).collect();
        readings.token_mut().set_pos_tags(tag_strings.clone());
        let analyzed = AnalyzedToken::new(token, 0, token.len())
            .with_pos_tags(tag_strings)
            .with_lemma(lemma.unwrap_or(token));
        readings.with_readings(vec![analyzed])
    }

    #[test]
    fn analyzed_token_starts_bare() {
        let analyzed = AnalyzedToken::new("dog", 2, 5);
        assert_eq!(analyzed.token(), "dog");
        assert_eq!(analyzed.lemma(), None);
        assert!(analyzed.pos_tags().is_empty());
        assert_eq!(analyzed.start(), 2);
        assert_eq!(analyzed.end(), 5);
        assert!(!analyzed.has_pos_tag("NN"));
    }

    #[test]
    fn analyzed_token_copies_positions_from_token() {
        let raw = Token::new("cat", 4, 7);
        let analyzed = AnalyzedToken::from_token(&raw);
        assert_eq!(analyzed.token(), "cat");
        assert_eq!(analyzed.start(), 4);
        assert_eq!(analyzed.end(), 7);
    }

    #[test]
    fn setters_replace_lemma_and_pos_tags() {
        let mut analyzed = bare("run")
            .with_lemma("run")
            .with_pos_tags(vec!["VB".to_string()]);
        assert_eq!(analyzed.lemma(), Some("run"));
        assert!(analyzed.has_pos_tag("VB"));
        analyzed.set_lemma(Some("sprint".to_string()));
        analyzed.set_pos_tags(vec!["VBP".to_string()]);
        assert_eq!(analyzed.lemma(), Some("sprint"));
        assert!(!analyzed.has_pos_tag("VB"));
        assert!(analyzed.has_pos_tag("VBP"));
        analyzed.set_lemma(None);
        assert_eq!(analyzed.lemma(), None);
    }

    #[test]
    fn token_pos_tag_matching_anchors_free_patterns() {
        let analyzed = bare("dogs").with_pos_tags(vec!["NNS".to_string(), "VBZ".to_string()]);
        assert!(analyzed.has_pos_tag_matching("NN.*"));
        assert!(analyzed.has_pos_tag_matching("NNS|XX"));
        assert!(!analyzed.has_pos_tag_matching("NN|XX"));
        assert!(analyzed.has_pos_tag_matching("^VBZ$"));
    }

    #[test]
    fn token_pos_tag_matching_invalid_pattern_falls_back_to_equality() {
        let analyzed = bare("x").with_pos_tags(vec!["(".to_string(), "NN".to_string()]);
        assert!(analyzed.has_pos_tag_matching("("));
        assert!(!analyzed.has_pos_tag_matching("N"));
    }

    #[test]
    fn readings_detect_whitespace_like_java_is_blank() {
        assert!(AnalyzedTokenReadings::new(bare(" ")).is_whitespace());
        assert!(AnalyzedTokenReadings::new(bare(" \n\t")).is_whitespace());
        assert!(!reading("dog", Some("dog"), &["NN"]).is_whitespace());
    }

    #[test]
    fn readings_expose_first_and_owned_readings() {
        let atr = reading("dog", Some("dog"), &["NN"]);
        assert_eq!(atr.first_reading().unwrap().lemma(), Some("dog"));
        assert_eq!(atr.readings().len(), 1);
        let owned = atr.into_readings();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].token(), "dog");
    }

    #[test]
    fn readings_mut_and_token_mut_allow_in_place_edits() {
        let mut atr = reading("dog", Some("dog"), &["NN"]);
        atr.readings_mut().push(AnalyzedToken::new("dog", 0, 3));
        assert_eq!(atr.readings().len(), 2);
        atr.token_mut().set_lemma(Some("hound".to_string()));
        assert_eq!(atr.token().lemma(), Some("hound"));
    }

    #[test]
    fn has_lemma_and_has_pos_tag_search_every_reading() {
        let mut atr = AnalyzedTokenReadings::new(bare("flies"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("fly", 0, 5)
                .with_lemma("fly")
                .with_pos_tags(vec!["VBP".to_string()]),
            AnalyzedToken::new("fly", 0, 5)
                .with_lemma("fly")
                .with_pos_tags(vec!["NNS".to_string()]),
        ]);
        assert!(atr.has_lemma("fly"));
        assert!(!atr.has_lemma("flight"));
        assert!(atr.has_pos_tag("VBP"));
        assert!(atr.has_pos_tag("NNS"));
        assert!(!atr.has_pos_tag("JJ"));
    }

    #[test]
    fn readings_pos_tag_matching_uses_regex_semantics() {
        let mut atr = AnalyzedTokenReadings::new(bare("dogs"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("dog", 0, 4).with_pos_tags(vec!["NNS".to_string()]),
        ]);
        assert!(atr.has_pos_tag_matching("NN.*"));
        assert!(atr.has_pos_tag_matching("NN.*"));
        assert!(!atr.has_pos_tag_matching("^VB"));
    }

    #[test]
    fn readings_pos_tag_matching_invalid_pattern_falls_back_to_equality() {
        let mut atr = AnalyzedTokenReadings::new(bare("x"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("x", 0, 1).with_pos_tags(vec!["[".to_string()]),
        ]);
        assert!(atr.has_pos_tag_matching("["));
        assert!(!atr.has_pos_tag_matching("x"));
    }

    #[test]
    fn has_pos_tag_matching_regex_uses_the_compiled_regex() {
        let mut atr = AnalyzedTokenReadings::new(bare("dogs"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("dog", 0, 4).with_pos_tags(vec!["NNS".to_string()]),
        ]);
        let re = regex::Regex::new("^N").unwrap();
        assert!(atr.has_pos_tag_matching_regex(&re));
        let re = regex::Regex::new("^V").unwrap();
        assert!(!atr.has_pos_tag_matching_regex(&re));
    }

    #[test]
    fn chunk_accessors_manage_the_chunk_list() {
        let mut atr = reading("dog", Some("dog"), &["NN"]);
        assert!(atr.chunks().is_empty());
        assert_eq!(atr.chunk(), None);
        atr.set_chunk(Some("NP".to_string()));
        assert_eq!(atr.chunk(), Some("NP"));
        assert!(atr.has_chunk("NP"));
        assert!(!atr.has_chunk("VP"));
        atr.set_chunks(vec!["VP".to_string(), "NP".to_string()]);
        assert!(atr.has_chunk_matching(&regex::Regex::new("N").unwrap()));
        assert!(!atr.has_chunk_matching(&regex::Regex::new("^X").unwrap()));
        atr.set_chunk(None);
        assert!(atr.chunks().is_empty());
    }

    #[test]
    fn typographic_apostrophe_flag_is_idempotent() {
        let mut atr = reading("dog’s", Some("dog’s"), &["NN"]);
        assert!(!atr.has_typographic_apostrophe());
        atr.mark_typographic_apostrophe();
        assert!(atr.has_typographic_apostrophe());
        atr.mark_typographic_apostrophe();
        assert_eq!(atr.chunks(), &[TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string()]);
    }

    #[test]
    fn replacing_chunks_keeps_the_apostrophe_flag_when_dropped() {
        let mut atr = reading("dog’s", Some("dog’s"), &["NN"]);
        atr.mark_typographic_apostrophe();
        atr.replace_chunk_tags_preserving_apostrophe_flag(vec!["NP".to_string()]);
        assert_eq!(
            atr.chunks(),
            &["NP".to_string(), TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string()]
        );
    }

    #[test]
    fn replacing_chunks_does_not_duplicate_the_apostrophe_flag() {
        let mut atr = reading("dog’s", Some("dog’s"), &["NN"]);
        atr.mark_typographic_apostrophe();
        atr.replace_chunk_tags_preserving_apostrophe_flag(vec![
            "NP".to_string(),
            TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string(),
        ]);
        assert_eq!(
            atr.chunks(),
            &["NP".to_string(), TYPOGRAPHIC_APOSTROPHE_CHUNK.to_string()]
        );
    }

    #[test]
    fn replacing_chunks_adds_no_apostrophe_flag_when_absent_before() {
        let mut atr = reading("dog", Some("dog"), &["NN"]);
        atr.replace_chunk_tags_preserving_apostrophe_flag(vec!["NP".to_string()]);
        assert_eq!(atr.chunks(), &["NP".to_string()]);
        assert!(!atr.has_typographic_apostrophe());
    }

    #[test]
    fn speller_and_immunization_flags_default_off() {
        let mut atr = reading("dog", Some("dog"), &["NN"]);
        assert!(!atr.is_ignored_by_speller());
        assert!(!atr.is_immunized());
        atr.ignore_spelling();
        atr.immunize();
        assert!(atr.is_ignored_by_speller());
        assert!(atr.is_immunized());
    }

    #[test]
    fn add_sent_end_appends_a_tagged_reading_and_marks_the_token() {
        let mut atr = reading("dog", Some("dog"), &["NN"]);
        assert!(!atr.is_sentence_end());
        atr.add_sent_end();
        assert!(atr.is_sentence_end());
        let last = atr.readings().last().unwrap();
        assert!(last.has_pos_tag("SENT_END"));
        assert_eq!(last.lemma(), Some("dog"));
        assert_eq!(last.token(), "dog");
        assert!(atr.token().has_pos_tag("SENT_END"));
    }

    #[test]
    fn add_sent_end_reuses_the_token_text_when_no_lemma_exists() {
        let mut atr = reading("...", None, &["SENT"]);
        atr.add_sent_end();
        assert_eq!(atr.readings().last().unwrap().lemma(), Some("..."));
    }

    #[test]
    fn add_sent_end_drops_a_trailing_untagged_reading_like_java() {
        let mut atr = AnalyzedTokenReadings::new(bare("dog"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("dog", 0, 3)
                .with_lemma("dog")
                .with_pos_tags(vec!["NN".to_string()]),
            AnalyzedToken::new("dog", 0, 3),
        ]);
        atr.add_sent_end();
        assert_eq!(atr.readings().len(), 2);
        assert!(atr.readings()[1].has_pos_tag("SENT_END"));
        assert!(!atr.readings()[1].pos_tags().contains(&"NN".to_string()));
    }

    #[test]
    fn sentence_start_is_detected_on_token_or_readings() {
        let via_token = reading("<S>", None, &["SENT_START"]);
        assert!(via_token.is_sentence_start());
        assert!(!via_token.is_sentence_end());

        let mut via_reading = reading("<S>", None, &[]);
        via_reading = via_reading.with_readings(vec![
            AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
        ]);
        assert!(via_reading.is_sentence_start());
        assert!(via_reading.token().pos_tags().is_empty());
    }

    #[test]
    fn all_pos_tags_deduplicates_across_readings() {
        let mut atr = AnalyzedTokenReadings::new(bare("flies"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("fly", 0, 5)
                .with_pos_tags(vec!["NNS".to_string(), "VBZ".to_string()]),
            AnalyzedToken::new("fly", 0, 5)
                .with_pos_tags(vec!["VBZ".to_string(), "NN".to_string()]),
        ]);
        assert_eq!(atr.all_pos_tags(), vec!["NNS", "VBZ", "NN"]);
    }

    #[test]
    fn all_lemmas_skips_readings_without_a_lemma() {
        let mut atr = AnalyzedTokenReadings::new(bare("x"));
        atr = atr.with_readings(vec![
            AnalyzedToken::new("a", 0, 1).with_lemma("lem-a"),
            AnalyzedToken::new("b", 0, 1),
            AnalyzedToken::new("c", 0, 1).with_lemma("lem-c"),
        ]);
        assert_eq!(atr.all_lemmas(), vec!["lem-a", "lem-c"]);
    }

    #[test]
    fn readings_deserialize_with_java_defaulted_fields() {
        let json = r#"{"token":{"token":"dog","lemma":null,"pos_tags":[],"start":0,"end":3},"readings":[{"token":"dog","lemma":null,"pos_tags":[],"start":0,"end":3}],"is_whitespace":false}"#;
        let atr: AnalyzedTokenReadings = serde_json::from_str(json).unwrap();
        assert_eq!(atr.token().token(), "dog");
        assert_eq!(atr.readings().len(), 1);
        assert!(atr.chunks().is_empty());
        assert!(!atr.is_ignored_by_speller());
        assert!(!atr.is_immunized());
    }

    #[test]
    fn analyzed_sentence_tracks_text_offsets_and_tokens() {
        let mut sentence = AnalyzedSentence::new("Hi there.", 2, 11);
        assert_eq!(sentence.text(), "Hi there.");
        assert_eq!(sentence.start(), 2);
        assert_eq!(sentence.end(), 11);
        assert!(sentence.tokens().is_empty());
        let tokens = vec![
            reading("Hi", Some("hi"), &["UH"]),
            AnalyzedTokenReadings::new(AnalyzedToken::new(" ", 4, 5)),
            reading("there.", None, &["RB"]),
        ];
        sentence.set_tokens(tokens);
        assert_eq!(sentence.tokens().len(), 3);
        assert_eq!(sentence.non_whitespace_tokens().len(), 2);
        sentence.tokens_mut().pop();
        assert_eq!(sentence.tokens().len(), 2);
    }

    #[test]
    fn pre_disambiguation_tokens_fall_back_to_current_tokens() {
        let mut sentence = AnalyzedSentence::new("Hi.", 0, 3);
        sentence.set_tokens(vec![reading("Hi", Some("hi"), &["NN"])]);
        assert_eq!(sentence.pre_disambiguation_tokens().len(), 1);
        assert_eq!(sentence.pre_disambiguation_non_whitespace_tokens().len(), 1);
        let snapshot = sentence.tokens().to_vec();
        sentence.set_pre_disambiguation_tokens(snapshot);
        sentence.tokens_mut().clear();
        assert_eq!(sentence.pre_disambiguation_tokens().len(), 1);
        assert_eq!(sentence.tokens().len(), 0);
    }
}
