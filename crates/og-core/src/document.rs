use serde::{Deserialize, Serialize};
use crate::Sentence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    text: String,
    sentences: Vec<Sentence>,
}

impl Document {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            sentences: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn sentences(&self) -> &[Sentence] {
        &self.sentences
    }

    pub fn set_sentences(&mut self, sentences: Vec<Sentence>) {
        self.sentences = sentences;
    }

    pub fn char_count(&self) -> usize {
        self.text.len()
    }
}
