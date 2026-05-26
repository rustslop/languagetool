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
