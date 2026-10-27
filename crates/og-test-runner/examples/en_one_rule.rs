use og_core::checker::{Disambiguator, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let text = std::env::args().nth(1).expect("sentence");
    let rule_number: usize = std::env::args()
        .nth(2)
        .expect("rule #")
        .parse()
        .expect("num");
    let xml = std::fs::read_to_string(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/disambiguation.xml",
    )
    .expect("disambiguation.xml");
    let cleaned = {
        let mut out = String::with_capacity(xml.len());
        let mut i = 0usize;
        while i < xml.len() {
            if xml.is_char_boundary(i) && xml[i..].starts_with("<!--") {
                match xml[i + 4..].find("-->") {
                    Some(rel) => i += 4 + rel + 3,
                    None => i = xml.len(),
                }
            } else {
                let step = xml[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                out.push_str(&xml[i..i + step]);
                i += step;
            }
        }
        out
    };
    let positions: Vec<usize> = cleaned
        .match_indices("<rule")
        .filter(|(i, _)| {
            cleaned[*i + 5..]
                .chars()
                .next()
                .is_some_and(|c| c == '>' || c.is_whitespace())
        })
        .map(|(i, _)| i)
        .collect();
    let s = positions[rule_number];
    let e = cleaned[s..]
        .find("</rule>")
        .map(|r| s + r + 7)
        .expect("end");
    let fragment = &cleaned[s..e];
    println!("{fragment}");

    let resource_dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
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
    let disambiguator = XmlDisambiguator::from_xml(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"en\">\n{fragment}\n</rules>\n"
    ))
    .expect("parse");
    let dump = |label: &str, sent: &AnalyzedSentence| {
        println!("--- {label}");
        for t in sent.tokens() {
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                .collect();
            println!("  {:?} => {}", t.token().token(), readings.join(", "));
        }
    };
    dump("before", &base);
    let mut after = base.clone();
    disambiguator.disambiguate(&mut after);
    dump("after", &after);
}
