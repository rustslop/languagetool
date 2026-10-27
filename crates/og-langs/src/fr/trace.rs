use crate::fr::test_support::FrenchPipeline;
use og_core::AnalyzedSentence;
use og_tokenizer::WordTokenizer as _;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::path::PathBuf;

fn rules_dir() -> PathBuf {
    crate::fr::test_support::fr_rules_dir().expect("fr rules dir")
}

fn compiled_grammar() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = rules_dir().join("grammar.xml");
        let xml = crate::fr::normalize_fr_rules_xml(
            &std::fs::read_to_string(&path).expect("grammar.xml"),
        )
        .into_owned();
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile grammar.xml"),
        ))
    })
    .expect("fr grammar")
}

fn compiled_style() -> std::sync::Arc<CompiledRuleSet> {
    static STYLE: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&STYLE, || {
        let path = rules_dir().join("style.xml");
        let xml =
            crate::fr::normalize_fr_rules_xml(&std::fs::read_to_string(&path).expect("style.xml"))
                .into_owned();
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile style.xml"),
        ))
    })
    .expect("fr style")
}

fn rules_owning_example<'a>(
    grammar: &'a CompiledRuleSet,
    style: &'a CompiledRuleSet,
    text: &str,
) -> Vec<(&'a CompiledRule, &'static str)> {
    let mut out = Vec::new();
    for (set, label) in [(grammar, "grammar.xml"), (style, "style.xml")] {
        for rule in &set.rules {
            if rule.examples.iter().any(|e| {
                let clean = e.text.replace("<marker>", "").replace("</marker>", "");
                e.text.contains(text) || clean.contains(text)
            }) {
                out.push((rule, label));
            }
        }
    }
    out
}

fn example_uses_raw_pos(fragment: &str) -> bool {
    // raw_pos is not compiled by the shared XML parser, so the flag is read
    // back from the rule XML: its absence from CompiledRule is the gap under diagnosis.
    for file in ["grammar.xml", "style.xml"] {
        let xml = std::fs::read_to_string(rules_dir().join(file)).expect("rules xml");
        let mut rest = xml.as_str();
        while let Some(offset) = rest.find(fragment) {
            let pos = xml.len() - rest.len() + offset;
            let before = xml[..pos].rfind("<rule").unwrap_or(0);
            let after = xml[pos..].find("</rule>").map(|i| pos + i).unwrap_or(pos);
            if xml[before..after].contains("raw_pos=\"yes\"") {
                return true;
            }
            rest = &xml[pos + fragment.len()..];
        }
    }
    false
}

fn dump_readings(sentence: &AnalyzedSentence) {
    for t in sentence.non_whitespace_tokens() {
        if t.token().token() == "<S>" {
            continue;
        }
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
            .collect();
        eprintln!(
            "    [{: >3}] {:?}: {}",
            t.token().start(),
            t.token().token(),
            readings.join(", ")
        );
    }
}

fn marker_range(text: &str) -> Option<(usize, usize)> {
    let s = text.find("<marker>")?;
    let e = text.find("</marker>")?;
    Some((s, e - "<marker>".len()))
}

