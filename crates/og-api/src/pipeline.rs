use og_core::checker::{SentenceTokenizer, WordTokenizer};
use og_core::{AnalyzedToken, AnalyzedTokenReadings, SentenceRange};

pub struct SentenceSplitterAdapter;

impl SentenceTokenizer for SentenceSplitterAdapter {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        static SPLITTER: std::sync::OnceLock<og_tokenizer::DefaultSentenceSplitter> =
            std::sync::OnceLock::new();
        let splitter = SPLITTER.get_or_init(og_tokenizer::DefaultSentenceSplitter::new);
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split(text);
        sentences
            .into_iter()
            .map(|s| SentenceRange {
                start: s.start(),
                end: s.end(),
            })
            .collect()
    }
}

pub struct WordTokenizerAdapter;

impl WordTokenizer for WordTokenizerAdapter {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        static EN: std::sync::OnceLock<std::sync::Arc<og_tokenizer::DefaultWordTokenizer>> =
            std::sync::OnceLock::new();
        let tokenizer =
            EN.get_or_init(
                || match og_tagger::english_tagger::shared_english_tagger() {
                    Some(tagger) => {
                        std::sync::Arc::new(og_tokenizer::DefaultWordTokenizer::new_english(
                            std::sync::Arc::new(move |w: &str| tagger.is_word_tagged(w)),
                        ))
                    }
                    None => std::sync::Arc::new(og_tokenizer::DefaultWordTokenizer::new()),
                },
            );
        let tokens = tokenizer.split_underscores(tokenizer.tokenize(text));
        tokens
            .into_iter()
            .map(|t| {
                let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::checker::{SentenceTokenizer, WordTokenizer};

    #[test]
    fn sentence_adapter_covers_the_text_with_ordered_ranges() {
        let text = "Hello there. Second one!";
        let ranges = SentenceSplitterAdapter.split(text);
        assert_eq!(ranges.len(), 2, "{ranges:?}");
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, text.len());
        for pair in ranges.windows(2) {
            assert!(
                pair[0].end <= pair[1].start,
                "ranges must not overlap: {ranges:?}"
            );
        }
        assert_eq!(text[0..ranges[0].end].trim_end(), "Hello there.");
    }

    #[test]
    fn sentence_adapter_returns_a_single_range_for_unbroken_text() {
        let text = "no sentence end here";
        let ranges = SentenceSplitterAdapter.split(text);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 0);
        assert_eq!(ranges[0].end, text.len());
    }

    fn token_texts(tokens: &[AnalyzedTokenReadings]) -> Vec<&str> {
        tokens.iter().map(|t| t.token().token()).collect()
    }

    #[test]
    fn word_adapter_shifts_offsets_by_the_sentence_start() {
        for offset in [0, 12] {
            let tokens = WordTokenizerAdapter.tokenize("Hello world.", offset);
            let texts = token_texts(&tokens);
            assert!(texts.contains(&"Hello"), "{texts:?}");
            assert!(texts.contains(&"world"), "{texts:?}");
            for token in &tokens {
                let token = token.token();
                let (start, end) = (token.start(), token.end());
                assert_eq!(&"Hello world."[start - offset..end - offset], token.token());
            }
        }
        let tokens = WordTokenizerAdapter.tokenize("Hello world.", 12);
        let hello = tokens
            .iter()
            .find(|t| t.token().token() == "Hello")
            .unwrap()
            .token();
        assert_eq!((hello.start(), hello.end()), (12, 17));
    }
}
