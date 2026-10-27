use crate::pt::test_support::PortuguesePipeline;
use og_core::AnalyzedSentence;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::path::PathBuf;

fn rules_dir() -> PathBuf {
    crate::pt::filters::pt_rules_dir().expect("pt rules dir")
}

fn compiled_grammar() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = rules_dir().join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile rules xml"),
        ))
    })
    .expect("pt grammar")
}

fn dump_readings(sentence: &AnalyzedSentence) {
    for t in sentence.non_whitespace_tokens() {
        let readings: Vec<String> = t
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
            .collect();
        eprintln!(
            "    [{: >2}] {:?}: {}",
            t.token().start(),
            t.token().token(),
            readings.join(", ")
        );
    }
}

fn rules_owning_example<'a>(grammar: &'a CompiledRuleSet, text: &str) -> Vec<&'a CompiledRule> {
    grammar
        .rules
        .iter()
        .filter(|r| r.examples.iter().any(|e| e.text.contains(text)))
        .collect()
}

pub fn probe(example_fragment: &str) {
    let engine = crate::pt::example_suite::suite_engine();
    let synthesizer = crate::pt::example_suite::suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let grammar = compiled_grammar();
    for rule in rules_owning_example(&grammar, example_fragment) {
        let example = rule
            .examples
            .iter()
            .find(|e| e.text.contains(example_fragment))
            .expect("example");
        let clean = example
            .text
            .replace("<marker>", "")
            .replace("</marker>", "");
        eprintln!(
            "== {} / {:?} [type {:?}] filter={:?} '{}'",
            rule.id,
            rule.sub_id,
            example.example_type,
            rule.filter.as_ref().map(|f| f.class.as_str()),
            clean
        );
        for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean) {
            let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
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
                match rule.filter.as_ref() {
                    Some(filter) => {
                        let (positions, texts) =
                            crate::pt::example_suite::matched_window(rule, &tokens, &m);
                        eprintln!("    window positions={positions:?} texts={texts:?}");
                        let filtered = crate::pt::example_suite::apply_suite_filter(
                            rule,
                            filter,
                            &sentence,
                            &tokens,
                            synthesizer.as_ref(),
                            m,
                        );
                        match filtered {
                            Some(m) => eprintln!(
                                "    filtered: kept @{}..{} reps={:?}",
                                m.offset(),
                                m.offset() + m.length(),
                                m.replacements()
                                    .iter()
                                    .map(|r| r.value().to_string())
                                    .collect::<Vec<_>>()
                            ),
                            None => eprintln!("    filtered: NULLED"),
                        }
                    }
                    None => eprintln!("    (no filter on this rule)"),
                }
            }
        }
    }
}

pub(crate) fn probe_dict(words: &[&str]) {
    let pipeline = PortuguesePipeline::shared();
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

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn trace_six_dispatch_residue_failures() {
        let _release_guard = crate::release_guard();
        super::probe("das mares");
        super::probe("milênios atras");
        super::probe("traslado ao aeroporto");
        super::probe("discretamente");
        super::probe("impresso</marker>");
        super::probe("mil ienes");
        super::probe("teriam-na");
    }

    #[test]
    #[ignore]
    fn trace_dict_readings_of_suspect_words() {
        let _release_guard = crate::release_guard();
        super::probe_dict(&[
            "das",
            "mares",
            "marés",
            "milênios",
            "milênio",
            "atras",
            "Quito",
            "quito",
            "limpo",
            "impresso",
            "imprimido",
            "mil",
            "gastado",
            "gasto",
            "aos",
            "ao",
            "no",
            "na",
        ]);
        let alt = std::path::PathBuf::from("/tmp/ptdict_probe");
        if alt.join("portuguese.dict").exists() {
            let tagger = super::super::tagger::PortugueseTagger::from_resource_dir(&alt);
            let words = [
                "das",
                "milênios",
                "milênio",
                "mil",
                "limpo",
                "impresso",
                "quito",
                "Quito",
                "mares",
            ];
            let tagged = og_tagger::Tagger::tag(&tagger, &words);
            eprintln!("  == alternate snapshot /tmp/ptdict_probe ==");
            for (word, atr) in words.iter().zip(tagged.iter()) {
                let readings: Vec<String> = atr
                    .readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                    .collect();
                eprintln!("  {word:?}: {}", readings.join(", "));
            }
        }
    }
}

