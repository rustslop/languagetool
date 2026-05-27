use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DictionaryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid dictionary format: {0}")]
    InvalidFormat(String),
}

pub struct Dictionary {
    words: HashSet<String>,
    lowercased: HashSet<String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self {
            words: HashSet::new(),
            lowercased: HashSet::new(),
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, DictionaryError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut dict = Self::new();

        for line in reader.lines() {
            let line = line?;
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.words.insert(word.to_string());
                dict.lowercased.insert(word.to_lowercase());
            }
        }

        Ok(dict)
    }

    /// Load from Morfologik decoded dictionary format (word\tlemma\tPOS).
    /// Only the first column (surface form) is used for spell checking.
    pub fn from_morfologik(path: &Path) -> Result<Self, DictionaryError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut dict = Self::new();

        for line in reader.lines() {
            let line = line?;
            let word = line.split('\t').next().unwrap_or("").trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.words.insert(word.to_string());
                dict.lowercased.insert(word.to_lowercase());
            }
        }

        Ok(dict)
    }

    pub fn from_words(words: &[&str]) -> Self {
        let mut dict = Self::new();
        for word in words {
            dict.words.insert(word.to_string());
            dict.lowercased.insert(word.to_lowercase());
        }
        dict
    }

    pub fn add_word(&mut self, word: &str) {
        self.words.insert(word.to_string());
        self.lowercased.insert(word.to_lowercase());
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word) || self.lowercased.contains(&word.to_lowercase())
    }

    pub fn contains_exact(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn word_list(&self) -> Vec<String> {
        self.words.iter().cloned().collect()
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}