pub fn probe(example_fragment: &str) {
    let engine = crate::fr::example_suite::build_pattern_engine();
    let pipeline = FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let raw_pos = example_uses_raw_pos(example_fragment);
    let grammar = compiled_grammar();
    let style = compiled_style();
    for (rule, file) in rules_owning_example(&grammar, &style, example_fragment) {
        let example = rule
            .examples
            .iter()
            .find(|e| {
                let clean = e.text.replace("<marker>", "").replace("</marker>", "");
                e.text.contains(example_fragment) || clean.contains(example_fragment)
            })
            .expect("example");
        let clean = example
            .text
            .replace("<marker>", "")
            .replace("</marker>", "");
        eprintln!(
            "== [{file}] {} / {:?} [type {:?}] raw_pos={raw_pos} filter={:?} '{}'",
            rule.id,
            rule.sub_id,
            example.example_type,
            rule.filter.as_ref().map(|f| f.class.as_str()),
            clean
        );
        if let Some(range) = marker_range(&example.text) {
            eprintln!("   marker {}..{}", range.0, range.1);
        }
        for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean) {
            eprintln!("  sentence @{sentence_start}: [{}]", sentence.text());
            dump_readings(&sentence);
            for m in engine.match_rule(rule, &sentence) {
                let span = m.offset()..m.offset() + m.length();
                eprintln!(
                    "  raw match {}..{} '{}'",
                    span.start,
                    span.end,
                    &sentence.text()[span.start.min(sentence.text().len())
                        ..span.end.min(sentence.text().len())]
                );
                let fr_filter = rule.filter.as_ref().filter(|f| {
                    f.class == crate::fr::suppress_misspelled::FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS
                });
                if let Some(filter) = fr_filter {
                    let filtered =
                        crate::fr::suppress_misspelled::accept_rule_match(m, &filter.args, &[]);
                    match filtered {
                        Some(m) => eprintln!(
                            "    filtered: kept @{}..{} reps={:?}",
                            m.offset() + sentence_start,
                            m.offset() + sentence_start + m.length(),
                            m.replacements()
                                .iter()
                                .map(|r| r.value().to_string())
                                .collect::<Vec<_>>()
                        ),
                        None => eprintln!("    filtered: NULLED"),
                    }
                }
            }
        }
    }
}

pub fn probe_dict(words: &[&str]) {
    let pipeline = FrenchPipeline::shared();
    let tagger = pipeline.tagger_ref();
    let tagged = og_tagger::Tagger::tag(tagger, words);
    for (word, atr) in words.iter().zip(tagged.iter()) {
        let readings: Vec<String> = atr
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
            .collect();
        eprintln!("  {word:?}: {}", readings.join(", "));
    }
}

pub fn probe_tokens(texts: &[&str]) {
    let pipeline = FrenchPipeline::shared();
    for text in texts {
        let tokens: Vec<String> = pipeline
            .tokenizer_ref()
            .tokenize(text)
            .iter()
            .map(|t| format!("{:?}", t.text()))
            .collect();
        eprintln!("TOKENS {text:?} => {}", tokens.join(" "));
    }
}

