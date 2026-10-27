use crate::es::trace::staged_sentence;
use std::collections::BTreeMap;

fn sort_forms(forms: &str) -> String {
    let Some(open) = forms.find('[') else {
        return forms.to_string();
    };
    if forms == ",[,]" {
        return forms.to_string();
    }
    let word = &forms[..=open];
    let inner = &forms[open + 1..forms.len() - 1];
    let mut parts: Vec<&str> = inner.split(',').collect();
    parts.sort_unstable();
    format!("{word}{}", parts.join(","))
}

fn token_forms(t: &og_core::AnalyzedTokenReadings) -> String {
    let readings: Vec<String> = t
        .readings()
        .iter()
        .map(|r| format!("{}/{}", r.lemma().unwrap_or("null"), r.pos_tags().join("+")))
        .collect();
    format!("{}[{}]", t.token().token(), readings.join(","))
}

fn clean_xml(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == '<' {
            depth += 1;
            i += 1;
            continue;
        }
        if bytes[i] == '>' && depth > 0 {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            out.push(bytes[i]);
        }
        i += 1;
    }
    out
}

#[test]
#[ignore]
fn spanish_disambiguation_examples_match_java_forms() {
    let _release_guard = crate::release_guard();
    let resource_dir = crate::es::es_resource_dir().expect("es dir");
    let raw = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
    let xml = og_xml::parser::XmlRuleParser::expand_entities_with_base(&raw, Some(&resource_dir));
    let d = og_tagger::XmlDisambiguator::from_xml(&xml).unwrap();
    let n = d.rule_count();
    let mut input_fails = 0usize;
    let mut output_fails = 0usize;
    let mut checked = 0usize;
    let mut by_rule: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut lines: Vec<String> = Vec::new();
    let mut untouched_checked = 0usize;
    let mut untouched_fails = 0usize;
    let sentence_string = |s: &og_core::AnalyzedSentence| {
        s.tokens()
            .iter()
            .map(|t| {
                format!(
                    "{:?}:{:?}",
                    t.token().token(),
                    t.readings()
                        .iter()
                        .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                        .collect::<Vec<_>>()
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    for idx in 0..n {
        let (untouched, examples) = d.rule_examples(idx);
        for raw in untouched {
            let clean = clean_xml(&raw.replace(['\n', '\t'], ""));
            if clean.trim().is_empty() {
                continue;
            }
            let mut before = staged_sentence(&clean);
            if let Some(last) = before.tokens_mut().last_mut() {
                last.add_sent_end();
            }
            d.apply_rules_through(&mut before, idx);
            let mut after = staged_sentence(&clean);
            if let Some(last) = after.tokens_mut().last_mut() {
                last.add_sent_end();
            }
            d.apply_rules_through(&mut after, idx + 1);
            untouched_checked += 1;
            if sentence_string(&before) != sentence_string(&after) {
                untouched_fails += 1;
                let id = d.rule_id(idx).unwrap_or("?");
                eprintln!("UNTOUCHED [{id}] changed: {}", clean.trim());
            }
        }
        for ex in examples {
            let (Some(input), Some(output)) = (&ex.input_form, &ex.output_form) else {
                continue;
            };
            let raw = &ex.text;
            let Some(marker_start) = raw.find("<marker>") else {
                continue;
            };
            let clean = clean_xml(raw);
            let mut base = staged_sentence(&clean);
            if let Some(last) = base.tokens_mut().last_mut() {
                last.add_sent_end();
            }
            d.apply_rules_through(&mut base, idx);
            let get_marker = |s: &og_core::AnalyzedSentence| -> Option<String> {
                s.tokens()
                    .iter()
                    .find(|t| t.token().start() == marker_start)
                    .map(token_forms)
            };
            let Some(before) = get_marker(&base) else {
                continue;
            };
            checked += 1;
            let mut after_sent = staged_sentence(&clean);
            if let Some(last) = after_sent.tokens_mut().last_mut() {
                last.add_sent_end();
            }
            d.apply_rules_through(&mut after_sent, idx + 1);
            let after = get_marker(&after_sent).unwrap_or_default();
            let id = d.rule_id(idx).unwrap_or("?").to_string();
            let entry = by_rule.entry(id.clone()).or_insert((0, 0));
            if sort_forms(&before) != sort_forms(input) {
                input_fails += 1;
                entry.0 += 1;
                lines.push(format!(
                    "IN  [{id}] {input} != {before} :: {}",
                    clean.trim()
                ));
            } else if sort_forms(&after) != sort_forms(output) {
                output_fails += 1;
                entry.1 += 1;
                lines.push(format!(
                    "OUT [{id}] {output} != {after} :: {}",
                    clean.trim()
                ));
            }
        }
    }
    eprintln!(
        "disambig examples: {checked} checked, input_fails={input_fails}, output_fails={output_fails}; untouched: {untouched_checked} checked, {untouched_fails} failed"
    );
    for (id, (i, o)) in &by_rule {
        if *i > 0 || *o > 0 {
            eprintln!("  {id}: in={i} out={o}");
        }
    }
    for l in lines.iter().take(400) {
        eprintln!("{l}");
    }
}

#[test]
fn global_disambiguation_marks_user_mentions_ignored_by_speller() {
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let sentence = &pipeline.analyze_text("Escribe a @usuario ya.")[0];
    let mention = sentence
        .tokens()
        .iter()
        .find(|t| t.token().token() == "@usuario")
        .expect("@usuario token");
    assert!(
        mention.is_ignored_by_speller(),
        "disambiguation-global.xml GLOBAL_IGNORE_USER_MENTION must ignore-spell @usuario"
    );
}

#[test]
fn perifrasis_verbal_addchunk_appends_gv_chunks() {
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let sentence = &pipeline.analyze_text("Tengo que comer.")[0];
    for word in ["Tengo", "que", "comer"] {
        let token = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == word)
            .unwrap_or_else(|| panic!("{word} token"));
        assert!(
            token.has_chunk("GV"),
            "{word} must carry the GV chunk (ADDCHUNK): chunks={:?}",
            token.chunks()
        );
    }
    let fuera = &pipeline.analyze_text("Comí fuera de casa.")[0];
    let comi = fuera
        .tokens()
        .iter()
        .find(|t| t.token().token() == "Comí")
        .expect("Comí token");
    assert!(
        !comi.has_chunk("GV"),
        "non-periphrasis verbs get no GV chunk: chunks={:?}",
        comi.chunks()
    );
}
