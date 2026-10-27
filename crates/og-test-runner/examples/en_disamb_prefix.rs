use og_core::checker::{Disambiguator, Tagger};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let text = std::env::args().nth(1).expect("sentence");
    let target = std::env::args().nth(2).expect("target token");
    let xml = std::fs::read_to_string(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/disambiguation.xml",
    )
    .expect("disambiguation.xml");

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

    let readings_for = |sentence: &AnalyzedSentence| -> Vec<String> {
        sentence
            .tokens()
            .iter()
            .filter(|t| t.token().token() == target)
            .map(|t| {
                t.readings()
                    .iter()
                    .flat_map(|r| r.pos_tags().to_vec())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect()
    };

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
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in positions {
        if let Some(rel) = cleaned[i..].find("</rule>") {
            spans.push((i, i + rel + "</rule>".len()));
        }
    }
    eprintln!("{} live rule fragments", spans.len());

    let mut state = base.clone();
    let mut before = readings_for(&state);
    println!("start {target}: {before:?}");
    for (idx, (s, e)) in spans.iter().enumerate() {
        let fragment = &cleaned[*s..*e];
        let id = fragment
            .find("id=\"")
            .map(|p| {
                let st = p + 4;
                let en = fragment[st..].find('"').unwrap() + st;
                &fragment[st..en]
            })
            .unwrap_or("?");
        let wrapped = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"en\">\n{fragment}\n</rules>\n"
        );
        let Ok(disambiguator) = XmlDisambiguator::from_xml(&wrapped) else {
            continue;
        };
        disambiguator.disambiguate(&mut state);
        let after = readings_for(&state);
        if after != before {
            println!("rule #{idx} id={id}: {target} -> {after:?}");
            before = after;
        }
    }
}
