use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;
use std::sync::Arc;

fn dump(tag: &str, sentence: &AnalyzedSentence) {
    println!("--- {tag}");
    for t in sentence.tokens() {
        if t.is_whitespace() || t.has_pos_tag("SENT_START") {
            continue;
        }
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| format!("{}[{}]", r.lemma().unwrap_or("?"), r.pos_tags().join("|")))
            .collect();
        println!(
            "  {:?} => {} chunks={:?}",
            t.token().token(),
            readings.join(" ; "),
            t.chunks()
        );
    }
}

fn main() {
    let text = std::env::args().nth(1).expect("sentence argument");
    let lt_root = std::path::PathBuf::from(std::env::var("LT_ROOT").unwrap_or_else(|_| {
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules".into()
    }));
    let resource_dir = lt_root.join("en/src/main/resources/org/languagetool/resource/en");

    let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
    let raw = tokenizer.split_underscores(tokenizer.tokenize(&text));
    let mut tokens: Vec<AnalyzedTokenReadings> = raw
        .into_iter()
        .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end())))
        .collect();
    tokens.insert(
        0,
        AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        ),
    );
    let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
    sentence.set_tokens(tokens);

    let tagger: Arc<dyn og_core::checker::Tagger> =
        Arc::new(EnglishTagger::from_resource_dir(&resource_dir));
    tagger.tag(sentence.tokens_mut());
    dump("after tagger", &sentence);

    og_tagger::chunker::load_opennlp_chunker(&resource_dir);
    og_tagger::chunker::apply_chunk_tags(&mut sentence);
    dump("after chunker", &sentence);

    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let disambiguator: Arc<dyn og_core::checker::Disambiguator> =
        Arc::new(XmlDisambiguator::from_xml(&xml).expect("disambiguator"));
    disambiguator.disambiguate(&mut sentence);
    dump("after disambiguation.xml", &sentence);
}
