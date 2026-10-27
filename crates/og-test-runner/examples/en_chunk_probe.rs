use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::opennlp::{
    GisModel, OpenNlpChunker, OpenNlpEnglishChunker, OpenNlpPosTagger, OpenNlpTokenizer,
};
use og_tokenizer::WordTokenizer as _;
use std::path::Path;

fn main() {
    let dir = Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let chunker = OpenNlpEnglishChunker::from_resource_dir(dir).expect("chunker models");
    let tok = OpenNlpTokenizer::new(GisModel::from_file(&dir.join("en-token.model")).expect("tok"));

    for text in std::env::args().skip(1) {
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
        let raw = tokenizer.split_underscores(tokenizer.tokenize(&text));
        let mut readings: Vec<AnalyzedTokenReadings> = raw
            .iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end())))
            .collect();
        readings.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        sentence.set_tokens(readings);

        let joined: String = raw.iter().map(|t| t.text()).collect();
        let clean = joined.replace('\u{2019}', "'");
        let spans = tok.tokenize_spans(&clean);
        let token_texts: Vec<&str> = spans.iter().map(|(s, e)| &clean[*s..*e]).collect();
        println!("opennlp tokens: {token_texts:?}");
        let pos_model = GisModel::from_file(&dir.join("en-pos-maxent.model")).expect("pos");
        let tagdict = og_tagger::opennlp::parse_tag_dictionary(
            &std::fs::read_to_string(dir.join("en-pos-maxent.tagdict")).expect("tagdict"),
            &pos_model,
        );
        let pos = OpenNlpPosTagger::new(pos_model, tagdict);
        let tag_idx = pos.tag(&token_texts);
        let tag_names: Vec<&str> = tag_idx.iter().map(|i| pos.outcome(*i)).collect();
        println!("opennlp pos:    {tag_names:?}");
        let chunk_model = GisModel::from_file(&dir.join("en-chunker.model")).expect("chunk");
        let ochunker = OpenNlpChunker::new(chunk_model);
        let chunk_idx = ochunker.chunk(&token_texts, &tag_names);
        let chunk_names: Vec<&str> = chunk_idx.iter().map(|i| ochunker.outcome(*i)).collect();
        println!("opennlp chunks: {chunk_names:?}");

        chunker.add_chunk_tags(&mut sentence);
        for t in sentence.tokens() {
            if t.is_whitespace() || t.token().token() == "<S>" {
                continue;
            }
            println!("  {:?} chunks={:?}", t.token().token(), t.chunks());
        }
    }
}
