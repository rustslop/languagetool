use og_core::checker::Tagger;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn snapshot(s: &AnalyzedSentence) -> String {
    s.tokens()
        .iter()
        .filter(|t| !t.is_whitespace())
        .map(|t| {
            format!(
                "{}[{}]",
                t.token().token(),
                t.readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let text = std::env::args().nth(1).expect("sentence");
    let rule_id = std::env::args().nth(2).expect("rule id");
    let resource_dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let gate_tagger = EnglishTagger::from_resource_dir(resource_dir);
    let gate = std::sync::Arc::new(gate_tagger);
    let gate_arc = gate.clone();
    let tokenizer =
        og_tokenizer::DefaultWordTokenizer::new_english(std::sync::Arc::new(move |w: &str| {
            gate_arc.is_word_tagged(w)
        }));
    let tagger = gate;
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
    Tagger::tag(tagger.as_ref(), base.tokens_mut());
    og_tagger::chunker::load_opennlp_chunker(resource_dir);
    og_tagger::chunker::apply_chunk_tags(&mut base);
    if let Some(last) = base.tokens_mut().last_mut() {
        last.add_sent_end();
    }
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
    let d = XmlDisambiguator::from_xml_with_english(&cleaned, Some(tagger.clone())).expect("parse");
    let first = d.first_index_of_rule_id(&rule_id).expect("id");
    eprintln!("rule id {rule_id} first index {first}");
    for idx in first..(first + 8) {
        let id = d.rule_id(idx).unwrap_or("?").to_string();
        if id != rule_id {
            break;
        }
        let mut s = base.clone();
        d.apply_rules_through(&mut s, first);
        let before = snapshot(&s);
        eprintln!("member {idx} pattern: {}", d.rule_pattern_debug(idx));
        eprintln!(
            "member {idx} matches: {:?}",
            d.debug_match_rule(&mut s, idx)
        );
        d.apply_rule(&mut s, idx);
        let after = snapshot(&s);
        if before != after {
            println!("MEMBER {idx} CHANGES THE SENTENCE:");
            let b: Vec<&str> = before.split(' ').collect();
            let a: Vec<&str> = after.split(' ').collect();
            for (x, y) in b.iter().zip(a.iter()) {
                if x != y {
                    println!("  {x} -> {y}");
                }
            }
        }
    }
}
