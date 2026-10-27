use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    text: String,
    start: usize,
    end: usize,
}

impl Token {
    pub fn new(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            text: text.into(),
            start,
            end,
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

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn is_whitespace(&self) -> bool {
        self.text.chars().all(|c| c.is_whitespace())
    }

    pub fn is_punctuation(&self) -> bool {
        self.text.chars().all(|c| c.is_ascii_punctuation())
    }

    pub fn is_word(&self) -> bool {
        self.text.chars().any(|c| c.is_alphabetic())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sentence {
    text: String,
    start: usize,
    end: usize,
    tokens: Vec<Token>,
}

impl Sentence {
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

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn set_tokens(&mut self, tokens: Vec<Token>) {
        self.tokens = tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_exposes_text_and_offsets() {
        let token = Token::new("word", 3, 7);
        assert_eq!(token.text(), "word");
        assert_eq!(token.start(), 3);
        assert_eq!(token.end(), 7);
        assert_eq!(token.len(), 4);
    }

    #[test]
    fn is_empty_follows_text_not_length() {
        assert!(Token::new("", 0, 0).is_empty());
        assert!(!Token::new("a", 0, 1).is_empty());
    }

    #[test]
    fn is_whitespace_requires_every_char_whitespace() {
        assert!(Token::new(" \t\n", 0, 3).is_whitespace());
        assert!(!Token::new("a b", 0, 3).is_whitespace());
        assert!(!Token::new("word", 0, 4).is_whitespace());
    }

    #[test]
    fn is_punctuation_matches_ascii_punctuation_only() {
        assert!(Token::new(".,;:", 0, 4).is_punctuation());
        assert!(!Token::new("a.", 0, 2).is_punctuation());
        assert!(!Token::new("–", 0, 3).is_punctuation());
    }

    #[test]
    fn is_word_requires_an_alphabetic_char() {
        assert!(Token::new("word", 0, 4).is_word());
        assert!(Token::new("a1", 0, 2).is_word());
        assert!(!Token::new("123", 0, 3).is_word());
    }

    #[test]
    fn token_round_trips_through_json() {
        let token = Token::new("word", 3, 7);
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, r#"{"text":"word","start":3,"end":7}"#);
        let back: Token = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text(), "word");
        assert_eq!(back.start(), 3);
        assert_eq!(back.end(), 7);
    }

    #[test]
    fn sentence_holds_text_offsets_and_tokens() {
        let mut sentence = Sentence::new("hi there", 2, 10);
        assert_eq!(sentence.text(), "hi there");
        assert_eq!(sentence.start(), 2);
        assert_eq!(sentence.end(), 10);
        assert!(sentence.tokens().is_empty());
        sentence.set_tokens(vec![Token::new("hi", 2, 4), Token::new("there", 5, 10)]);
        assert_eq!(sentence.tokens().len(), 2);
        assert_eq!(sentence.tokens()[1].text(), "there");
    }

    #[test]
    fn sentence_round_trips_through_json() {
        let mut sentence = Sentence::new("hi", 0, 2);
        sentence.set_tokens(vec![Token::new("hi", 0, 2)]);
        let back: Sentence =
            serde_json::from_str(&serde_json::to_string(&sentence).unwrap()).unwrap();
        assert_eq!(back.text(), "hi");
        assert_eq!(back.tokens().len(), 1);
    }
}
