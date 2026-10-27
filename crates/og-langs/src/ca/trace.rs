use crate::ca::example_suite::CatalanPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{
    CompiledPattern, CompiledPatternElement, CompiledRule, CompiledRuleSet, XmlCompiler,
};

pub(crate) fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = crate::ca::example_suite::ca_rules_dir()
            .expect("ca rules dir")
            .join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("ca grammar.xml");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile ca grammar.xml"),
        ))
    })
    .expect("ca")
}

pub(crate) fn suite_engine() -> PatternRuleEngine {
    PatternRuleEngine::new()
}

fn group_of<'a>(rules: &'a CompiledRuleSet, rule_id: &str) -> Vec<&'a CompiledRule> {
    rules.rules.iter().filter(|r| r.id == rule_id).collect()
}

pub(crate) fn probe(rule_id: &str, text: &str) {
    let pipeline = CatalanPipeline::shared();
    let engine = suite_engine();
    let rules = compiled_rules();
    eprintln!("=== probe [{rule_id}] '{text}' ===");
    for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
        eprintln!(
            "-- sentence [{sentence_start}..{}] '{:?}'",
            sentence_start + sentence.text().len(),
            sentence.text()
        );
        for t in sentence.non_whitespace_tokens() {
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                .collect();
            eprintln!(
                "   {}@{}..{} {:?} => {}",
                t.token().start() + sentence_start,
                t.token().start(),
                t.token().end(),
                t.token().token(),
                readings.join(", ")
            );
        }
        for rule in group_of(&rules, rule_id) {
            let matches = engine.match_rule(rule, &sentence);
            eprintln!(
                "   raw matches for {} (sub {:?}): {}",
                rule.id,
                rule.sub_id.as_deref(),
                matches.len()
            );
            for m in &matches {
                eprintln!(
                    "      off {} len {} msg {} replacements {:?}",
                    m.offset() + sentence_start,
                    m.length(),
                    m.message(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

pub(crate) fn probe_with_engine_trace(rule_id: &str, text: &str) {
    probe_with_engine_trace_sub(rule_id, None, text);
}

pub(crate) fn probe_with_engine_trace_sub(rule_id: &str, sub_id: Option<&str>, text: &str) {
    let pipeline = CatalanPipeline::shared();
    let engine = suite_engine();
    let rules = compiled_rules();
    eprintln!("=== engine trace [{rule_id}::{sub_id:?}] '{text}' ===");
    for rule in group_of(&rules, rule_id)
        .iter()
        .filter(|r| sub_id.is_none_or(|s| r.sub_id.as_deref() == Some(s)))
    {
        for (sentence, _) in pipeline.analyze_offset_text(text) {
            og_rules::pattern_trace::set_enabled(true);
            og_rules::pattern_trace::clear();
            let _ = engine.match_rule(rule, &sentence);
            for ev in og_rules::pattern_trace::take() {
                eprintln!(
                    "   [elem {} token {} {:?}] {}",
                    ev.pattern_idx, ev.token_idx, ev.token, ev.cause
                );
            }
            og_rules::pattern_trace::set_enabled(false);
        }
    }
}

pub(crate) fn pattern_of(rule_id: &str) {
    for rule in compiled_rules().rules.iter().filter(|r| r.id == rule_id) {
        eprintln!(
            "== rule {} (sub {:?}) pattern tokens: {} antipatterns: {}",
            rule.id,
            rule.sub_id.as_deref(),
            rule.pattern.elements.len(),
            rule.antipatterns.len()
        );
        for (i, el) in rule.pattern.tokens.iter().enumerate() {
            eprintln!(
                "   [{i}] text={:?} postag={:?} inflected={} regexp={} skip={} negate={} min={:?} max={:?} exceptions={}",
                el.text.as_deref(),
                el.postag.as_deref(),
                el.inflected,
                el.is_regexp,
                el.skip,
                el.negate,
                el.min,
                el.max,
                el.exceptions.len()
            );
        }
        for (a, ap) in rule.antipatterns.iter().enumerate() {
            for (i, el) in ap.tokens.iter().enumerate() {
                eprintln!(
                    "   anti[{a}][{i}] text={:?} re={:?} postag={:?} inflected={} cs={} skip={} min={:?} max={:?} exceptions={}",
                    el.text.as_deref(),
                    el.compiled_regexp.as_ref().map(|r| r.as_str()),
                    el.postag.as_deref(),
                    el.inflected,
                    el.case_sensitive,
                    el.skip,
                    el.min,
                    el.max,
                    el.exceptions.len()
                );
            }
        }
    }
}

fn token_needs_readings(t: &og_xml::compiler::CompiledPatternToken) -> bool {
    t.postag.is_some()
        || t.compiled_postag_regexp.is_some()
        || t.compiled_postag_regexp_fancy.is_some()
        || t.inflected
        || t.chunk.is_some()
        || t.chunk_re.is_some()
        || t.exceptions.iter().any(exception_needs_readings)
}

fn exception_needs_readings(e: &og_xml::compiler::CompiledException) -> bool {
    e.postag.is_some()
        || e.compiled_postag_regexp.is_some()
        || e.compiled_postag_regexp_fancy.is_some()
        || e.inflected
}

fn element_needs_readings(e: &CompiledPatternElement) -> bool {
    match e {
        CompiledPatternElement::Token(t) => token_needs_readings(t),
        CompiledPatternElement::OrGroup(g) => g.iter().any(token_needs_readings),
        CompiledPatternElement::AndGroup(g) => g.iter().any(token_needs_readings),
    }
}

pub(crate) fn pattern_needs_readings(pattern: &CompiledPattern) -> bool {
    pattern.tokens.iter().any(token_needs_readings)
        || pattern.elements.iter().any(element_needs_readings)
        || !pattern.unify_groups.is_empty()
}

pub(crate) fn rule_needs_readings(rule: &CompiledRule) -> bool {
    pattern_needs_readings(&rule.pattern) || rule.antipatterns.iter().any(pattern_needs_readings)
}

fn count_matches(rule_id: &str, text: &str) -> usize {
    count_matches_sub(rule_id, None, text)
}

fn count_matches_sub(rule_id: &str, sub_id: Option<&str>, text: &str) -> usize {
    let pipeline = CatalanPipeline::shared();
    let engine = suite_engine();
    let rules = compiled_rules();
    let mut matches = 0usize;
    for rule in group_of(&rules, rule_id)
        .iter()
        .filter(|r| sub_id.is_none_or(|s| r.sub_id.as_deref() == Some(s)))
    {
        for (sentence, _) in pipeline.analyze_offset_text(text) {
            matches += engine.match_rule(rule, &sentence).len();
        }
    }
    matches
}

fn count_matches_unsplit(rule_id: &str, sub_id: Option<&str>, text: &str) -> usize {
    let pipeline = CatalanPipeline::shared();
    let engine = suite_engine();
    let rules = compiled_rules();
    let sentence = pipeline.analyze_range(text, 0, text.len());
    let mut matches = 0usize;
    for rule in group_of(&rules, rule_id)
        .iter()
        .filter(|r| sub_id.is_none_or(|s| r.sub_id.as_deref() == Some(s)))
    {
        matches += engine.match_rule(rule, &sentence).len();
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn probe_round22_divendres_agost_pipeline_state() {
        let _release_guard = crate::release_guard();
        let pipeline = CatalanPipeline::shared();
        let engine = suite_engine();
        let rules = compiled_rules();
        for text in [
            "Divendres, 15 d'agost del 2013.",
            "Dilluns, 7 d'octubre de 2014",
            "Dimarts, 7 d'octubre de 2014",
        ] {
            eprintln!("=== {text:?}");
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                for tr in sentence.tokens() {
                    eprintln!(
                        "  tok {:?} {}..{}",
                        tr.token().token(),
                        tr.token().start(),
                        tr.token().end()
                    );
                }
            }
            for rule in group_of(&rules, "CA_DATE_WEEKDAY") {
                let mut total = 0usize;
                for (sentence, _) in pipeline.analyze_offset_text(text) {
                    total += engine.match_rule(rule, &sentence).len();
                }
                if total > 0 {
                    eprintln!("  sub {:?} kept {} matches", rule.sub_id, total);
                }
            }
        }
    }

    #[test]
    fn mes1_antipattern_immunizes_its_own_example() {
        assert_eq!(
            count_matches("MES1", "Destaca juliol com a mes sec."),
            0,
            "positive control: the shared engine's antipattern machinery handles MES1's \
             antipattern (month-name regexp with skip=3 plus exception scope=next \
             inflected=ser, then 'mes', then the weather-adjective regexp, grammar.xml:\
             72590-72596) - antipattern_immunize_spans over match_pattern_tokens honors \
             skip (pattern_rule.rs:1583-1616) and scope=next exceptions (:1610-1629), and \
             the &mesos_any; entity expands via XmlRuleParser::expand_entities_with_base \
             (og-xml parser.rs:1306) - if this pin flips the engine regressed"
        );
    }

    #[test]
    fn mes1_residue_pin_still_fires_the_plain_correct_example() {
        assert_eq!(
            count_matches("MES1", "El mes que ve."),
            1,
            "the MES1 fp family (76 fails, the largest in the surface bucket): Java \
             suppresses this match only through <filter class='org.languagetool.rules.\
             SuppressIfAnyRuleMatchesFilter' args='ruleIDs:MES2'>, which re-analyzes the \
             sentence with each suggestion applied and drops the match when MES2 fires at \
             an overlapping position (SuppressIfAnyRuleMatchesFilter.java:43-58) - on \
             'El m\u{e9}s que ve.' MES2's DET/m\u{e9}s/de/month sub-rules fire and veto. \
             Our engine's filter dispatch (og-rules pattern_rule.rs:2675 apply_filter) has \
             no arm for this class, and every MES2 sub-rule pattern is postag-based, so \
             the veto additionally needs the missing ca-ES.dict - this pin flips to 0 only \
             when BOTH the engine arm and the dictionary exist"
        );
    }

    #[test]
    fn inflected_surface_rules_stay_lemma_starved() {
        assert_eq!(
            count_matches("ENRECORDAR", "No t'enrecordes?"),
            0,
            "ENRECORDAR (21 fails): <token inflected='yes'>enrecordar</token> matches \
             lemma-or-surface (PatternToken.java:364-374 getTestToken); with the dict \
             'enrecordes' carries lemma 'enrecordar' and matches, on our added.txt-only \
             tagger the token has no readings and surface 'enrecordes' != 'enrecordar' - \
             the accepted-excluded dict family, not an engine gap (our fallback at \
             pattern_rule.rs:2245-2255 mirrors getTestToken)"
        );
        assert_eq!(
            count_matches("LHI_SEL", "L'hi denominava aix\u{ed}."),
            0,
            "LHI_SEL (10 fails): the third pattern token <token inflected='yes'>\
             denominar</token> needs the lemma reading of 'denominava' (PatternToken.\
             java:364-374); the surface tokens l'|li and hi|'l match fine, only the \
             inflected verb is dict-starved"
        );
    }

    #[test]
    fn mesos_minuscula_sub1_suppressed_by_a_dict_dependent_antipattern() {
        assert_eq!(
            count_matches_sub("MESOS_MINUSCULA", Some("1"), "El 3 de Juliol de 1985."),
            0,
            "MESOS_MINUSCULA sub 1 (9 fails, no_match side): the pattern token \
             (?-i)&mesos_any_maj; matches 'Juliol' (verified by probe_round20_inline_\
             case_flag_regex) and the match is dropped by the case-sensitive antipattern \
             at grammar.xml:85338-85343: \\\\p{{Lu}}.* + [min=0 max=4: \\\\p{{Lu}}.*|de|del|\
             d'|l'|\\\\d+] + month, which matches 'El' + ['3','de'] + 'Juliol' and \
             immunizes the span; in Java 'El' carries a D.* reading that the antipattern's \
             <exception postag='SPS00|D.*|C.*'> vetoes, so the antipattern cannot start - \
             dict-starved, the untagged 'El' cannot exercise the veto"
        );
        assert_eq!(
            count_matches_sub("MESOS_MINUSCULA", Some("1"), "el 3 de Juliol de 1985."),
            1,
            "lowercase control: 'el' fails the case-sensitive \\\\p{{Lu}}.* first token, so \
             the antipattern cannot fire and the same pattern matches - proving the \
             engine's antipattern, min/max and case-sensitivity handling are correct and \
             the suppression is purely the missing D.* reading"
        );
    }

    #[test]
    fn aleshores_residue_pin_still_fires_the_correct_example() {
        assert_eq!(
            count_matches("ALESHORES", "A les hores convingudes"),
            1,
            "the fp twin of the MESOS family: Java suppresses this correct example \
             through ALESHORES' antipattern hores + <token postag='A..[FC][PN].|V.P..PF.'\
             postag_regexp='yes'/>, which needs the adjective/participle reading of \
             'convingudes' (grammar.xml:119849-119853); dict-starved the antipattern \
             cannot fire and the surface pattern a+les+hores matches - flips to 0 when \
             ca-ES.dict lands"
        );
    }

    #[test]
    fn classifier_counts_inflected_tokens_as_dict_dependent() {
        let rules = compiled_rules();
        let accent_solt = rules
            .rules
            .iter()
            .find(|r| r.id == "ACCENT_SOLT")
            .expect("ACCENT_SOLT compiled");
        assert!(
            !rule_needs_readings(accent_solt),
            "ACCENT_SOLT (single [`\u{b4}] regexp token, grammar.xml ACCENT_SOLT) must stay surface-only"
        );
        let enrecordar = rules
            .rules
            .iter()
            .find(|r| r.id == "ENRECORDAR")
            .expect("ENRECORDAR compiled");
        assert!(
            enrecordar.pattern.tokens.iter().any(|t| t.inflected),
            "ENRECORDAR must carry inflected tokens"
        );
        assert!(rule_needs_readings(enrecordar));
        let mes1 = rules
            .rules
            .iter()
            .find(|r| r.id == "MES1")
            .expect("MES1 compiled");
        assert!(
            rule_needs_readings(mes1),
            "round-21 correction: MES1's own antipattern carries the exception scope=next \
         inflected=ser (grammar.xml:72590-72596), and antipatterns run through the same \
         reading-based matcher as patterns (PatternRule.java:223-226 immunizes via \
         DisambiguationPatternRuleReplacer.java:82-99, PatternRuleMatcher over readings), so \
         MES1 is dict-dependent after all - its 76-fp family was already attributed to the \
         SuppressIfAnyRuleMatchesFilter+MES2 chain, both dict-hungry"
        );
    }

    #[test]
    fn classifier_reads_antipattern_and_exception_postags() {
        let rules = compiled_rules();
        let palestina = rules
            .rules
            .iter()
            .find(|r| r.id == "PALESTINA")
            .expect("PALESTINA compiled");
        assert!(
            !palestina.pattern.tokens.iter().any(token_needs_readings),
            "PALESTINA's pattern is the plain case-sensitive token 'palestina'"
        );
        assert!(
            rule_needs_readings(palestina),
            "PALESTINA's fp family (9 fails) is suppressed in Java only by its three antipatterns \
         (N.[FC]S000|A..[FC]S.|_GN_FS + A..[FC]S., CC, inflected ser, grammar.xml PALESTINA), \
         all of which consult readings - dict-dependent, not engine-side"
        );
        let diacritics02 = rules
            .rules
            .iter()
            .find(|r| r.id == "DIACRITICS_02")
            .expect("DIACRITICS_02 compiled");
        assert!(
            diacritics02
                .pattern
                .tokens
                .iter()
                .any(|t| t.exceptions.iter().any(exception_needs_readings)),
            "DIACRITICS_02's marker token carries <exception postag='D.*|V.N.*|[NA].*|V.P.*' \
         postag_regexp='yes'/> (grammar.xml:31567), tested per reading by \
         AbstractPatternRulePerformer.java:232-234 isExceptionMatchedCompletely"
        );
        assert!(rule_needs_readings(diacritics02));
        let hagut = rules
            .rules
            .iter()
            .find(|r| r.id == "HAGUT")
            .expect("HAGUT compiled");
        assert!(
            rule_needs_readings(hagut),
            "HAGUT's fp family (7 fails) is suppressed by <exception scope='previous' \
         inflected='yes'>haver</exception> on the pattern token (grammar.xml HAGUT): Java \
         vetoes 'Ha hagut' because 'Ha' carries lemma haver - inflected exceptions need the \
         dictionary"
        );
    }

    #[ignore]
    #[test]
    fn probe_round21_engine_side_breakdown() {
        let _release_guard = crate::release_guard();
        let Ok(lines) = std::fs::read_to_string("/tmp/test_failures_ca_module.txt") else {
            eprintln!("no /tmp/test_failures_ca_module.txt - run the module suite first");
            return;
        };
        let mut fails: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for line in lines.lines() {
            if let Some(rest) = line.strip_prefix("FAIL: [")
                && let Some((id, _)) = rest.split_once(']')
            {
                *fails.entry(id.to_string()).or_default() += 1;
            }
        }
        let implemented_filters = [
            "org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter",
            "org.languagetool.rules.ApostropheTypeFilter",
        ];
        let dict_blocked_filters = ["org.languagetool.rules.ca.ConvertToGenderAndNumberFilter"];
        let tokenizer_dict_families = ["DEJA_VU", "PROVA_DEL_COTO"];
        let mut engine_side: Vec<(String, usize)> = Vec::new();
        let mut filter_side: Vec<(String, usize)> = Vec::new();
        let mut dict_side = 0usize;
        for (id, n) in &fails {
            let mut sub_rules = 0usize;
            let mut reading_subs = 0usize;
            let mut unimplemented_filter = false;
            for r in compiled_rules().rules.iter().filter(|r| &r.id == id) {
                sub_rules += 1;
                reading_subs += usize::from(rule_needs_readings(r));
                if let Some(f) = &r.filter
                    && !implemented_filters.contains(&f.class.as_str())
                {
                    unimplemented_filter = true;
                }
            }
            if sub_rules == 0
                || tokenizer_dict_families.contains(&id.as_str())
                || reading_subs > 0
                || (unimplemented_filter
                    && compiled_rules().rules.iter().any(|r| {
                        &r.id == id
                            && r.filter
                                .as_ref()
                                .is_some_and(|f| dict_blocked_filters.contains(&f.class.as_str()))
                    }))
            {
                dict_side += n;
            } else if unimplemented_filter {
                filter_side.push((id.clone(), *n));
            } else {
                engine_side.push((id.clone(), *n));
            }
        }
        engine_side.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        filter_side.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let total: usize = fails.values().sum();
        let engine_total: usize = engine_side.iter().map(|(_, n)| n).sum();
        let filter_total: usize = filter_side.iter().map(|(_, n)| n).sum();
        eprintln!(
            "round-21 attribution: {total} failures -> {dict_side} dict-dependent (postag OR \
             inflected on any pattern/antipattern token or exception, per PatternToken.java:\
             364-374, AbstractPatternRulePerformer.java:232-234 and PatternRule.java:223-226, \
             plus the round-22 additions: ConvertToGenderAndNumberFilter rules need the \
             ca-ES_synth.dict synthesis and tagger readings, and DEJA_VU/PROVA_DEL_COTO \
             hyphen words are kept whole only by the wordsToAdd dictionary lookup at \
             CatalanWordTokenizer.java:204-226), \
             {filter_total} on rules carrying a filter class with no engine arm, \
             {engine_total} on fully surface rules"
        );
        eprintln!(
            "filter-class residue by rule: {}",
            filter_side
                .iter()
                .take(20)
                .map(|(id, n)| format!("{id}({n})"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        eprintln!(
            "surface residue by rule: {}",
            engine_side
                .iter()
                .take(40)
                .map(|(id, n)| format!("{id}({n})"))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    #[test]
    fn round21_clitics_stay_whole_so_date_patterns_match() {
        assert_eq!(
            count_matches("CA_DATE_WEEKDAY", "Dilluns, 7 d'octubre de 2014"),
            1,
            "guards the round-21 tokenizer fix: Java's wordsToAdd keeps the closed clitic set \
             whole via the dictionary (CatalanWordTokenizer.java:225-235, pinned by \
             CatalanWordTokenizerTest [N', hi]/[d', 1]) and grammar.xml references d'/l'/s'/n'/\
             m'/t' as literal tokens 236/235/93/69/29/12 times, so those forms are dict-atomic; \
             our added.txt-only tagger used to split d' into d+' and the pattern token \
             <token regexp=\"yes\">de|d'</token> (grammar.xml CA_DATE_WEEKDAY) could never \
             match - if this pin flips back to 0 the clitic whitelist in word_tokenizer.rs \
             regressed"
        );
        assert_eq!(
            count_matches("CA_DATE_WEEKDAY", "Divendres, 15 d'agost del 2013."),
            1,
            "incorrect example (Java matches: the year is wrong) - matches now that d'agost \
             tokenizes like Java"
        );
        assert_eq!(
            count_matches("CA_DATE_WEEKDAY", "Dimarts, 7 d'octubre de 2014"),
            0,
            "correct date: 7 October 2014 was a dimarts, so Java's DateCheckFilter \
             (AbstractDateCheckWithSuggestionsFilter.java:186-189 dayOfWeekFromString == \
             dayOfWeekFromDate -> null) drops the raw pattern match; the round-22 wiring \
             adds DateLang::Ca (weekday equals-table and month startsWith chain from \
             DateFilterHelper.java:31-75, day names :47-57, wrong-year message \
             DateCheckFilter.java:52-54) plus the org.languagetool.rules.ca.DateCheckFilter \
             arm in apply_filter, and the filter's plain-number args now resolve against \
             per-element last-token positions like Java's getSkipCorrectedReference \
             (RuleFilter.java:121-133 over one span entry per pattern element, \
             AbstractPatternRulePerformer.java:124/141) - the old per-token indexing made \
             year:7 land on 'l' whenever the min=0 max=8 element consumed two tokens \
             ('de'+'l' in 'del 2013'), dropping Java's wrong-year match on 'Divendres, 15 \
             d'agost del 2013.'; if this pin flips back to >0 the .ca. arm or the element \
             resolution regressed"
        );
    }

    #[test]
    fn round21_cometes_residue_is_the_shared_apostrophe_normalization() {
        assert_eq!(
            count_matches("COMETES_INCORRECTES", "El pou d'Avall."),
            0,
            "guards the round-21 tokenizer fix: with d' whole, the pattern exception \
             <exception regexp=\"yes\">[lnmtsd]'|[\\[\\(\u{ab}\u{201c}]</exception> \
             (grammar.xml COMETES_INCORRECTES) vetoes the match exactly like Java - the \
             pre-fix fp was the split ' token that no exception could veto"
        );
        assert_eq!(
            count_matches(
                "COMETES_INCORRECTES",
                "El pou d'\u{2018}aquella casa\u{2019}."
            ),
            2,
            "residue: sub-rules 1 and 2 match 'casa' + '\u{2019}' because the shared engine \
             normalizes apostrophes on BOTH sides of the plain-token compare \
             (og-rules pattern_rule.rs:1123-1124 over og-xml unifier.rs:8-14, which folds \
             U+2019/U+2018/U+201B to '), so the pattern token <token spacebefore=\"no\">\
             \u{2018}</token> accepts the closing quote \u{2019} - Java has no such fold \
             (PatternTokenMatcher compares with equals/equalsIgnoreCase) - flips to 0 when \
             the shared owner stops equating the two quotes in token matching"
        );
        assert_eq!(
            count_matches(
                "COMETES_INCORRECTES",
                "\u{2018}lletra de l'alfabet grec\u{2019}"
            ),
            1,
            "same engine-side fold: sub-rule 2 matches 'grec' + '\u{2019}' against the \
             pattern's \u{2018} token"
        );
    }

    #[test]
    fn round21_antipattern_and_exception_fps_stay_dict_starved() {
        assert_eq!(
            count_matches("HAGUT", "Ha hagut de fer-ho."),
            1,
            "HAGUT (7 fails): Java vetoes via <exception scope='previous' inflected='yes'>\
             haver</exception> on the pattern token (grammar.xml HAGUT) because 'Ha' carries \
             lemma haver - AbstractPatternRulePerformer.java:232-234 tests exceptions per \
             reading; dict-starved 'Ha' has no readings so the exception cannot veto - flips \
             to 0 with ca-ES.dict"
        );
        assert_eq!(
            count_matches("PALESTINA", "Una dona palestina."),
            1,
            "PALESTINA (9 fails): Java suppresses via the antipattern N.[FC]S000|A..[FC]S.|\
             _GN_FS + palestina ('dona' is NCFS000) or the inflected=ser antipattern \
             (grammar.xml PALESTINA); antipatterns run through the same reading-based matcher \
             (PatternRule.java:223-226, DisambiguationPatternRuleReplacer.java:82-99) - \
             dict-starved they cannot fire, flips to 0 with ca-ES.dict"
        );
        assert_eq!(
            count_matches("DISPAR", "eren situacions dispars."),
            1,
            "DISPAR (10 fails): the antipattern NC.*|RG.*|.*LOC_ADV.*|V.[MSI].*|_GV_|VS.* + \
             dispars? needs 'situacions' to carry its NC* reading (grammar.xml DISPAR) - \
             dict-starved, flips to 0 with ca-ES.dict"
        );
        assert_eq!(
            count_matches(
                "PEL_QUE",
                "\u{c9}s aquesta la q\u{fc}esti\u{f3}, pel que sembla."
            ),
            1,
            "PEL_QUE (16 fails): sub-rule 2's antipattern pe+l+que+no+V.[IS].*+_PUNCT.*|CC \
             vetoes when 'sembla' carries VMIP3S0 (grammar.xml PEL_QUE) - dict-starved, \
             flips to 0 with ca-ES.dict"
        );
        assert_eq!(
            count_matches(
                "DIFERENCIA",
                "el que realment la diferencia de totes les altres"
            ),
            1,
            "DIFERENCIA verb-side fp: the group's que + min0max2 RG|LOC_ADV + la + \
             diferencia antipatterns (grammar.xml DIFERENCIA) need adverb readings - \
             dict-starved, flips to 0 with ca-ES.dict"
        );
    }

    #[test]
    fn round24_d_sr_srx_split_is_java_parity_and_the_example_passes_unsplit() {
        assert_eq!(
            count_matches("D_SR", "D\u{ba}. Jaume Ivars."),
            0,
            "strict (sentence-split) path, and the split after 'D\u{ba}. ' is what Java's \
             loomchild cascade produces too: SrxTextIterator.java:358-391 takes the earliest \
             break rule by position (ties by document order) and consults only the no-break \
             rules preceding it in the ruleset (RuleManager.java:48-83 builds each break \
             rule's exception alternation from the no-break rules seen so far), and no \
             Catalan no-break rule covers 'D\u{ba}. ' before an uppercase word - the acronym \
             rule at segment.srx:4918 gates its afterbreak on lowercase-only (Ll), the initials rule at \
             :4871 needs an uppercase letter immediately before the dot, and \u{ba} is Lo \
             not Lu for the rule at :4927. Round 21 pinned this as an og-tokenizer bug on \
             the false premise that Java keeps the text whole; that premise is refuted"
        );
        assert_eq!(
            count_matches_unsplit("D_SR", None, "D\u{ba}. Jaume Ivars."),
            1,
            "Java parity of the upstream harness: PatternRuleTest.java:617-620 routes ca \
             incorrect examples through getMatchesForSingleSentence (:866-868, \
             lt.getAnalyzedSentence over the whole text with no SRX splitting), so upstream's \
             CatalanPatternRuleTest passes the grammar.xml:139737 example without the \
             splitter ever running on it; the suite now mirrors that branch (example_suite \
             unsplit_incorrect) and the D\u{ba} + . + Lu-initial pattern matches the \
             unsplit token stream"
        );
    }

    #[test]
    fn round24_etcetera_srx_split_is_java_parity_and_the_example_passes_unsplit() {
        assert_eq!(
            count_matches("ETCETERA", "Tres, quatre, cinc, ... etc."),
            0,
            "strict path: the only candidate ellipsis no-break rule \
             [non-space](ellipsis)[space] at segment.srx:4958 requires a NON-whitespace \
             character immediately before '...' (verified by cat -A on the vendored file), \
             but 'cinc, ...' carries a space between the comma and the dots, so the \
             exception cannot fire in the Java cascade either - both engines split before \
             'etc.'. The round-21 citation of CatalanSentenceTokenizerTest:90 was a \
             different family (ellipsis directly after 'tot' with a closing quote, matched \
             by the rule at :4963)"
        );
        assert_eq!(
            count_matches_unsplit("ETCETERA", Some("3"), "Tres, quatre, cinc, ... etc."),
            1,
            "unsplit, sub-rule 3 (, . . . ,? etc .) alone matches exactly like Java's \
             per-sub-rule getMatchesForSingleSentence evaluation of grammar.xml:85936 \
             (getPatternRulesByIdAndSubId runs one sub-rule per example): the tokenizer \
             splits '...' into three '.' tokens (word_tokenizer.rs test_ellipsis_standalone)"
        );
    }

    #[test]
    fn round21_cometes_rectes_residue_is_the_regexp_normalization_asymmetry() {
        assert_eq!(
            count_matches_unsplit("COMETES_RECTES", None, "És de ‘gran qualitat\"."),
            1,
            "COMETES_RECTES sub-rule 2's <token regexp=\"yes\">[‘’]</token> now matches: \
             element_surface_text_matches tries the raw surface first and the apostrophe-\
             normalized form second, so regexes written against typographic quotes see the \
             raw token like Java's StringMatcher (no fold anywhere in the matcher path) \
             while straight-apostrophe regexes keep matching typographic tokens. The \
             grammar.xml:83049 example therefore passes; this was the last engine-side \
             surface-normalization family in the ca residue"
        );
    }

    #[test]
    fn round24_et_al_srx_split_is_java_parity_and_the_example_passes_unsplit() {
        assert_eq!(
            count_matches("ET_AL", "F. Garcia et. al., J. Pont."),
            0,
            "strict path, Java-parity split after 'et. ': the acronym rule at \
             segment.srx:4918 (letter-dot repetitions behind a word boundary) cannot match \
             'et.' because the boundary fails before the 't' and the group needs \
             letter+dot pairs, and the composed-abbreviation rule at :4950 is the literal \
             space form 'et al.'. Round 22's claim that Java's tokenizer keeps it whole is \
             refuted - upstream passes the grammar.xml:87543 example because \
             PatternRuleTest.java:617-620 never splits ca incorrect examples"
        );
        assert_eq!(
            count_matches_unsplit("ET_AL", Some("2"), "F. Garcia et. al., J. Pont."),
            1,
            "unsplit, sub-rule 2 (et + . + a + l + .) alone matches with the contraction \
             split (CatalanWordTokenizer.java:166-176) intact, mirroring upstream's green \
             per-sub-rule example test; sub-rule 1 (et + [min=0 .] + a + l) also matches \
             this text, so the whole-group count is 2 - each sub-rule's own example is \
             evaluated against itself exactly like Java's getPatternRulesByIdAndSubId"
        );
        assert_eq!(
            count_matches("ET_AL", "F. Garcia et al, J. Pont."),
            1,
            "positive control: sub-rule 1 (et + [min=0 .] + a + l) fires once the sentence \
             stays whole, proving the rule compile, the contraction split and the \
             suggestions are all correct"
        );
    }

    #[test]
    fn round22_hyphenated_dict_words_stay_split_without_ca_dict() {
        assert_eq!(
            count_matches("DEJA_VU", "Un déjà-vu."),
            1,
            "DEJA_VU (1 fail, re-attributed this round): the fp is the tokenizer's hyphen \
             split, NOT <match> interpolation - grammar.xml:64055-64067 has no <match> \
             element. CatalanWordTokenizer.java:204-226 wordsToAdd keeps 'déjà-vu' whole \
             only when CatalanTagger.INSTANCE_CAT.tag(...).isTagged() succeeds, a ca-ES.\
             dict lookup; our added.txt-only tagger does not tag it, so the StringTokenizer \
             fallback at :224-228 splits it into déjà + - + vu and the 3-token pattern \
             d[eèé]j[aàá] + [- min=0] + v[uú] matches - flips to 0 only when the \
             dictionary-driven keep-whole lookup exists"
        );
        assert_eq!(
            count_matches("PROVA_DEL_COTO", "Serà la prova del cotó-en-pèl."),
            1,
            "PROVA_DEL_COTO (1 fail): same wordsToAdd dictionary lookup - Java keeps \
             'cotó-en-pèl' whole so the <token regexp=\"yes\">cotó|cotó-en-pèl</token> \
             alternative matches the single token and the match spans the full marker \
             (the suite artifact shows our match 9..24 'prova del cotó' vs marker 9..32 \
             'prova del cotó-en-pèl') - dict-dependent via the tokenizer, not an engine gap"
        );
        assert_eq!(
            count_matches("PROVA_DEL_COTO", "Serà la prova del cotó fluix."),
            2,
            "positive control: sub-rule 1 (prova de l cotó fluix) fires with the span that \
             covers the marker, and sub-rule 2 ALSO fires because its \
             cot\u{f3}|cot\u{f3}-en-p\u{e8}l regexp matches the bare 'cot\u{f3}' token - \
             Java matches both too, and its example passes because sub-rule 1's span \
             contains the marker"
        );
    }

    #[test]
    fn round22_gender_number_filter_families_need_the_synth_dict() {
        assert_eq!(count_matches("COSE", "Tinc una cose per a tu."), 1);
        assert_eq!(
            count_matches("RODALIES", "Per la ciutat i les seves rodalies."),
            1
        );
        assert_eq!(
            count_matches("RECOLZAMENT", "Sense recolzament popular."),
            1
        );
        assert_eq!(
            count_matches("TINGLADO", "Era un tinglado."),
            1,
            "the 13 span-shape failures (AVANTATJA_AVANTATGE 3, RECOLZAMENT 3, COLLE 2, \
             TINGLADO 2, COSE/LIJA/RODALIES 1 each) all carry <filter class='org.\
             languagetool.rules.ca.ConvertToGenderAndNumberFilter'>: Java rebuilds the \
             match over the determiner it re-inflects (ConvertToGenderAndNumberFilter.\
             java:197-201 sets startPos=i on the DA reading, :330-331 emits the RuleMatch \
             over tokens[startPos]..tokens[endPos]) and synthesizes every suggested word \
             via CatalanSynthesizer (synthesizeWithGenderAndNumber :372-397 over \
             BaseSynthesizer '/ca/ca.sor' '/ca/ca-ES_synth.dict' '/ca/ca-ES_tags.txt', \
             CatalanSynthesizer.java:59-60). ca-ES_synth.dict and ca-ES_tags.txt are \
             absent from the Java tree too (find: only ca.sor exists), and the tagger \
             readings the filter consults (:66 lemmaSelect, :79 suggestion tagging, \
             :124-131 neighbor agreement) need ca-ES.dict - accepted-excluded on data \
             missing from BOTH trees, like MorfologikCatalanSpellerRule. A dict-less \
             engine arm cannot reproduce the span either: the :81-85 fallback keeps the \
             original span and empty synthesis sets ignoreThisSuggestion (:172-175), \
             dropping the match instead of expanding it"
        );
    }

    #[test]
    fn round21_chunk_rules_stay_dict_starved() {
        assert_eq!(
            count_matches("EN_FA_QUATRE_ANYS", "D'aix\u{f2} en fa quatre anys."),
            0,
            "EN_FA_QUATRE_ANYS (3 fails): the pattern needs <token chunk=\"PTime\"/>, and \
             every PTime producer is an action=\"addchunk\" rule in disambiguation.xml whose \
             pattern is postag-based (PERIODES_DE_TEMPS at disambiguation.xml:1512-1530 uses \
             DA.*|AO.*|NC.*) - dict-starved twice over: no readings means no chunk, and the \
             chunk classifier (token_needs_readings counts chunk/chunk_re) keeps this family \
             in the dict bucket"
        );
    }

    #[ignore]
    #[test]
    fn probe_round21_family_counts() {
        let _release_guard = crate::release_guard();
        for (id, text) in [
            ("CA_DATE_WEEKDAY", "Dilluns, 7 d'octubre de 2014"),
            ("CA_DATE_WEEKDAY", "Dimarts, 7 d'octubre de 2014"),
            ("CA_DATE_WEEKDAY", "Dimarts, 7 oct 2014"),
            ("CA_DATE_WEEKDAY", "Divendres, 15 d'agost del 2013."),
            ("COMETES_INCORRECTES", "El pou d'Avall."),
            (
                "COMETES_INCORRECTES",
                "El pou d'\u{2018}aquella casa\u{2019}.",
            ),
            (
                "COMETES_INCORRECTES",
                "\u{2018}lletra de l'alfabet grec\u{2019}",
            ),
            ("HAGUT", "Ha hagut de fer-ho."),
            ("PALESTINA", "Una dona palestina."),
            ("DISPAR", "eren situacions dispars."),
            (
                "PEL_QUE",
                "\u{c9}s aquesta la q\u{fc}esti\u{f3}, pel que sembla.",
            ),
            (
                "DIFERENCIA",
                "Aquest fet indica que la diferencia en l'estructura ha tendit a reduir-se.",
            ),
            (
                "DIFERENCIA",
                "el que realment la diferencia de totes les altres",
            ),
            ("ENRECORDAR", "No t'enrecordes?"),
            ("LHI_SEL", "L'hi denominava aix\u{ed}."),
            ("EN_FA_QUATRE_ANYS", "D'aix\u{f2} en fa quatre anys."),
            ("COMETES_RECTES", "\u{c9}s de \u{2018}gran qualitat\"."),
            ("ETCETERA", "Tres, quatre, cinc, ... etc."),
            ("ET_AL", "F. Garcia et. al., J. Pont."),
            ("ET_AL", "F. Garcia et al, J. Pont."),
            ("DEJA_VU", "Un d\u{e9}j\u{e0}-vu."),
            (
                "PROVA_DEL_COTO",
                "Ser\u{e0} la prova del cot\u{f3}-en-p\u{e8}l.",
            ),
            ("PROVA_DEL_COTO", "Ser\u{e0} la prova del cot\u{f3} fluix."),
            ("COSE", "Tinc una cose per a tu."),
            ("RODALIES", "Per la ciutat i les seves rodalies."),
            ("RECOLZAMENT", "Sense recolzament popular."),
            ("D_SR", "D\u{ba}. Jaume Ivars."),
        ] {
            eprintln!(
                "{id} on '{text}': strict={} unsplit={}",
                count_matches(id, text),
                count_matches_unsplit(id, None, text)
            );
        }
        probe(
            "COMETES_INCORRECTES",
            "El pou d'\u{2018}aquella casa\u{2019}.",
        );
        probe(
            "COMETES_INCORRECTES",
            "\u{2018}lletra de l'alfabet grec\u{2019}",
        );
        probe("D_SR", "D\u{ba}. Jaume Ivars.");
        probe("ETCETERA", "Tres, quatre, cinc, ... etc.");
        probe("ET_AL", "F. Garcia et. al., J. Pont.");
    }

    #[ignore]
    #[test]
    fn probe_round20_mes1_antipattern_trace() {
        let _release_guard = crate::release_guard();
        pattern_of("MES1");
        probe("MES1", "Destaca juliol com a mes sec.");
        probe_with_engine_trace("MES1", "Destaca juliol com a mes sec.");
        probe("MES1", "El mes que ve.");
        probe(
            "MES1",
            "Per mes detalls, vegeu la nostra pol\u{ed}tica de privacitat.",
        );
    }

    #[ignore]
    #[test]
    fn probe_round20_enrecordar_readings() {
        let _release_guard = crate::release_guard();
        pattern_of("ENRECORDAR");
        pattern_of("LHI_SEL");
        probe("ENRECORDAR", "No t'enrecordes?");
        probe("LHI_SEL", "L'hi denominava aix\u{ed}.");
    }

    #[ignore]
    #[test]
    fn probe_round20_inline_case_flag_regex() {
        let _release_guard = crate::release_guard();
        let rules = compiled_rules();
        let month_re = "^(?:(?-i)Gener|Febrer|Mar\u{e7}|Abril|Maig|Juny|Juliol|Agost|Setembre|Octubre|Novembre|Desembre)$";
        match regex::RegexBuilder::new(month_re)
            .case_insensitive(true)
            .build()
        {
            Ok(re) => eprintln!(
                "plain ci build: OK is_match(Juliol)={} is_match(juliol)={} is_match(GENER)={}",
                re.is_match("Juliol"),
                re.is_match("juliol"),
                re.is_match("GENER")
            ),
            Err(e) => eprintln!("plain ci build: FAIL {e}"),
        }
        match og_xml::java_case_fold::compile_java_ci(month_re) {
            Some(re) => eprintln!(
                "java_case_fold: OK is_match(Juliol)={} is_match(juliol)={}",
                re.is_match("Juliol"),
                re.is_match("juliol")
            ),
            None => eprintln!("java_case_fold: None"),
        }
        let compiled = rules
            .rules
            .iter()
            .find(|r| r.id == "MESOS_MINUSCULA" && r.sub_id.as_deref() == Some("1"))
            .unwrap();
        let tok = &compiled.pattern.tokens[0];
        eprintln!(
            "MESOS_MINUSCULA.1 token0: cs_re={} ci_re={} fancy={}",
            tok.compiled_regexp.is_some(),
            tok.compiled_regexp_ci.is_some(),
            tok.compiled_regexp_fancy.is_some()
        );
        if let Some(re) = &tok.compiled_regexp_ci {
            eprintln!(
                "ci is_match(Juliol)={} is_match(juliol)={}",
                re.is_match("Juliol"),
                re.is_match("juliol")
            );
        }
    }

    #[ignore]
    #[test]
    fn probe_round20_mesos_minuscula_suppression_bisect() {
        let _release_guard = crate::release_guard();
        let rules = compiled_rules();
        let pipeline = CatalanPipeline::shared();
        let engine = suite_engine();
        for text in [
            "El 3 de Juliol de 1985.",
            "el 3 de Juliol de 1985.",
            "Vaig al Juliol.",
            "3 de Juliol",
            "X de Juliol",
            "X 3 de Juliol",
            "el tres de Juliol",
        ] {
            let per_sub: Vec<String> = group_of(&rules, "MESOS_MINUSCULA")
                .iter()
                .map(|rule| {
                    let mut n = 0usize;
                    for (sentence, _) in pipeline.analyze_offset_text(text) {
                        n += engine.match_rule(rule, &sentence).len();
                    }
                    format!("{}={}", rule.sub_id.clone().unwrap_or_default(), n)
                })
                .collect();
            eprintln!("MESOS_MINUSCULA on '{text}': {}", per_sub.join(" "));
        }
        probe_with_engine_trace_sub("MESOS_MINUSCULA", Some("1"), "El 3 de Juliol de 1985.");
    }

    #[ignore]
    #[test]
    fn probe_round25_example_coverage_gap() {
        let _release_guard = crate::release_guard();
        let rule_set = compiled_rules();
        let compiled_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
        eprintln!(
            "compiled rules: {} compiled examples: {} (grammar.xml carries 27098)",
            rule_set.rules.len(),
            compiled_examples
        );
        let mut group_index: std::collections::HashMap<(String, Option<String>), usize> =
            std::collections::HashMap::new();
        let mut groups: Vec<(String, Option<String>, usize, usize)> = Vec::new();
        for rule in &rule_set.rules {
            let key = (rule.id.clone(), rule.sub_id.clone());
            let next = groups.len();
            let idx = *group_index.entry(key).or_insert(next);
            if idx == next {
                groups.push((rule.id.clone(), rule.sub_id.clone(), rule.examples.len(), 1));
            } else {
                groups[idx].2 += rule.examples.len();
                groups[idx].3 += 1;
            }
        }
        let evaluated: usize = groups.iter().map(|g| g.2).sum();
        eprintln!(
            "suite groups: {} evaluated examples (group-first only): {}",
            groups.len(),
            evaluated
        );
        let mut dupes: Vec<&(String, Option<String>, usize, usize)> =
            groups.iter().filter(|g| g.3 > 1).collect();
        dupes.sort_unstable_by_key(|g| std::cmp::Reverse(g.2));
        eprintln!(
            "duplicate (id, sub_id) keys losing group.tail examples: {}",
            dupes
                .iter()
                .take(25)
                .map(|g| format!("{}[{:?}]x{}", g.0, g.1, g.3))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let mut no_sub: Vec<&CompiledRule> = rule_set
            .rules
            .iter()
            .filter(|r| r.sub_id.is_none() && r.examples.is_empty())
            .collect();
        no_sub.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        eprintln!(
            "compiled rules with zero examples: {} of {} rules; ids: {}",
            rule_set
                .rules
                .iter()
                .filter(|r| r.examples.is_empty())
                .count(),
            rule_set.rules.len(),
            rule_set
                .rules
                .iter()
                .filter(|r| r.examples.is_empty())
                .take(25)
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let _ = &mut no_sub;
    }

    #[ignore]
    #[test]
    fn probe_round25_sub_rule_inventory() {
        let _release_guard = crate::release_guard();
        let rules = compiled_rules();
        for id in [
            "DARRERA",
            "QUANT_QUAN",
            "SON",
            "ESCOLTAR_SENTIR",
            "QUE_INICIAL_AMBACCENT_VERB",
            "ACCENTUATION_CHECK",
        ] {
            for rule in group_of(&rules, id).iter().take(3) {
                let first: Vec<String> = rule
                    .pattern
                    .tokens
                    .iter()
                    .take(3)
                    .map(|t| {
                        format!(
                            "{:?}/postag={:?}/inf={}",
                            t.text.as_deref(),
                            t.postag.as_deref(),
                            t.inflected
                        )
                    })
                    .collect();
                let exs: Vec<&str> = rule
                    .examples
                    .iter()
                    .map(|e| e.text.as_str())
                    .take(2)
                    .collect();
                eprintln!(
                    "{id} sub {:?} tokens [{}] examples {:?}",
                    rule.sub_id,
                    first.join(", "),
                    exs
                );
            }
        }
    }

    #[test]
    fn round25_top_residue_families_pin_their_java_matching_sub_rules() {
        assert_eq!(
            count_matches_sub("ESCOLTAR_SENTIR", Some("1"), "Escoltava dir coses."),
            0,
            "ESCOLTAR_SENTIR (124 fails, the largest residue family) pins the exact Java \
             (id, subId) scope of PatternRuleTest.java:579 getPatternRulesByIdAndSubId: the \
             example belongs to sub-rule 1 (grammar.xml:121717-121726), <token inflected=\"yes\"\
             >escoltar</token> + <token regexp=\"yes\">dir|parlar|comentar|expressar|manifestar\
             </token> with filter AdjustVerbSuggestionsFilter - 'Escoltava' must carry lemma \
             escoltar, which only ca-ES.dict provides (CatalanTagger.java:64 loads it; absent \
             from this tree); the group's surface sub-rules cover different phrases so they \
             cannot satisfy this example even in the same-id scope - dict-starved, flips to 1 \
             when a ca lexicon lands"
        );
        assert_eq!(
            count_matches_sub("QUE_INICIAL_AMBACCENT_VERB", Some("1"), "Que ha passat?"),
            0,
            "QUE_INICIAL_AMBACCENT_VERB (96 fails): sub-rule 1 is grammar.xml:70158-70167 - \
             SENT_START + \\p{{P}}* + marker 'que' + <token min=\"0\" inflected=\"yes\">haver\
             </token> + <token skip=\"-1\" postag=\"V.[ISMNGP].*|_GV_\" inflected=\"yes\" \
             regexp=\"yes\">passar|voler|necessitar|de</token> + '?' - 'ha' and 'passat' need \
             their VAIP*/VMP* readings from ca-ES.dict; the surface tokens ('que', '?') alone \
             cannot complete the pattern - dict-starved"
        );
        assert_eq!(
            count_matches_sub("ACCENTUATION_CHECK", Some("1"), "Amb industria."),
            0,
            "ACCENTUATION_CHECK (86 fails): sub-rule 1 is grammar.xml:31674-31690 - <token \
             postag=\"SPS00|LOC_PREP\" postag_regexp=\"yes\"> (exception postag RG|_possible_\
             nompropi) + marker <token> with exception postag D.*|V.N.*|[NA].*|V.P.*|_possible_\
             nompropi + DiacriticsCheckFilter(form:\\2 postag:[AN].*) - 'Amb' must carry SPS00 \
             and the filter checks 'industria' against the [AN].* postag set, both from \
             ca-ES.dict - dict-starved"
        );
        assert_eq!(
            count_matches_sub(
                "DARRERA",
                Some("1"),
                "Crisi darrera crisi, no se'n surten mai."
            ),
            0,
            "DARRERA no_match half (20 fails): sub-rule 1 is grammar.xml:54734-54743, <token \
             postag=\"[NA].*|V.P.*\" postag_regexp=\"yes\"/> + marker 'darrera' + <token><match \
             no=\"0\"/></token> - 'Crisi' must carry an NC* reading (grammar.xml:54743 is this \
             exact example) - dict-starved; the same-id scope does not save it because no \
             sibling sub-rule's surface pattern covers 'Crisi darrera crisi'"
        );
        assert_eq!(
            count_matches_sub("QUANT_QUAN", Some("1"), "Quant tens?"),
            1,
            "QUANT_QUAN fp (13 fails): sub-rule 1's pattern (grammar.xml:20067-20072) is the \
             single marker token 'quant' (fully surface), so our engine fires on 'Quant' - Java \
             suppresses via the sub-rule antipattern at grammar.xml:20041-20045, <token>quant\
             </token> + <token skip=\"-1\" inflected=\"yes\" regexp=\"yes\">tenir|fer</token> \
             + '?', because 'tens' carries the tenir lemma from ca-ES.dict - our added.txt-only \
             tagger gives 'tens' no readings so the inflected antipattern cannot veto; flips \
             to 0 with a ca lexicon"
        );
        assert_eq!(
            count_matches_sub("SON", Some("1"), "son pare no té nas"),
            1,
            "SON fp (15 fails): sub-rule 1's pattern (grammar.xml:71091-71095) is the bare \
             marker token 'son' with <exception postag=\"_GN_.S|_possible_nompropi|NP.*\"> - \
             Java vetoes the match because possessive 'son' carries a _GN_.S reading from \
             ca-ES.dict, and the sub-rule antipattern <token postag=\"SPS00|.*LOC_PREP.*\"> + \
             son (grammar.xml:71087-71090) also needs readings; dict-starved 'son' is \
             reading-empty so the exception cannot veto and we fire - flips to 0 with a ca \
             lexicon"
        );
    }
}
