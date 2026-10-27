use og_core::checker::{Disambiguator as _, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let text = std::env::args().nth(1).expect("sentence");
    let target = std::env::args().nth(2).expect("target token");
    let resource_dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");

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

    let readings_for = |sentence: &AnalyzedSentence| -> Vec<String> {
        sentence
            .tokens()
            .iter()
            .filter(|t| t.token().token() == target)
            .map(|t| {
                t.readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect()
    };

    let positions: Vec<usize> = xml
        .match_indices("<rule")
        .filter(|(i, _)| {
            xml[*i + 5..]
                .chars()
                .next()
                .is_some_and(|c| c == '>' || c.is_whitespace())
        })
        .map(|(i, _)| i)
        .collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in positions {
        if let Some(rel) = xml[i..].find("</rule>") {
            spans.push((i, i + rel + "</rule>".len()));
        }
    }

    println!("start {target}: {:?}", readings_for(&base));
    let mut sentence = base.clone();
    let mut last = readings_for(&sentence);
    for (idx, (s, e)) in spans.iter().enumerate() {
        let fragment = &xml[*s..*e];
        let id = fragment
            .find("id=\"")
            .map(|p| &fragment[p + 4..fragment[p + 4..].find('"').unwrap() + p + 4])
            .unwrap_or("?");
        let wrapped = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"en\">\n{fragment}\n</rules>\n"
        );
        let Ok(disambiguator) = XmlDisambiguator::from_xml(&wrapped) else {
            continue;
        };
        disambiguator.disambiguate(&mut sentence);
        let after = readings_for(&sentence);
        if after != last {
            println!("rule #{idx} id={id}: {target} -> {after:?}");
            last = after;
        }
    }
    println!("final {target}: {last:?}");
}
