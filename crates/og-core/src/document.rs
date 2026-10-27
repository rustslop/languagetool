use crate::Sentence;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Token;

    #[test]
    fn document_starts_empty_of_sentences() {
        let document = Document::new("Hello world.");
        assert_eq!(document.text(), "Hello world.");
        assert!(document.sentences().is_empty());
        assert_eq!(document.char_count(), 12);
    }

    #[test]
    fn document_stores_sentences() {
        let mut document = Document::new("One. Two.");
        let mut first = Sentence::new("One.", 0, 4);
        first.set_tokens(vec![Token::new("One.", 0, 4)]);
        let mut second = Sentence::new(" Two.", 4, 9);
        second.set_tokens(vec![Token::new("Two.", 5, 9)]);
        document.set_sentences(vec![first, second]);
        assert_eq!(document.sentences().len(), 2);
        assert_eq!(document.sentences()[1].start(), 4);
    }

    #[test]
    fn document_round_trips_through_json() {
        let mut document = Document::new("Hi.");
        document.set_sentences(vec![Sentence::new("Hi.", 0, 3)]);
        let back: Document =
            serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
        assert_eq!(back.text(), "Hi.");
        assert_eq!(back.sentences().len(), 1);
        assert_eq!(back.char_count(), 3);
    }
}
