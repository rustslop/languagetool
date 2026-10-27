use og_core::checker::Tagger;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

fn main() {
    let xml_path = std::env::args().nth(1).expect("xml file");
    let text = std::env::args().nth(2).expect("sentence");
    let resource_dir = std::path::Path::new(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    );
    let xml = std::fs::read_to_string(&xml_path).expect("xml");
    let gate = std::sync::Arc::new(EnglishTagger::from_resource_dir(resource_dir));
    let gate2 = gate.clone();
    let tokenizer =
        og_tokenizer::DefaultWordTokenizer::new_english(std::sync::Arc::new(move |w: &str| {
            gate2.is_word_tagged(w)
        }));
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
    Tagger::tag(gate.as_ref(), base.tokens_mut());
    og_tagger::chunker::load_opennlp_chunker(resource_dir);
    og_tagger::chunker::apply_chunk_tags(&mut base);
    if let Some(last) = base.tokens_mut().last_mut() {
        last.add_sent_end();
    }
    let nws: Vec<String> = base
        .tokens()
        .iter()
        .filter(|t| !t.is_whitespace())
        .map(|t| t.token().token().to_string())
        .collect();
    println!("tokens: {nws:?}");
    for (i, t) in base
        .tokens()
        .iter()
        .filter(|t| !t.is_whitespace())
        .enumerate()
    {
        println!(
            "  {i} {} [{}] {:?}",
            t.token().token(),
            t.chunks().join(","),
            t.readings()
                .iter()
                .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                .collect::<Vec<_>>()
        );
    }
    let d = XmlDisambiguator::from_xml(&xml).expect("parse");
    for idx in 0..d.rule_count() {
        println!(
            "rule {idx} id={:?} matches={:?}",
            d.rule_id(idx),
            d.debug_match_rule(&mut base.clone(), idx)
        );
    }
    let mut sentence = base.clone();
    og_core::checker::Disambiguator::disambiguate(&d, &mut sentence);
    for (i, t) in sentence
        .tokens()
        .iter()
        .filter(|t| !t.is_whitespace())
        .enumerate()
    {
        let before = base
            .tokens()
            .iter()
            .filter(|t| !t.is_whitespace())
            .nth(i)
            .map(|b| {
                b.readings()
                    .iter()
                    .map(|r| r.pos_tags().join("|"))
                    .collect::<Vec<_>>()
                    .join(",")
                    .to_string()
            })
            .unwrap_or_default();
        let after = t
            .readings()
            .iter()
            .map(|r| r.pos_tags().join("|"))
            .collect::<Vec<_>>()
            .join(",");
        if before != after {
            println!("CHANGED {i} {}: {before} -> {after}", t.token().token());
        }
    }
}
