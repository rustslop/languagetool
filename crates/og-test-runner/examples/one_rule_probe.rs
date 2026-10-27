use og_core::checker::{Disambiguator as _, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let xml_path = std::env::args().nth(1).expect("xml file");
    let text = std::env::args().nth(2).expect("sentence");
    let target = std::env::args().nth(3).expect("target token");
    let resource_dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let xml = std::fs::read_to_string(&xml_path).expect("xml");
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
    let mut base = AnalyzedSentence::new(&text, 0, text.len());
    base.set_tokens(tokens);
    let tagger = EnglishTagger::from_resource_dir(resource_dir);
    Tagger::tag(&tagger, base.tokens_mut());
    og_tagger::chunker::load_opennlp_chunker(resource_dir);
    og_tagger::chunker::apply_chunk_tags(&mut base);
    if let Some(last) = base.tokens_mut().last_mut() {
        last.add_sent_end();
    }
    let show = |s: &AnalyzedSentence| {
        s.tokens()
            .iter()
            .filter(|t| t.token().token() == target)
            .map(|t| {
                format!(
                    "[{}]",
                    t.readings()
                        .iter()
                        .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<String>()
    };
    let before = show(&base);
    let d = XmlDisambiguator::from_xml(&xml).expect("parse");
    let mut sentence = base.clone();
    d.disambiguate(&mut sentence);
    println!("before {target}={before}");
    println!("after  {target}={}", show(&sentence));
}