fn compiled_style() -> std::sync::Arc<CompiledRuleSet> {
    static STYLE: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&STYLE, || {
        let path = rules_dir().join("style.xml");
        let xml = std::fs::read_to_string(&path).expect("style.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile style.xml"),
        ))
    })
    .expect("pt style")
}

fn describe_token(t: &og_xml::compiler::CompiledPatternToken) -> String {
    let mut s = String::new();
    if t.negate {
        s.push('!');
    }
    match &t.text {
        Some(text) => s.push_str(&format!("{text:?}")),
        None => s.push('*'),
    }
    if t.inflected {
        s.push_str("(infl)");
    }
    if let Some(postag) = &t.postag {
        s.push_str(&format!(
            "/{}{}",
            if t.negate_pos { "!" } else { "" },
            postag
        ));
    }
    if let Some(min) = t.min {
        s.push_str(&format!("{{{min}..{}}}", t.max.unwrap_or(-1)));
    }
    if t.skip != 0 {
        s.push_str(&format!("@skip{}", t.skip));
    }
    for exc in &t.exceptions {
        s.push_str(&format!(
            " -exc{:?}{}",
            exc.text.as_deref().unwrap_or(""),
            exc.scope
                .as_ref()
                .map(|sc| format!(":{sc}"))
                .unwrap_or_default()
        ));
    }
    s
}

fn describe_element(elem: &og_xml::compiler::CompiledPatternElement) -> String {
    use og_xml::compiler::CompiledPatternElement;
    match elem {
        CompiledPatternElement::Token(t) => describe_token(t),
        CompiledPatternElement::OrGroup(alts) => {
            let inner: Vec<String> = alts.iter().map(describe_token).collect();
            format!("or[{}]", inner.join("|"))
        }
        CompiledPatternElement::AndGroup(all) => {
            let inner: Vec<String> = all.iter().map(describe_token).collect();
            format!("and[{}]", inner.join("&"))
        }
    }
}

#[cfg(test)]
mod round1_tests {
    use super::PortuguesePipeline;
    use super::XmlCompiler;

