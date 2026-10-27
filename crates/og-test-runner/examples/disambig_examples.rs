use og_core::checker::Tagger;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::{EnglishTagger, XmlDisambiguator};
use og_tokenizer::WordTokenizer as _;

enum TokenizerChoice {
    En(std::sync::Arc<og_tokenizer::DefaultWordTokenizer>),
    Fr(std::sync::Arc<og_tokenizer::FrenchWordTokenizer>),
    Plain,
}

struct Ctx {
    tagger: std::sync::Arc<dyn Tagger>,
    disambiguator: XmlDisambiguator,
    tokenizer: TokenizerChoice,
    resource_dir: std::path::PathBuf,
}

fn strip_markup(example: &str) -> (String, Option<(usize, usize)>) {
    let collapsed: String = example
        .chars()
        .filter(|c| *c != '\n' && *c != '\t' && *c != '\r')
        .collect();
    let mut cleaned = String::with_capacity(collapsed.len());
    let mut marker: Option<(usize, usize)> = None;
    let mut i = 0usize;
    while i < collapsed.len() {
        if collapsed.is_char_boundary(i)
            && collapsed[i..].starts_with('<')
            && let Some(rel) = collapsed[i..].find('>')
        {
            let tag = &collapsed[i + 1..i + rel];
            if tag == "marker" {
                marker = Some((cleaned.len(), usize::MAX));
            } else if tag == "/marker"
                && let Some((start, _)) = marker
            {
                marker = Some((start, cleaned.len()));
            }
            i += rel + 1;
            continue;
        }
        let step = collapsed[i..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        cleaned.push_str(&collapsed[i..i + step]);
        i += step;
    }
    (cleaned, marker)
}

fn forms_string(t: &AnalyzedTokenReadings) -> String {
    let token = t.token().token();
    let mut forms: Vec<String> = t
        .readings()
        .iter()
        .map(|r| {
            if r.pos_tags()
                .first()
                .is_some_and(|p| p.as_str() == "SENT_END")
            {
                return "</S>".to_string();
            }
            let lemma = r.lemma().filter(|l| !l.is_empty());
            let pos = r.pos_tags().first().map(|p| p.as_str());
            match (lemma, pos) {
                (Some(lemma), Some(pos)) => format!("{lemma}/{pos}"),
                (Some(lemma), None) => lemma.to_string(),
                (None, Some(pos)) => format!("{token}/{pos}"),
                (None, None) => token.to_string(),
            }
        })
        .collect();
    forms.sort();
    format!("{token}[{}]", forms.join(","))
}

fn snapshot(s: &AnalyzedSentence) -> Vec<(String, Vec<String>)> {
    s.tokens()
        .iter()
        .map(|t| {
            (
                t.token().token().to_string(),
                t.readings()
                    .iter()
                    .map(|r| {
                        format!(
                            "{}/{}",
                            r.lemma().unwrap_or_default(),
                            r.pos_tags().join("|")
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn marked_token<'a>(
    s: &'a AnalyzedSentence,
    marker_start: usize,
    form: &str,
) -> Option<&'a AnalyzedTokenReadings> {
    s.tokens()
        .iter()
        .filter(|t| t.token().token() != "<S>" || form.contains("<S>"))
        .find(|t| t.token().start() == marker_start)
}

fn main() {
    let lang = std::env::args().nth(1).unwrap_or_else(|| "en".into());
    let only_rule = std::env::args().nth(2);
    let xml_path = format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}/disambiguation.xml"
    );
    let resource_dir = std::path::PathBuf::from(format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"
    ));
    let xml = std::fs::read_to_string(&xml_path).expect("disambiguation.xml");
    let cleaned = strip_comments(&xml);

    let (tagger, tokenizer): (std::sync::Arc<dyn Tagger>, TokenizerChoice) = match lang.as_str() {
        "en" => {
            let en = std::sync::Arc::new(EnglishTagger::from_resource_dir(&resource_dir));
            let gate = en.clone();
            let tokenizer = og_tokenizer::DefaultWordTokenizer::new_english(std::sync::Arc::new(
                move |w: &str| gate.is_word_tagged(w),
            ));
            (en, TokenizerChoice::En(std::sync::Arc::new(tokenizer)))
        }
        "fr" => {
            let fr = std::sync::Arc::new(og_tagger::GenericTagger::french(&resource_dir));
            let gate = fr.clone();
            let tokenizer =
                og_tokenizer::FrenchWordTokenizer::new(std::sync::Arc::new(move |w: &str| {
                    gate.is_word_tagged(w)
                }));
            (fr, TokenizerChoice::Fr(std::sync::Arc::new(tokenizer)))
        }
        "de" => {
            let tagger: std::sync::Arc<dyn Tagger> =
                match og_tagger::GermanTagger::from_resource_dir(&resource_dir) {
                    Some(t) => std::sync::Arc::new(t),
                    None => std::sync::Arc::new(EnglishTagger::from_resource_dir(&resource_dir)),
                };
            (tagger, TokenizerChoice::Plain)
        }
        other => {
            let tagger: std::sync::Arc<dyn Tagger> = match og_test_runner::generic_dict_stem(other)
            {
                Some("french") => {
                    std::sync::Arc::new(og_tagger::GenericTagger::french(&resource_dir))
                }
                Some("russian") => {
                    std::sync::Arc::new(og_tagger::GenericTagger::russian(&resource_dir))
                }
                Some("arabic") => std::sync::Arc::new(
                    og_tagger::arabic_tagger::ArabicTagger::from_resource_dir(&resource_dir),
                ),
                Some(stem) => std::sync::Arc::new(og_tagger::GenericTagger::from_resource_dir(
                    &resource_dir,
                    stem,
                )),
                None => std::sync::Arc::new(EnglishTagger::new()),
            };
            (tagger, TokenizerChoice::Plain)
        }
    };
    let disambiguator = XmlDisambiguator::from_xml_with_english(
        &cleaned,
        og_tagger::english_tagger::shared_english_tagger(),
    )
    .expect("compile");
    let ctx = Ctx {
        tagger,
        disambiguator,
        tokenizer,
        resource_dir,
    };

    let mut untouched_total = 0usize;
    let mut ambiguous_total = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for idx in 0..ctx.disambiguator.rule_count() {
        let id = ctx.disambiguator.rule_id(idx).unwrap_or("?").to_string();
        if let Some(filter) = only_rule.as_ref()
            && id != *filter
        {
            continue;
        }
        let (untouched, ambiguous) = ctx.disambiguator.rule_examples(idx);
        let prefix_stop = ctx.disambiguator.first_index_of_rule_id(&id).unwrap_or(idx);

        for ex in untouched {
            untouched_total += 1;
            let (clean, _) = strip_markup(ex);
            let mut s1 = raw_analyze(&ctx, &clean);
            ctx.disambiguator.apply_rules_through(&mut s1, prefix_stop);
            let before = snapshot(&s1);
            ctx.disambiguator.apply_rule(&mut s1, idx);
            let after = snapshot(&s1);
            if before != after {
                failures.push(format!("FAIL: [{id}] untouched-touched: '{clean}'"));
            }
        }

        for ex in ambiguous {
            ambiguous_total += 1;
            let (Some(input_form), Some(output_form)) = (&ex.input_form, &ex.output_form) else {
                continue;
            };
            let (clean, marker) = strip_markup(&ex.text);
            let Some((marker_start, _)) = marker else {
                failures.push(format!("FAIL: [{id}] no-marker: '{clean}'"));
                continue;
            };
            let mut s1 = raw_analyze(&ctx, &clean);
            ctx.disambiguator.apply_rules_through(&mut s1, prefix_stop);
            let input_ok = marked_token(&s1, marker_start, input_form)
                .map(forms_string)
                .is_some_and(|f| &f == input_form);
            ctx.disambiguator.apply_rule(&mut s1, idx);
            let output_form_actual = marked_token(&s1, marker_start, output_form).map(forms_string);
            let output_ok = output_form_actual
                .as_deref()
                .is_some_and(|f| f == output_form.as_str());
            if !input_ok {
                failures.push(format!(
                    "FAIL: [{id}] input-form: '{clean}' expected {input_form} got {}",
                    output_form_actual.unwrap_or_else(|| "<no token>".into())
                ));
            } else if !output_ok {
                failures.push(format!(
                    "FAIL: [{id}] output-form: '{clean}' expected {output_form} got {}",
                    output_form_actual.unwrap_or_else(|| "<no token>".into())
                ));
            }
        }
    }

    println!(
        "{lang}: {untouched_total} untouched + {ambiguous_total} ambiguous examples, {} failures",
        failures.len()
    );
    for f in failures.iter().take(60) {
        println!("{f}");
    }
    let out_path = format!("/tmp/disamb_failures_{lang}.txt");
    std::fs::write(&out_path, failures.join("\n") + "\n").ok();
    println!("failures written to {out_path}");
}

fn raw_analyze(ctx: &Ctx, text: &str) -> AnalyzedSentence {
    let raw = match &ctx.tokenizer {
        TokenizerChoice::En(tokenizer) => tokenizer.split_underscores(tokenizer.tokenize(text)),
        TokenizerChoice::Fr(tokenizer) => tokenizer.tokenize(text),
        TokenizerChoice::Plain => {
            let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
            tokenizer.split_underscores(tokenizer.tokenize(text))
        }
    };
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
    let mut sentence = AnalyzedSentence::new(text, 0, text.len());
    sentence.set_tokens(tokens);
    Tagger::tag(ctx.tagger.as_ref(), sentence.tokens_mut());
    og_tagger::chunker::load_opennlp_chunker(&ctx.resource_dir);
    og_tagger::chunker::apply_chunk_tags(&mut sentence);
    if let Some(last) = sentence.tokens_mut().last_mut() {
        last.add_sent_end();
    }
    sentence
}

fn strip_comments(xml: &str) -> String {
    let mut cleaned = String::with_capacity(xml.len());
    let mut i = 0usize;
    while i < xml.len() {
        if xml.is_char_boundary(i) && xml[i..].starts_with("<!--") {
            match xml[i + 4..].find("-->") {
                Some(rel) => i += 4 + rel + 3,
                None => i = xml.len(),
            }
        } else {
            let step = xml[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            cleaned.push_str(&xml[i..i + step]);
            i += step;
        }
    }
    cleaned
}
