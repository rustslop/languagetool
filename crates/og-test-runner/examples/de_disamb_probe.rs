use og_core::checker::{Disambiguator, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{GermanTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let xml_path = std::env::args().nth(1).expect("xml");
    let text = std::env::args().nth(2).expect("text");
    let dir = std::path::PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
    );
    let tagger = GermanTagger::from_resource_dir(&dir).expect("tagger");
    let raw = og_tokenizer::DefaultWordTokenizer::new_generic().tokenize(&text);
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
    tagger.tag(sentence.tokens_mut());
    {
        let toks = sentence.tokens_mut();
        for t in toks.iter_mut().rev() {
            if !t.is_whitespace() && !t.token().pos_tags().contains(&"SENT_START".to_string()) {
                if !t.has_pos_tag("SENT_END") {
                    t.add_sent_end();
                }
                break;
            }
        }
    }
    println!("--- after tagger:");
    for t in sentence.tokens() {
        if t.is_whitespace() {
            continue;
        }
        println!(
            "  {:?} => {}",
            t.token().token(),
            t.readings()
                .iter()
                .map(|r| format!("{}[{}]", r.lemma().unwrap_or("?"), r.pos_tags().join("|")))
                .collect::<Vec<_>>()
                .join(" ; ")
        );
    }
    let disambig = XmlDisambiguator::from_xml(&std::fs::read_to_string(&xml_path).expect("xml"))
        .expect("parse");
    println!("rules: {}", disambig.rule_count());
    disambig.disambiguate(&mut sentence);
    for t in sentence.tokens() {
        if t.is_whitespace() {
            continue;
        }
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| format!("{}[{}]", r.lemma().unwrap_or("?"), r.pos_tags().join("|")))
            .collect();
        println!("{:?} => {}", t.token().token(), readings.join(" ; "));
    }
}