    #[test]
    #[ignore]
    fn trace_round1_global_disambiguation_and_style_residue() {
        let _release_guard = crate::release_guard();
        super::probe("google.com.br");
        let engine = crate::pt::example_suite::suite_engine();
        let style = crate::pt::filters::pt_rules_dir()
            .expect("pt rules dir")
            .join("style.xml");
        let xml = std::fs::read_to_string(&style).expect("style.xml");
        let rule_set = XmlCompiler::new()
            .compile_file_with_base(&xml, style.parent())
            .expect("compile style.xml");
        let pipeline = PortuguesePipeline::shared();
        for (id, text) in [
            (
                "CIENTÍFICO_TERMOS_TÉCNICOS_TERMINOLOGIA",
                "Os termos técnicos são importantes.",
            ),
            (
                "SIMPLIFICAR_QUE_PRON_V_ANDAR_A_INFINITIVO",
                "Há décadas que me andava a arrastar sem objetivos.",
            ),
        ] {
            eprintln!("== {id} '{text}'");
            for rule in rule_set.rules.iter().filter(|r| r.id == id) {
                for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                    for m in engine.match_rule(rule, &sentence) {
                        eprintln!(
                            "  sub {:?}: match {}..{} reps={:?}",
                            rule.sub_id,
                            m.offset() + sentence_start,
                            m.offset() + sentence_start + m.length(),
                            m.replacements()
                                .iter()
                                .map(|r| r.value().to_string())
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod round2_tests {
    use super::PortuguesePipeline;
    use super::{compiled_style, describe_element};
    use og_xml::compiler::CompiledRule;

    struct FailureLine {
        rule_id: String,
        outcome: &'static str,
        text: String,
    }

    fn parse_artifact(path: &str) -> Vec<FailureLine> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                if !line.starts_with("FAIL: [") {
                    return None;
                }
                let id_end = line.find(']')?;
                let rule_id = line[7..id_end].to_string();
                let rest = &line[id_end + 1..];
                let text_start = rest.find(": '")? + 3;
                let text = rest[text_start..].strip_suffix('\'')?.to_string();
                let outcome = if rest.starts_with(" Expected match") {
                    "no_match"
                } else if rest.starts_with(" Expected no match") {
                    "fp"
                } else {
                    "marker"
                };
                Some(FailureLine {
                    rule_id,
                    outcome,
                    text,
                })
            })
            .collect()
    }

    fn dump_compact_readings(sentence: &og_core::AnalyzedSentence) {
        for t in sentence.non_whitespace_tokens() {
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}:{}", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                .collect();
            eprintln!("      {:?} = {}", t.token().token(), readings.join(" | "));
        }
    }

    #[test]
    #[ignore]
    fn trace_round2_style_residue_families() {
        let _release_guard = crate::release_guard();
        let failures = parse_artifact("/tmp/test_failures_pt_style.txt");
        assert!(
            !failures.is_empty(),
            "artifact /tmp/test_failures_pt_style.txt is empty; run portuguese_style_examples_with_pt_pipeline first"
        );
        let engine = crate::pt::example_suite::suite_engine();
        let pipeline = PortuguesePipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);

        let mut order: Vec<String> = Vec::new();
        for f in &failures {
            if !order.contains(&f.rule_id) {
                order.push(f.rule_id.clone());
            }
        }
        for rule_id in &order {
            let mine: Vec<&FailureLine> =
                failures.iter().filter(|f| &f.rule_id == rule_id).collect();
            let style = compiled_style();
            let sub_rules: Vec<&CompiledRule> =
                style.rules.iter().filter(|r| &r.id == rule_id).collect();
            if sub_rules.is_empty() {
                eprintln!("### {rule_id} x{} has no style.xml sub-rule", mine.len());
                continue;
            }
            let outcomes: Vec<&str> = mine.iter().map(|f| f.outcome).collect();
            eprintln!(
                "### {rule_id} x{} [{}] subs={} filter={:?}",
                mine.len(),
                outcomes.join(","),
                sub_rules.len(),
                sub_rules[0].filter.as_ref().map(|f| f.class.as_str())
            );
            for (si, rule) in sub_rules.iter().enumerate() {
                let elems: Vec<String> =
                    rule.pattern.elements.iter().map(describe_element).collect();
                eprintln!("  sub#{si} {:?} elems: {}", rule.sub_id, elems.join(" "));
                for (ai, anti) in rule.antipatterns.iter().enumerate() {
                    let elems: Vec<String> = anti.elements.iter().map(describe_element).collect();
                    eprintln!("    anti#{ai}: {}", elems.join(" "));
                }
            }
            let mut seen_texts: Vec<&str> = Vec::new();
            for f in &mine {
                if seen_texts.contains(&f.text.as_str()) {
                    continue;
                }
                seen_texts.push(&f.text);
                eprintln!("  -- [{}] '{}'", f.outcome, f.text);
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&f.text) {
                    dump_compact_readings(&sentence);
                    let _tokens: Vec<&og_core::AnalyzedTokenReadings> =
                        sentence.non_whitespace_tokens();
                    for (si, rule) in sub_rules.iter().enumerate() {
                        og_rules::pattern_trace::clear();
                        og_rules::pattern_trace::set_enabled(true);
                        let mut matched = false;
                        for m in engine.match_rule(rule, &sentence) {
                            matched = true;
                            eprintln!(
                                "    sub#{si} MATCH {}..{} '{}'",
                                m.offset() + sentence_start,
                                m.offset() + sentence_start + m.length(),
                                &sentence.text()[m.offset()
                                    ..(m.offset() + m.length()).min(sentence.text().len())]
                            );
                        }
                        og_rules::pattern_trace::set_enabled(false);
                        if !matched && f.outcome == "no_match" {
                            let events = og_rules::pattern_trace::take();
                            if let Some(max_pat) = events.iter().map(|e| e.pattern_idx).max() {
                                let deepest = events
                                    .iter()
                                    .filter(|e| e.pattern_idx == max_pat)
                                    .min_by_key(|e| e.token_idx)
                                    .unwrap();
                                eprintln!(
                                    "    sub#{si} stall elem#{} tok={:?}: {}",
                                    deepest.pattern_idx, deepest.token, deepest.cause
                                );
                            } else {
                                eprintln!("    sub#{si} no trace events");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod round2_puzzles {
    use super::PortuguesePipeline;
    use super::compiled_style;

    fn dump_all_events(rule_id: &str, text: &str) {
        let engine = crate::pt::example_suite::suite_engine();
        let pipeline = PortuguesePipeline::shared();
        let style = compiled_style();
        let sub_rules: Vec<_> = style.rules.iter().filter(|r| r.id == rule_id).collect();
        eprintln!("== {rule_id} '{text}'");
        for rule in &sub_rules {
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                og_rules::pattern_trace::clear();
                og_rules::pattern_trace::set_enabled(true);
                let matches = engine.match_rule(rule, &sentence);
                og_rules::pattern_trace::set_enabled(false);
                let events = og_rules::pattern_trace::take();
                eprintln!("  sub {:?}: {} raw matches", rule.sub_id, matches.len());
                for m in &matches {
                    eprintln!(
                        "    MATCH {}..{} '{}'",
                        m.offset() + sentence_start,
                        m.offset() + sentence_start + m.length(),
                        &sentence.text()
                            [m.offset()..(m.offset() + m.length()).min(sentence.text().len())]
                    );
                }
                for e in &events {
                    eprintln!(
                        "    ev elem#{} tok#{:?} {:?}: {}",
                        e.pattern_idx, e.token_idx, e.token, e.cause
                    );
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn trace_round2_stall_puzzles() {
        let _release_guard = crate::release_guard();
        dump_all_events(
            "SIMPLIFICAR_POIS_VERBO_AO_VERBOINF",
            "O stress é importante, pois, afeta o comportamento humano.",
        );
        dump_all_events(
            "QUANDO_TER_DE_QUE_VINF_AO_VINF",
            "Quando tenho de lidar com muitos elementos.",
        );
        dump_all_events(
            "ERRO_TRADUÇÃO_COBRIR_ABRANGER",
            "Em relação a temas não cobertos por este documento.",
        );
        dump_all_events(
            "CIENTÍFICO_EMPREGAR_TERMO",
            "Na nossa pesquisa usa-se o termo “dualismo”.",
        );
        dump_all_events(
            "SER_CONSISTIR_RESIDIR",
            "Já que existe diferença entre monarquia parlamentarista x monarquia absolutista como é nos países do Oriente Médio totalmente ditatorial.",
        );
        dump_all_events(
            "BÁSICO_ELEMENTAR",
            "Para mim não chegam as palavras despojadas e reduzidas ao básico: os taninos, os citrinos.",
        );
    }
}

#[cfg(test)]
mod round2_fix_impact {
    use super::compiled_style;
    use og_xml::compiler::{
        CompiledPattern, CompiledPatternElement, CompiledPatternToken, CompiledRule,
    };

    const BROKEN: &str = "^(?:no)$";

    fn fix_token(t: &mut CompiledPatternToken) {
        if t.compiled_postag_regexp
            .as_ref()
            .is_some_and(|re| re.as_str() == BROKEN)
        {
            t.compiled_postag_regexp = None;
            t.compiled_postag_regexp_fancy = None;
        }
        if t.compiled_regexp
            .as_ref()
            .is_some_and(|re| re.as_str() == BROKEN)
        {
            t.compiled_regexp = None;
            t.compiled_regexp_fancy = None;
        }
        for exc in &mut t.exceptions {
            if exc
                .compiled_regexp
                .as_ref()
                .is_some_and(|re| re.as_str() == BROKEN)
            {
                exc.compiled_regexp = None;
                exc.compiled_regexp_fancy = None;
            }
        }
    }

    fn fix_pattern(p: &mut CompiledPattern) {
        for t in &mut p.tokens {
            fix_token(t);
        }
        for e in &mut p.elements {
            match e {
                CompiledPatternElement::Token(t) => fix_token(t),
                CompiledPatternElement::OrGroup(alts) => {
                    for t in alts {
                        fix_token(t);
                    }
                }
                CompiledPatternElement::AndGroup(all) => {
                    for t in all {
                        fix_token(t);
                    }
                }
            }
        }
    }

    fn count_broken(rule: &CompiledRule) -> usize {
        fn tokens_of(p: &CompiledPattern) -> impl Iterator<Item = &CompiledPatternToken> {
            p.tokens.iter()
        }
        let main = tokens_of(&rule.pattern)
            .filter(|t| {
                t.compiled_postag_regexp
                    .as_ref()
                    .is_some_and(|re| re.as_str() == BROKEN)
                    || t.compiled_regexp
                        .as_ref()
                        .is_some_and(|re| re.as_str() == BROKEN)
            })
            .count();
        let excs = tokens_of(&rule.pattern)
            .flat_map(|t| t.exceptions.iter())
            .filter(|e| {
                e.compiled_regexp
                    .as_ref()
                    .is_some_and(|re| re.as_str() == BROKEN)
            })
            .count();
        let anti = rule
            .antipatterns
            .iter()
            .flat_map(tokens_of)
            .filter(|t| {
                t.compiled_postag_regexp
                    .as_ref()
                    .is_some_and(|re| re.as_str() == BROKEN)
                    || t.compiled_regexp
                        .as_ref()
                        .is_some_and(|re| re.as_str() == BROKEN)
            })
            .count();
        main + excs + anti
    }

    fn fixed_style_rules() -> Vec<CompiledRule> {
        compiled_style()
            .rules
            .iter()
            .map(|r| {
                let mut c = r.clone();
                fix_pattern(&mut c.pattern);
                for a in &mut c.antipatterns {
                    fix_pattern(a);
                }
                c
            })
            .collect()
    }

    #[test]
    #[ignore]
    fn trace_round2_no_attr_fix_impact_on_style_suite() {
        let _release_guard = crate::release_guard();
        let broken_tokens: usize = compiled_style().rules.iter().map(count_broken).sum();
        eprintln!("style.xml: tokens carrying the dead ^(?:no)$ regex: {broken_tokens}");
        let engine = crate::pt::example_suite::suite_engine();
        let pipeline = super::PortuguesePipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);
        let rules = fixed_style_rules();
        let mut groups: std::collections::HashMap<(String, Option<String>), Vec<&CompiledRule>> =
            std::collections::HashMap::new();
        for r in &rules {
            groups
                .entry((r.id.clone(), r.sub_id.clone()))
                .or_default()
                .push(r);
        }
        let mut group_list: Vec<_> = groups.into_values().collect();
        group_list.sort_by_key(|g| std::cmp::Reverse(g.len()));
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut fail_lines: Vec<String> = Vec::new();
        for group in &group_list {
            let Some(rule) = group.first() else { continue };
            for example in &rule.examples {
                let clean = example
                    .text
                    .replace("<marker>", "")
                    .replace("</marker>", "");
                let mut matches = 0usize;
                for aux in group.iter() {
                    for (sentence, _start) in pipeline.analyze_offset_text(&clean) {
                        matches += engine.match_rule(aux, &sentence).len();
                    }
                }
                let ok = match example.example_type {
                    og_xml::types::XmlExampleType::Incorrect => matches > 0,
                    og_xml::types::XmlExampleType::Correct => matches == 0,
                    og_xml::types::XmlExampleType::TriggersError => true,
                };
                if ok {
                    passed += 1;
                } else {
                    failed += 1;
                    fail_lines.push(format!("FAIL: [{}] {}", rule.id, clean));
                }
            }
        }
        eprintln!(
            "style.xml with regexp='no'/postag_regexp='no' simulated fix: {}/{} passed ({:.1}%)",
            passed,
            passed + failed,
            100.0 * passed as f64 / (passed + failed).max(1) as f64
        );
        let style_xml =
            std::fs::read_to_string(super::rules_dir().join("style.xml")).expect("style.xml");
        let without_comments = crate::pt::example_suite::strip_xml_comments_pub(&style_xml);
        let antipattern =
            crate::pt::example_suite::collect_style_antipattern_examples(&without_comments);
        let mut anti_passed = 0usize;
        let mut anti_failed = 0usize;
        let mut anti_lines: Vec<String> = Vec::new();
        let rule_index: std::collections::HashMap<(String, Option<String>), Vec<&CompiledRule>> = {
            let mut m: std::collections::HashMap<(String, Option<String>), Vec<&CompiledRule>> =
                std::collections::HashMap::new();
            for r in &rules {
                m.entry((r.id.clone(), r.sub_id.clone()))
                    .or_default()
                    .push(r);
            }
            m
        };
        for ((id, sub), texts) in &antipattern.by_key {
            let Some(targets) = rule_index.get(&(id.clone(), sub.clone())) else {
                continue;
            };
            for text in texts {
                let mut matches = 0usize;
                for aux in targets {
                    for (sentence, _) in pipeline.analyze_offset_text(text) {
                        matches += engine.match_rule(aux, &sentence).len();
                    }
                }
                if matches == 0 {
                    anti_passed += 1;
                } else {
                    anti_failed += 1;
                    anti_lines.push(format!(
                        "FAIL: [{id}/{} ANTIPATTERN]: {text}",
                        sub.clone().unwrap_or_default()
                    ));
                }
            }
        }
        eprintln!(
            "style.xml antipatterns under the simulated fix: {}/{} passed",
            anti_passed,
            anti_passed + anti_failed
        );
        let _ = std::fs::write(
            "/tmp/test_failures_pt_style_simfix.txt",
            fail_lines.join("\n") + "\n",
        );
        let _ = std::fs::write(
            "/tmp/test_failures_pt_style_simfix_antipatterns.txt",
            anti_lines.join("\n") + "\n",
        );
        eprintln!(
            "{} residual failures written to /tmp/test_failures_pt_style_simfix.txt",
            fail_lines.len()
        );
        for group in &group_list {
            let Some(rule) = group.first() else { continue };
            for example in &rule.examples {
                let clean = example
                    .text
                    .replace("<marker>", "")
                    .replace("</marker>", "");
                let mut total = 0usize;
                for aux in group.iter() {
                    for (sentence, _s) in pipeline.analyze_offset_text(&clean) {
                        total += engine.match_rule(aux, &sentence).len();
                    }
                }
                let ok = match example.example_type {
                    og_xml::types::XmlExampleType::Incorrect => total > 0,
                    og_xml::types::XmlExampleType::Correct => total == 0,
                    og_xml::types::XmlExampleType::TriggersError => true,
                };
                if ok {
                    continue;
                }
                eprintln!("RESIDUAL [{}] '{}'", rule.id, clean);
                for aux in group.iter() {
                    for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean) {
                        og_rules::pattern_trace::clear();
                        og_rules::pattern_trace::set_enabled(true);
                        let mut matched = false;
                        for m in engine.match_rule(aux, &sentence) {
                            matched = true;
                            eprintln!(
                                "  sub {:?} MATCH {}..{} '{}'",
                                aux.sub_id,
                                m.offset() + sentence_start,
                                m.offset() + sentence_start + m.length(),
                                &sentence.text()[m.offset()
                                    ..(m.offset() + m.length()).min(sentence.text().len())]
                            );
                        }
                        og_rules::pattern_trace::set_enabled(false);
                        if !matched
                            && example.example_type == og_xml::types::XmlExampleType::Incorrect
                        {
                            let events = og_rules::pattern_trace::take();
                            if let Some(max_pat) = events.iter().map(|e| e.pattern_idx).max() {
                                let deepest = events
                                    .iter()
                                    .filter(|e| e.pattern_idx == max_pat)
                                    .min_by_key(|e| e.token_idx)
                                    .unwrap();
                                eprintln!(
                                    "  sub {:?} stall elem#{} tok={:?}: {}",
                                    aux.sub_id, deepest.pattern_idx, deepest.token, deepest.cause
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod round2_dict {
    #[test]
    #[ignore]
    fn trace_round2_style_suspect_words() {
        let _release_guard = crate::release_guard();
        super::probe_dict(&[
            "resolver",
            "resolvem",
            "refiro",
            "revendo",
            "host",
            "holística",
            "planejar",
            "usamos",
            "usei",
            "usámos",
            "preso",
            "acrescentar",
            "amigas",
            "amigos",
            "honrosamente",
            "continuamente",
            "rapidamente",
            "tão-pouco",
            "termos",
            "importantes",
            "observam",
            "passou",
            "ocorre",
            "semana",
            "amanhecer",
            "mortos",
            "combate",
            "sobre-humano",
            "segunda-feira",
            "downloads",
            "ambientes",
            "desse",
            "deles",
            "nesta",
            "disso",
        ]);
    }
}

#[cfg(test)]
mod round2_pins {
    use super::PortuguesePipeline;

    fn tagger_tags(word: &str) -> Vec<String> {
        let pipeline = PortuguesePipeline::shared();
        let tagger = pipeline.tagger_ref();
        let tagged = og_tagger::Tagger::tag(tagger, &[word]);
        tagged[0]
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
            .collect()
    }

    fn pipeline_tokens(text: &str) -> Vec<String> {
        super::super::test_support::analyze_portuguese(text)
            .iter()
            .flat_map(|s| s.non_whitespace_tokens().to_vec())
            .map(|t| t.token().token().to_string())
            .collect()
    }

    #[test]
    fn style_residue_contraction_tags_are_plus_joined_truncated_chains() {
        assert_eq!(
            tagger_tags("das"),
            vec!["de+as/SPS00+*".to_string()],
            "Java pins das/[de:o]SPS00:DA0FP0 (PortugueseTaggerTest.java:87); the pinned 2016-era dict gives a plus-joined truncated chain"
        );
        assert_eq!(
            tagger_tags("deles"),
            vec!["de+eles/SPS00+PP3MP00".to_string()],
            "INIMIGO_ADVERSÁRIO's scope=next exception postag SPS00:PP3.+ can never fire on this reading"
        );
        assert!(tagger_tags("nesta") == vec!["em+esta/SPS00+*".to_string()]);
        assert!(tagger_tags("disso") == vec!["de+isso/SPS00+PD0NN00".to_string()]);
    }

    #[test]
    fn style_residue_enclitic_and_hyphen_words_are_untagged_and_split() {
        for word in [
            "aturam-me",
            "usa-se",
            "pedi-las",
            "segunda-feira",
            "sobre-humano",
        ] {
            let readings = tagger_tags(word);
            assert!(
                readings.is_empty() || readings.iter().all(|r| r.ends_with("/-")),
                "{word} must be dict-untagged so the tokenizer splits it (PortugueseWordTokenizer.java:161-171 parity): {readings:?}"
            );
        }
        assert_eq!(
            pipeline_tokens("Os meus pais aturam-me há décadas."),
            vec![
                "<S>", "Os", "meus", "pais", "aturam", "-", "me", "há", "décadas", "."
            ],
            "PortugueseWordTokenizer.java:161-171 keeps hyphen words whole iff the tagger tags them; aturam-me is untagged in this dict so it splits, shrinking ATURAR_SUPORTAR_TOLERAR's marker"
        );
        assert_eq!(
            pipeline_tokens("Quero isto para a segunda-feira de hoje."),
            vec![
                "<S>", "Quero", "isto", "para", "a", "segunda", "-", "feira", "de", "hoje", "."
            ]
        );
    }

    #[test]
    fn style_residue_missing_vocabulary_and_stale_readings() {
        for word in [
            "resolver",
            "resolvem",
            "refiro",
            "revendo",
            "downloads",
            "tão-pouco",
        ] {
            let readings = tagger_tags(word);
            assert!(
                readings.is_empty() || readings.iter().all(|r| r == &format!("{word}/-")),
                "{word} has no entry in the pinned dict (Java's v0.14 dict tags it): {readings:?}"
            );
        }
        assert!(
            !tagger_tags("usamos").iter().any(|r| r.contains("VMIS1P0")),
            "usamos carries only VMIP1P0; Java's dict has the VMIS1P0 homograph that REMOVER_EU_NÓS needs"
        );
        assert!(
            !tagger_tags("preso").iter().any(|r| r.contains("prender")),
            "preso lacks the prender/VMP00SM participle reading PRESO_RETIDO_V2 needs"
        );
        assert!(
            !tagger_tags("ambientes").iter().any(|r| r.contains("NC")),
            "ambientes carries only verb readings; QUE_SE_V_PRESENTE needs ambiente/NCMP000"
        );
        assert!(
            tagger_tags("mortos")
                .iter()
                .any(|r| r.starts_with("matar/")),
            "mortos carries the wrong participle lemma matar; MORRER_PERECER_FALECEN needs morrer/VMP00PM"
        );
        let acrescentar = tagger_tags("acrescentar");
        assert!(
            !acrescentar
                .iter()
                .any(|r| r.contains("VMN01S0") || r.contains("VMN03S0")),
            "acrescentar lacks the personal-infinitive readings SER_SIGNIFICATIVO's VMN0.S.+ element needs: {acrescentar:?}"
        );
    }
}
