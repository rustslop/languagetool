use og_core::checker::Tagger as _;
use og_tokenizer::WordTokenizer as _;

fn main() {
    let text = std::env::args().nth(1).expect("text");
    let target = std::env::args().nth(2).expect("target");
    let code = "en";
    let lt_modules = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules";
    let resource_dir = std::path::PathBuf::from(format!(
        "{lt_modules}/{code}/src/main/resources/org/languagetool/resource/{code}"
    ));
    let core_resource_dir = og_langs::engine::find_lt_core_resource_dir().expect("core");
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let english_tagger = std::sync::Arc::new(og_tagger::EnglishTagger::new());
    let has_english = english_tagger.entry_count() > 0;
    let english_tagger = has_english.then_some(english_tagger);
    eprintln!("rules loaded, english_tagger: {}", has_english);
    let d =
        og_tagger::XmlDisambiguator::from_xml_with_english(&xml, english_tagger).expect("disambig");
    eprintln!("rules: {}", d.rule_count());
    let _ = (&resource_dir, &core_resource_dir);
    let tagger = og_tagger::EnglishTagger::from_resource_dir(&resource_dir);
    let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
    let raw = tokenizer.tokenize(&text);
    let mut tokens: Vec<og_core::AnalyzedTokenReadings> = raw
        .into_iter()
        .map(|t| {
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                t.text(),
                t.start(),
                t.end(),
            ))
        })
        .collect();
    tokens.insert(
        0,
        og_core::AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        ),
    );
    tagger.tag(&mut tokens);
    for (i, t) in tokens.iter().enumerate() {
        if t.token().token() == target {
            eprintln!(
                "pre[{i}] {} {:?}",
                t.token().token(),
                t.readings()
                    .iter()
                    .map(|r| r.pos_tags().join("|"))
                    .collect::<Vec<_>>()
            );
        }
    }
    let total = tokens.last().map(|t| t.token().end()).unwrap_or(0);
    let mut sentence = og_core::AnalyzedSentence::new(text.clone(), 0, total);
    sentence.set_tokens(tokens);
    let mut before = String::new();
    for t in sentence.tokens().iter() {
        if t.token().token() == target {
            before = t
                .readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>()
                .join(",");
        }
    }
    let only: Option<usize> = std::env::args().nth(3).map(|a| a.parse().unwrap());
    if let Some(idx) = only {
        let hits = d.debug_match_rule(&mut sentence, idx);
        eprintln!("matches at: {:?}", hits);
        for (i, t) in sentence.tokens().iter().enumerate() {
            eprintln!(
                "tok[{i}] {} {:?}",
                t.token().token(),
                t.readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect::<Vec<_>>()
            );
        }
        return;
    }
    for idx in 0..d.rule_count() {
        d.apply_rule(&mut sentence, idx);
        let mut after = String::new();
        for t in sentence.tokens().iter() {
            if t.token().token() == target {
                after = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect::<Vec<_>>()
                    .join(",");
            }
        }
        if after != before {
            eprintln!(
                "rule #{idx}: {} -> {} via {} | {}",
                before,
                after,
                d.rule_debug(idx),
                d.rule_pattern_debug(idx)
            );
            before = after;
        }
    }
    for (i, t) in sentence.tokens().iter().enumerate() {
        if t.token().token() == target {
            eprintln!(
                "post[{i}] {} {:?}",
                t.token().token(),
                t.readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect::<Vec<_>>()
            );
        }
    }
}
