use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tokenizer::WordTokenizer as _;
use std::sync::LazyLock;

static TOKENIZER: LazyLock<og_tokenizer::DefaultWordTokenizer> =
    LazyLock::new(og_tokenizer::DefaultWordTokenizer::new);

pub(crate) fn sentence_range(text: &str, start: usize, end: usize) -> AnalyzedSentence {
    let mut sentence = AnalyzedSentence::new(&text[start..end], start, end);
    let mut tokens = vec![AnalyzedTokenReadings::new(
        AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
    )];
    tokens.extend(
        TOKENIZER
            .tokenize(&text[start..end])
            .into_iter()
            .filter(|t| !t.text().trim().is_empty())
            .map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(
                    t.text(),
                    start + t.start(),
                    start + t.end(),
                ))
            }),
    );
    sentence.set_tokens(tokens);
    sentence
}

pub(crate) fn sentence(text: &str) -> AnalyzedSentence {
    sentence_range(text, 0, text.len())
}

pub(crate) fn sentences_split_after(text: &str, markers: &[&str]) -> Vec<AnalyzedSentence> {
    let mut bounds: Vec<usize> = Vec::new();
    let mut search_from = 0usize;
    for marker in markers {
        let idx = text[search_from..]
            .find(marker)
            .unwrap_or_else(|| panic!("marker {marker} not found in {text}"));
        let end = search_from + idx + marker.len();
        bounds.push(end);
        search_from = end;
    }
    let mut start = 0usize;
    let mut sentences = Vec::with_capacity(bounds.len() + 1);
    for end in bounds {
        sentences.push(sentence_range(text, start, end));
        start = end;
    }
    sentences.push(sentence_range(text, start, text.len()));
    sentences
}

pub(crate) fn token_with_pos(
    text: &str,
    start: usize,
    end: usize,
    pos_tags: &[&str],
) -> AnalyzedTokenReadings {
    AnalyzedTokenReadings::new(
        AnalyzedToken::new(text, start, end)
            .with_pos_tags(pos_tags.iter().map(|t| t.to_string()).collect()),
    )
}