pub fn probe_inline_rule(rule_id: &str, text: &str) {
    let engine = crate::fr::example_suite::build_pattern_engine();
    let pipeline = FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    for (set, file) in [
        (compiled_grammar(), "grammar.xml"),
        (compiled_style(), "style.xml"),
    ] {
        for rule in &set.rules {
            if rule.id != rule_id {
                continue;
            }
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                let matches = engine.match_rule(rule, &sentence);
                eprintln!(
                    "INLINE [{file}] {} / {:?} => {} matches",
                    rule.id,
                    rule.sub_id,
                    matches.len()
                );
                for m in &matches {
                    eprintln!(
                        "  match {}..{} '{}'",
                        m.offset() + sentence_start,
                        m.offset() + sentence_start + m.length(),
                        &text[(m.offset() + sentence_start).min(text.len())
                            ..(m.offset() + sentence_start + m.length()).min(text.len())]
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn trace_pre_disambiguation_residue_failures() {
        let _release_guard = crate::release_guard();
        for fragment in [
            "Il faut tout",
            "cache sous la table",
            "passe sous la table",
            "attent-il",
            "repentir",
            "ce</marker> lever",
            "n'aurais pas",
        ] {
            super::probe(fragment);
        }
    }

    #[test]
    #[ignore]
    fn trace_tokenizer_shape_residue_failures() {
        let _release_guard = crate::release_guard();
        for fragment in [
            "32.1.1985",
            "300.1.1985",
            "3000.1.1985",
            "31.4",
            "30.2",
            "31.2",
            "29.2.1900",
            "29.2.2100",
            "8%</marker>",
            "8,5%</marker>",
            "51% des",
            "est arrivé à l'hôtel",
            "à la télévision",
            "m'éclairer",
        ] {
            super::probe(fragment);
        }
        super::probe_tokens(&[
            "Il est né le 32.1.1985.",
            "Une croissance de 8%.",
            "Une croissance de 8,5%.",
            "Le candidat a obtenu 51% des voies.",
            "-il est arrivé à l'hôtel hier.",
            "Vas -tu les regarder à la télévision ?",
            "Quelqu\"un peut -il m'éclairer et si possible en justifiant.",
            "D´enfants.",
            "D‘enfants.",
            "D′enfants.",
        ]);
    }

    #[test]
    #[ignore]
    fn trace_matcher_residue_failures() {
        let _release_guard = crate::release_guard();
        for fragment in [
            "la fois :",
            "203 de Schmutz",
            "la pêcherie",
            "post précédent",
            "néo-post-castristes",
            "post saison 14,3",
            "tous vos",
            "de mes",
            "ni ami ni amie",
            "nuance lilas",
            "néo-impressionnisme",
            "ou vous téléphonerai",
            "d'énergie croissant",
            "est donc né ce lieu",
            "sites parles",
            "Il fait frais",
            "usage de son bras",
            "de plus en plus",
            "peut tourner sur 360",
            "poursoi",
            "négligé les enqêteurs",
            "ne sé",
            "D‘enfants",
        ] {
            super::probe(fragment);
        }
    }

    #[test]
    #[ignore]
    fn trace_marker_offset_residue_failures() {
        let _release_guard = crate::release_guard();
        for fragment in ["il faut partir", "il est certain"] {
            super::probe(fragment);
        }
    }

    #[test]
    #[ignore]
    fn trace_unclassified_residue_failures() {
        let _release_guard = crate::release_guard();
        for fragment in [
            "Antidote, le remède",
            "Docteur délivrez",
            "Il devient de plus en plus",
            "écrirai ou vous téléphonerai",
            "qu'aurait négligé",
            "D‘",
            "est par ordre de niveau",
            "rencontre est donc né",
            "peut-être élevé",
            "étudiant ou étudiante",
            "ni ami ni amie",
            "nuance lilas",
        ] {
            super::probe(fragment);
        }
    }

    #[test]
    #[ignore]
    fn trace_style_residue_failure() {
        let _release_guard = crate::release_guard();
        super::probe("étudiant ou étudiante");
    }

    #[test]
    #[ignore]
    fn trace_il_devient_quoi_variants() {
        let _release_guard = crate::release_guard();
        for text in [
            "Il devient grand.",
            "Il devient vraiment grand.",
            "Il devient vraiment très grand.",
            "Il devient vraiment très très grand.",
            "Il devient de plus en plus.",
            "Il reste grand.",
            "Il semble grand.",
        ] {
            super::probe_inline_rule("IL_DEVIENT_QUOI", text);
        }
    }

    #[test]
    #[ignore]
    fn trace_inline_residue_rules() {
        let _release_guard = crate::release_guard();
        super::probe_inline_rule("APOS_INCORRECT", "D‘enfants.");
        super::probe_inline_rule("APOS_INCORRECT", "D´enfants.");
        super::probe_inline_rule("APOS_INCORRECT", "D′enfants.");
    }

    #[test]
    #[ignore]
    fn trace_poursoi_variants() {
        let _release_guard = crate::release_guard();
        for text in [
            "Il le fait poursoi.",
            "Il le fait poursoi le chemin.",
            "Ils le font ensuivant le chemin.",
            "Ils le font ensuivant.",
        ] {
            super::probe_inline_rule("MOTS_COLLES_OU_SEPARES", text);
        }
    }

    #[test]
    #[ignore]
    fn trace_dict_readings_of_suspect_words() {
        let _release_guard = crate::release_guard();
        super::probe_dict(&[
            "néo-impressionnisme",
            "néo-post-castristes",
            "Diaz-Canel",
            "post",
            "précédent",
            "castristes",
            "saison",
            "repas",
            "mots",
            "maux",
            "voies",
            "cache",
            "lever",
            "dire",
            "poursoi",
            "alors",
        ]);
    }

    #[test]
    #[ignore]
    fn trace_nuance_lilas_dict_readings() {
        let _release_guard = crate::release_guard();
        super::probe_dict(&[
            "Ses",
            "grandes",
            "fleurs",
            "doubles",
            "sont",
            "roses",
            "à",
            "nuance",
            "lilas",
            "et",
            "très",
            "parfumées",
            ".",
        ]);
    }
}
