use og_core::checker::{SentenceTokenizer, WordTokenizer};
use og_core::{AnalyzedToken, AnalyzedTokenReadings, SentenceRange};

pub struct SentenceSplitterAdapter;

impl SentenceTokenizer for SentenceSplitterAdapter {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        let splitter = og_tokenizer::DefaultSentenceSplitter::new();
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split(text);
        sentences.into_iter().map(|s| SentenceRange {
            start: s.start(),
            end: s.end(),
        }).collect()
    }
}

pub struct WordTokenizerAdapter;

impl WordTokenizer for WordTokenizerAdapter {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
        use og_tokenizer::WordTokenizer as _;
        let tokens = tokenizer.tokenize(text);
        tokens.into_iter().map(|t| {
            let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
            AnalyzedTokenReadings::new(at)
        }).collect()
    }
}
