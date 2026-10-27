pub mod french_word_tokenizer;
pub mod german_word_tokenizer;
pub mod sentence_splitter;
pub mod srx;
pub mod word_tokenizer;

pub use french_word_tokenizer::FrenchWordTokenizer;
pub use german_word_tokenizer::GermanWordTokenizer;
pub use sentence_splitter::*;
pub use word_tokenizer::*;

use og_core::{Sentence, Token};

pub trait SentenceTokenizer: Send + Sync {
    fn split(&self, text: &str) -> Vec<Sentence>;
}

pub trait WordTokenizer: Send + Sync {
    fn tokenize(&self, text: &str) -> Vec<Token>;
}
