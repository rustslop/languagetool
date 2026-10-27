use crate::pl::test_support::PolishPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};

pub(crate) fn trace_engine() -> PatternRuleEngine {
    crate::pl::example_suite::suite_engine()
}

pub(crate) fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = crate::pl::pl_rules_dir()
            .expect("pl rules dir")
            .join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("grammar.xml");
        let xml = crate::pl::normalize_pl_rules_xml(&xml);
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(xml.as_ref(), path.parent())
                .expect("compile pl grammar.xml"),
        ))
    })
    .expect("pl grammar")
}

fn group_of<'a>(rules: &'a CompiledRuleSet, rule_id: &str) -> Vec<&'a CompiledRule> {
    rules.rules.iter().filter(|r| r.id == rule_id).collect()
}

pub(crate) fn probe_with_engine_trace(rule_id: &str, text: &str) {
    let pipeline = PolishPipeline::shared();
    let engine = trace_engine();
    let rules = compiled_rules();
    eprintln!("=== engine trace [{rule_id}] '{text}' ===");
    for rule in group_of(&rules, rule_id) {
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
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
            let _ = sentence_start;
        }
    }
}

pub(crate) fn probe(rule_id: &str, text: &str) {
    let pipeline = PolishPipeline::shared();
    let engine = trace_engine();
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
                    "      off {} len {} msg {}",
                    m.offset() + sentence_start,
                    m.length(),
                    m.message()
                );
            }
        }
    }
}

pub(crate) fn pattern_of(rule_id: &str) {
    for rule in compiled_rules().rules.iter().filter(|r| r.id == rule_id) {
        eprintln!(
            "== rule {} (sub {:?}) pattern tokens: {}",
            rule.id,
            rule.sub_id.as_deref(),
            rule.pattern.elements.len()
        );
        for (i, el) in rule.pattern.tokens.iter().enumerate() {
            eprintln!(
                "   [{i}] text={:?} postag={:?} regexp={} skip={} negate={} min={:?} max={:?} space={:?}",
                el.text.as_deref(),
                el.postag.as_deref(),
                el.is_regexp,
                el.skip,
                el.negate,
                el.min,
                el.max,
                el.space_before.as_deref()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn probe_nested_quotes_both_examples() {
        let _release_guard = crate::release_guard();
        probe(
            "ZAGNIEZDZONE_CUDZYSLOWY",
            "To jest jak wiadomo \u{201e}cytat \u{201e}w cytacie\u{201d}, kt\u{f3}ry jest d\u{142}ugi\u{201d}.",
        );
        probe(
            "ZAGNIEZDZONE_CUDZYSLOWY",
            "\u{201e}Ka\u{17c}dy noemat ma \u{2018}tre\u{15b}\u{107}\u{2019}\u{201d}.",
        );
    }

    #[ignore]
    #[test]
    fn probe_interp_entity_compilation() {
        let _release_guard = crate::release_guard();
        let interp = "[\\p{Punct}\\-\\u{2013}\\u{2212}\\u{2014}\\u{2015}\\u{201e}\\u{00ab}\\u{00bb}\\u{2026}\\u{2019}&&[^\\)]]+";
        eprintln!("interp entity: {interp}");
        match regex::Regex::new(interp) {
            Ok(re) => eprintln!(
                "regex crate: OK; is_match(.)={} is_match(\u{201e})={} is_match(\u{2026})={} is_match(w)={}",
                re.is_match("."),
                re.is_match("\u{201e}"),
                re.is_match("\u{2026}"),
                re.is_match("w")
            ),
            Err(e) => eprintln!("regex crate: FAIL {e}"),
        }
        for rule in compiled_rules()
            .rules
            .iter()
            .filter(|r| r.id == "ZDANIA_ZLOZONE")
            .take(2)
        {
            for (i, tok) in rule.pattern.tokens.iter().enumerate() {
                eprintln!(
                    "ZDANIA_ZLOZE elem {i}: text={:?} postag={:?} re={:?} fancy={:?}",
                    tok.text.as_deref(),
                    tok.postag.as_deref(),
                    tok.compiled_regexp.as_ref().map(|r| r.as_str()),
                    tok.compiled_regexp_fancy.as_ref().map(|r| r.as_str()),
                );
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_interp_comma_rule_firing() {
        let _release_guard = crate::release_guard();
        use og_core::checker::Disambiguator as _;
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let text = "To jest co\u{15b}, co wiedzia\u{142}em od dawna.";
        let mut xml_stage = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        if let Some(xml) = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
            .ok()
            .and_then(|x| {
                let normalized = crate::pl::normalize_disambiguation_xml(&x);
                og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None).ok()
            })
        {
            xml.disambiguate(&mut xml_stage);
        }
        for t in xml_stage.non_whitespace_tokens() {
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                .collect();
            eprintln!("   {:?} => {}", t.token().token(), readings.join(", "));
        }
    }

    #[ignore]
    #[test]
    fn probe_punct_rule_parsing() {
        let _release_guard = crate::release_guard();
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .unwrap();
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&xml, None)
            .expect("parse disambiguation.xml");
        eprintln!("disambiguator rule count: {}", parsed.rule_count());
        if let Some(idx) = parsed
            .rule_ids()
            .iter()
            .position(|id| *id == "PUNCT_NO_DOT")
        {
            eprintln!("PUNCT_NO_DOT idx {idx}");
            eprintln!("{}", parsed.rule_debug(idx));
            eprintln!("{}", parsed.rule_pattern_debug(idx));
            eprintln!("{}", parsed.rule_debug(idx));
            eprintln!("{}", parsed.rule_pattern_debug(idx));
        }
        let text = "To jest co\u{15b}, co wiedzia\u{142}em.";
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        if let Some(idx) = parsed
            .rule_ids()
            .iter()
            .position(|id| *id == "PUNCT_NO_DOT")
        {
            parsed.apply_rule(&mut sentence, idx);
            for t in sentence.non_whitespace_tokens() {
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                    .collect();
                eprintln!(
                    "after-rule {:?} => {}",
                    t.token().token(),
                    readings.join(", ")
                );
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_parser_exception_text_shapes() {
        let _release_guard = crate::release_guard();
        let mk = |token_body: &str| {
            format!(
                "<rules lang=\"pl\"><unification feature=\"number\"><equivalence type=\"sg\"><token postag=\".*\\bsg\\b.*\" postag_regexp=\"yes\"/></equivalence><equivalence type=\"pl\"><token postag=\".*\\bpl(?:tant)?\\b.*\" postag_regexp=\"yes\"/></equivalence></unification><unification feature=\"gender\"><equivalence type=\"f\"><token postag=\".*\\bf\\b.*\" postag_regexp=\"yes\"/></equivalence></unification><unification feature=\"case\"><equivalence type=\"nom\"><token postag=\".*\\bnom\\b.*\" postag_regexp=\"yes\"/></equivalence></unification><rule name=\"t\" id=\"T1\"><pattern>{token_body}</pattern><disambig action=\"add\"><wd pos=\"TEST\"/></disambig></rule></rules>"
            )
        };
        let shapes = [
            ("<token>abc</token>", "plain"),
            (
                "<token>abc<exception postag=\"SENT_END\"/></token>",
                "text-then-exception",
            ),
            (
                "<token><exception postag=\"SENT_END\"/>abc</token>",
                "exception-then-text",
            ),
            (
                "<token regexp=\"yes\">[abc]+<exception postag=\"SENT_END\"/></token>",
                "regex-text-then-exception",
            ),
            (
                "<token regexp=\"yes\">[\\p{Punct}-\\u{2212}\\u{2013}\\u{2014}\\u{2015}\\u{201e}\\u{00ab}\\u{00bb}\\u{2026}\\u{201d}&&[^!.]]<exception postag=\"SENT_END\"/><exception postag=\"interp:nospace\"/></token>",
                "real-char-class-2-exc",
            ),
            (
                "<token regexp=\"yes\">[\\p{Punct}-\\u{2212}\\u{2013}\\u{2014}\\u{2015}\\u{201e}\\u{00ab}\\u{00bb}\\u{2026}\\u{201d}&&[^!.]]</token>",
                "real-char-class-no-exc",
            ),
        ];
        let real = "[\\p{Punct}-\\u{2212}\\u{2013}\\u{2014}\\u{2015}\\u{201e}\\u{00ab}\\u{00bb}\\u{2026}\\u{201d}&&[^!.]]";
        match regex::Regex::new(real) {
            Ok(re) => eprintln!(
                "regex-crate real class OK: w={} ,={} .={}",
                re.is_match("w"),
                re.is_match(","),
                re.is_match(".")
            ),
            Err(e) => eprintln!("regex-crate real class FAIL: {e}"),
        }
        let real2 = "[\\p{Punct}\\-\\u{2013}\\u{2212}\\u{2014}\\u{2015}\\u{201e}\\u{00ab}\\u{00bb}\\u{2026}\\u{201d}\\u{2019}&&[^\\)]]+";
        match regex::Regex::new(real2) {
            Ok(re) => eprintln!(
                "regex-crate grammar-entity class OK: w={} ,={}",
                re.is_match("w"),
                re.is_match(",")
            ),
            Err(e) => eprintln!("regex-crate grammar-entity class FAIL: {e}"),
        }
        for (body, label) in shapes {
            let xml = mk(body);
            match og_tagger::XmlDisambiguator::from_xml_with_english(&xml, None) {
                Ok(d) => {
                    let dbg = d.rule_pattern_debug(0);
                    eprintln!("{label}: {dbg}");
                }
                Err(e) => eprintln!("{label}: PARSE FAIL {e}"),
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_double_hyphen_rule() {
        let _release_guard = crate::release_guard();
        pattern_of("LACZNIK_MYSLNIK");
        probe("LACZNIK_MYSLNIK", "Rozumiesz -- przegrywasz.");
    }

    #[ignore]
    #[test]
    fn probe_nietypowa_antipattern_trace() {
        let _release_guard = crate::release_guard();
        probe_with_engine_trace(
            "NIETYPOWA_KOMBINACJA_DUZYCH_I_MALYCH_LITER",
            "To jest dziwnie napisane s\u{141}oWo.",
        );
    }

    #[ignore]
    #[test]
    fn probe_zdania_zlozone_family() {
        let _release_guard = crate::release_guard();
        pattern_of("ZDANIA_ZLOZONE");
        probe(
            "ZDANIA_ZLOZONE",
            "Kupi\u{142}em wino chcia\u{142}em te\u{17c} pi\u{107} wod\u{119}.",
        );
    }

    #[ignore]
    #[test]
    fn probe_adj_subst_adj_unify_residue() {
        let _release_guard = crate::release_guard();
        pattern_of("ADJ_SUBST_ADJ_UNIFY");
        probe(
            "ADJ_SUBST_ADJ_UNIFY",
            "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.",
        );
        probe(
            "ADJ_SUBST_ADJ_UNIFY",
            "Sosna jest du\u{17c}y drzewem iglastym.",
        );
        probe(
            "ADJ_SUBST_ADJ_UNIFY",
            "Lubi\u{119} pi\u{142}k\u{119} no\u{17c}n\u{105}.",
        );
    }

    #[ignore]
    #[test]
    fn probe_weekday_and_name_rules() {
        let _release_guard = crate::release_guard();
        pattern_of("DNI_TYGODNIA");
        probe(
            "DNI_TYGODNIA",
            "To si\u{119} sta\u{142}o w wielki pi\u{105}tek.",
        );
        pattern_of("BIALY_STOK");
        probe(
            "BIALY_STOK",
            "Jakie zabytki nadaja si\u{119} na poczt\u{f3}wki w Bia\u{142}ym stoku?",
        );
        pattern_of("HYPER");
        probe(
            "HYPER",
            "W tym tek\u{15b}cie s\u{105} liczne hyperodno\u{15b}niki.",
        );
    }

    #[ignore]
    #[test]
    fn probe_round10_single_residues() {
        let _release_guard = crate::release_guard();
        probe(
            "JEDNAK_PRZECINEK",
            "Jednak\u{17c}e, warto spr\u{f3}bowa\u{107}.",
        );
        probe("WIECEJ_JAK", "Wi\u{119}cej jak Kosmetyk!");
        probe(
            "NIETYPOWA_KOMBINACJA_DUZYCH_I_MALYCH_LITER",
            "To jest dziwnie napisane s\u{141}oWo.",
        );
        probe("O_WYM", "Rozmawiali o tym i o wym.");
        probe("WLADCE", "Surowce zawieraj\u{105}ce te alkaloidy");
        probe(
            "SPACJA_ZA_PRZECINKIEM_DZIESITNYM",
            "Wzr\u{f3}s\u{142} zaledwie o 1, 9%.",
        );
        probe("NIEZGODNOSC_PRZYPADKU_PO_LICZEBNIKU", "Mam 1,5% chleb.");
        probe(
            "MNIE_MNIEJ",
            "P\u{142}ac\u{105} nawet 40% mnie ni\u{17c} w sklepie.",
        );
        probe(
            "SKROTY_Z_KROPKA",
            "Stopie\u{144} dysocjacji w temperaturze 4000 K wynosi nieca\u{142}e 3%.",
        );
        probe("PO_SRODKU", "Prawda po \u{15b}rodku");
        probe("PO_WOLI", "\u{15a}pieszcie si\u{119} po woli");
        probe("BRAK_KROPKI", "Po\u{17c}ycz mi sto z\u{142}otych.");
        probe("BRAK_KROPKI", "Po\u{17c}ycz mi sto z\u{142}otych\u{2026}");
        probe(
            "BRAK_KROPKI",
            "To jest zdanie z \u{201e}cytatem na ko\u{144}cu i przypisem\u{201d}.",
        );
        probe("RYMY", "Tu wyl\u{105}dowa\u{142}y oddzia\u{142}y.");
        probe("PREP_CASUS", "By\u{142}em przy grobie Mickiewicza.");
        probe(
            "UZYWAC_CO",
            "Chcia\u{142}by\u{15b} u\u{17c}y\u{107} troch\u{119} \u{17c}ycia?",
        );
        probe(
            "ZA_ZWYCZAJ",
            "W Egipcie wszyscy dygnitarze uwa\u{17c}aj\u{105} \u{142}ap\u{f3}wki za zwyczaj tak powszechny...",
        );
        probe("IMIONA_Z_APOSTROFAMI", "Porozmawiajmy o IPN'ie.");
    }

    #[ignore]
    #[test]
    fn probe_disambig_lemma_remove_actions() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        for wanted in ["K", "k_bez_kropki"] {
            let Some(idx) = parsed.rule_ids().iter().position(|id| *id == wanted) else {
                eprintln!("rule {wanted}: NOT FOUND");
                continue;
            };
            eprintln!("rule {wanted} idx {idx}: {}", parsed.rule_debug(idx));
            let text = "Stopie\u{144} dysocjacji w temperaturze 4000 K wynosi nieca\u{142}e 3%.";
            let mut sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            parsed.apply_rule(&mut sentence, idx);
            for t in sentence.non_whitespace_tokens() {
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                    .collect();
                eprintln!(
                    "   after {wanted}: {:?} => {}",
                    t.token().token(),
                    readings.join(", ")
                );
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_round15_zdania_zlozone_residues() {
        let _release_guard = crate::release_guard();
        for text in [
            "Romuald odpowiedzia\u{142} z wa\u{17c}n\u{105} i tajemnicz\u{105} min\u{105} zwielokrotniaj\u{105}c g\u{142}osk\u{119}.",
            "Je\u{15b}li w w\u{119}glowodorze wyst\u{119}puje wi\u{105}zanie podw\u{f3}jne i potr\u{f3}jne to wa\u{17c}niejsze przy nazewnictwie jest podw\u{f3}jne.",
            "Fatma uca\u{142}owawszy r\u{119}ce obu in\u{17c}ynier\u{f3}w i dzieci wysz\u{142}a, b\u{142}ogos\u{142}awi\u{105}c szczeg\u{f3}lniej Nel.",
            "Kupi\u{142}em wino chcia\u{142}em te\u{17c} pi\u{107} wod\u{119}.",
        ] {
            probe("ZDANIA_ZLOZONE", text);
        }
    }

    #[ignore]
    #[test]
    fn probe_round15_zdania_zlozone_engine_trace() {
        let _release_guard = crate::release_guard();
        probe_with_engine_trace(
            "ZDANIA_ZLOZONE",
            "Romuald odpowiedzia\u{142} z wa\u{17c}n\u{105} i tajemnicz\u{105} min\u{105} zwielokrotniaj\u{105}c g\u{142}osk\u{119}.",
        );
    }

    #[ignore]
    #[test]
    fn probe_round15_remaining_singles() {
        let _release_guard = crate::release_guard();
        probe(
            "ZAGNIEZDZONE_CUDZYSLOWY",
            "\u{201e}Ka\u{17c}dy noemat ma \u{2018}tre\u{15b}\u{107}\u{2019}\u{201d}.",
        );
        probe(
            "ADJ_SUBST_ADJ_UNIFY",
            "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.",
        );
        probe(
            "ZDANIE_PODRZEDNE_Z_KTORY_LUB_JAKI",
            "I skoczy\u{142} ku domowi, w kt\u{f3}rego sieni s\u{142}ycha\u{107} by\u{142}o \u{15b}iej\u{105}ce si\u{119} g\u{142}osy i huk obracanych \u{17c}aren.",
        );
        probe(
            "NIETYPOWA_KOMBINACJA_DUZYCH_I_MALYCH_LITER",
            "To jest dziwnie napisane s\u{141}oWo.",
        );
        probe("O_WYM", "Rozmawiali o tym i o wym.");
        probe("WLADCE", "Surowce zawieraj\u{105}ce te alkaloidy");
        probe(
            "BRAK_PRZECINKA_ZE",
            "I c\u{f3}\u{17c}, stary Balu \u{2014} doda\u{142}a \u{2014} jak \u{17c}e\u{15b} wyszed\u{142} z tej przygody?",
        );
        probe(
            "BRAK_PRZECINKA_GDY",
            "Co znajdujemy gdy \u{201e}spogl\u{105}damy\u{201d} wewn\u{105}trz?",
        );
        probe(
            "BRAK_PRZECINKA_ANI",
            "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.",
        );
        probe("PREP_CASUS", "By\u{142}em przy grobie Mickiewicza.");
        probe("RYMY", "Tu wyl\u{105}dowa\u{142}y oddzia\u{142}y.");
        probe("IMIONA_Z_APOSTROFAMI", "Porozmawiajmy o IPN'ie.");
        probe("PO_SRODKU", "Prawda po \u{15b}rodku");
        probe("PO_WOLI", "\u{15a}pieszcie si\u{119} po woli");
        probe(
            "ZA_ZWYCZAJ",
            "W Egipcie wszyscy dygnitarze uwa\u{17c}aj\u{105} \u{142}ap\u{f3}wki za zwyczaj tak powszechny...",
        );
    }

    #[ignore]
    #[test]
    fn probe_round15_disambig_rule_effects() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let snapshot = |s: &og_core::AnalyzedSentence, watched: &str| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .filter(|t| t.token().token() == watched)
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().join("|"))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect()
        };
        let cases: &[(&str, &str)] = &[
            (
                "I c\u{f3}\u{17c}, stary Balu \u{2014} doda\u{142}a \u{2014} jak \u{17c}e\u{15b} wyszed\u{142} z tej przygody?",
                "jak",
            ),
            ("By\u{142}em przy grobie Mickiewicza.", "grobie"),
            (
                "Tu wyl\u{105}dowa\u{142}y oddzia\u{142}y.",
                "wyl\u{105}dowa\u{142}y",
            ),
            ("Rozmawiali o tym i o wym.", "wym"),
            ("O tym i o wym", "wym"),
            ("\u{15a}pieszcie si\u{119} po woli", "woli"),
            ("Prawda po \u{15b}rodku", "\u{15b}rodku"),
            (
                "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.",
                "b\u{105}d\u{17a}",
            ),
        ];
        for (text, watched) in cases {
            eprintln!("=== '{text}' watching {watched:?} ===");
            let mut sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            eprintln!("   pre  {watched:?}: {:?}", snapshot(&sentence, watched));
            for (idx, id) in parsed.rule_ids().iter().enumerate() {
                let before = snapshot(&sentence, watched);
                parsed.apply_rule(&mut sentence, idx);
                let after = snapshot(&sentence, watched);
                if before != after {
                    eprintln!("   rule {id} (idx {idx}): {before:?} -> {after:?}");
                }
            }
            eprintln!("   post {watched:?}: {:?}", snapshot(&sentence, watched));
        }
    }

    #[ignore]
    #[test]
    fn probe_round15_o_wym_disambig_rule() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let snapshot = |s: &og_core::AnalyzedSentence| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .map(|t| {
                    format!(
                        "{:?}:{}",
                        t.token().token(),
                        t.readings()
                            .iter()
                            .map(|r| r.pos_tags().join("|"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect()
        };
        for wanted in ["O_WYM_", "o_wym_2"] {
            let Some(idx) = parsed.rule_ids().iter().position(|id| *id == wanted) else {
                eprintln!("rule {wanted}: NOT FOUND");
                continue;
            };
            eprintln!("== rule {wanted} (idx {idx}) ==");
            eprintln!("{}", parsed.rule_debug(idx));
            for text in ["Rozmawiali o tym i o wym.", "O tym i o wym"] {
                let mut sentence =
                    crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
                let before = snapshot(&sentence);
                parsed.apply_rule(&mut sentence, idx);
                let after = snapshot(&sentence);
                eprintln!("  '{text}'");
                eprintln!("    before {before:?}");
                eprintln!("    after  {after:?}");
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_round15_zagniezdzone_wladce_traces() {
        let _release_guard = crate::release_guard();
        pattern_of("ZAGNIEZDZONE_CUDZYSLOWY");
        probe_with_engine_trace(
            "ZAGNIEZDZONE_CUDZYSLOWY",
            "\u{201e}Ka\u{17c}dy noemat ma \u{2018}tre\u{15b}\u{107}\u{2019}\u{201d}.",
        );
        pattern_of("WLADCE");
        probe_with_engine_trace("WLADCE", "Surowce zawieraj\u{105}ce te alkaloidy");
    }

    #[test]
    fn zdania_zlozone_residue_pin_scope_next_window_overvetoes_java_landing_candidate() {
        let pipeline = PolishPipeline::shared();
        let engine = trace_engine();
        let text = "Romuald odpowiedzia\u{142} z wa\u{17c}n\u{105} i tajemnicz\u{105} min\u{105} zwielokrotniaj\u{105}c g\u{142}osk\u{119}.";
        let mut matches = 0usize;
        for rule in compiled_rules()
            .rules
            .iter()
            .filter(|r| r.id == "ZDANIA_ZLOZONE")
        {
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                matches += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            matches, 0,
            "the ZDANIA_ZLOZONE no_match family (12 of the 16 residues): on this sentence the \
             engine trace stops exploring element 1 candidates after 'wa\u{17c}n\u{105}' because \
             every landing candidate whose (token_idx+1)..=after window (pattern_rule.rs:\
             1380-1388, :1430-1437) contains 'i'[comp:comma] is vetoed by element 0's scope=next \
             exception SENT_END|<ELLIPSIS>|interp:comma|comp:comma, so 'zwielokrotniaj\u{105}c' \
             [pcon:imperf] is never reached; Java matches this example via \
             AbstractPatternRulePerformer.java:169-193, whose prevMatched logic (reset per \
             element at :106, accumulated at :181-191, sticky across the candidate loop) is the \
             reference the shared engine must reproduce - this pin flips to >0 when it does"
        );
    }

    #[test]
    fn zdanie_podrzepne_residue_pin_overfires_where_java_sticky_veto_rejects() {
        let pipeline = PolishPipeline::shared();
        let engine = trace_engine();
        let text = "Nic, nawet d\u{119}bu, po kt\u{f3}rego stuletniej korze ze\u{15b}lizgiwa\u{142}y si\u{119} wst\u{119}gi piorun\u{f3}w.";
        let mut matches = 0usize;
        for rule in compiled_rules()
            .rules
            .iter()
            .filter(|r| r.id == "ZDANIE_PODRZEDNE_Z_KTORY_LUB_JAKI")
        {
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                matches += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            matches, 1,
            "the ZDANIE_PODRZEDNE_Z_KTORY_LUB_JAKI fp pair (2 of the 16 residues): our engine \
             matches this correct example where Java rejects it, so some Java-side veto is \
             missing - the same AbstractPatternRulePerformer.java:106,169-193 prevMatched \
             machinery (per-element reset, per-reading accumulation, stickiness across the \
             candidate loop) is the reference - this pin flips to 0 when the shared engine \
             reproduces it"
        );
    }

    #[test]
    fn wladce_residue_pin_still_fires_the_false_positive() {
        let pipeline = PolishPipeline::shared();
        let engine = trace_engine();
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let mut matches = 0usize;
        for rule in compiled_rules().rules.iter().filter(|r| r.id == "WLADCE") {
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                matches += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            matches, 1,
            "the WLADCE fp (1 of the 16 residues): the exact-case Surowiec/depr:pl:nom:m2 \
             reading survives the whole module pipeline and no normalized disambiguation.xml \
             rule touches it even applied individually (probe_round18_wladce_individual_rule_\
             effects), so the Java-side drop must live in an og-tagger matcher path our engine \
             does not reproduce - the count-guard family is a proven divergence in both \
             directions (DisambiguationPatternRuleReplacer.java:171-173 no-ops unify_adj_subst \
             sub-rule 4, whose 2 unified elements face 3 matched elements, while our engine \
             applies it and drops zawieraj\u{105}ce's pact:sg reading), and the 90 scope=next \
             exceptions plus filterall matched-readings semantics are the other unverified \
             families - this pin flips to 0 when the shared engine reproduces the Java drop"
        );
    }

    #[test]
    fn quote_class_survives_java_ci_compilation() {
        let plain = regex::RegexBuilder::new("^(?:[\u{2018}\u{201e}])$")
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(
            plain.is_match("\u{2018}"),
            "plain ci compile matches the class member"
        );
        let folded = og_xml::java_case_fold::compile_java_ci("^(?:[\u{2018}\u{201e}])$");
        assert!(
            folded.as_ref().is_some_and(|re| re.is_match("\u{2018}")),
            "java_case_fold must keep the quote class intact"
        );
    }

    #[ignore]
    #[test]
    fn probe_round15_prep_casus_trace() {
        let _release_guard = crate::release_guard();
        probe_with_engine_trace("PREP_CASUS", "By\u{142}em przy grobie Mickiewicza.");
    }

    #[ignore]
    #[test]
    fn probe_round15_rymy_trace() {
        let _release_guard = crate::release_guard();
        pattern_of("RYMY");
        probe_with_engine_trace("RYMY", "Tu wyl\u{105}dowa\u{142}y oddzia\u{142}y.");
    }

    fn disambiguated_sentence(text: &str) -> og_core::AnalyzedSentence {
        PolishPipeline::shared()
            .analyze_offset_text(text)
            .into_iter()
            .next()
            .map(|(s, _)| s)
            .unwrap_or_else(|| PolishPipeline::shared().analyze_range(text, 0, text.len()))
    }

    #[ignore]
    #[test]
    fn probe_round16_badz_koordynacja_2_isolation() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let text = "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.";
        let surowce_text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let beztlenowe_text =
            "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.";
        let snapshot = |s: &og_core::AnalyzedSentence| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .map(|t| {
                    format!(
                        "{:?}:{}",
                        t.token().token(),
                        t.readings()
                            .iter()
                            .map(|r| r.pos_tags().join("|"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect()
        };
        for wanted in [
            "badz_koordynacja_2",
            "inst_badz_inst",
            "TAM_JEST",
            "unify_adj_subst",
            "MAJA_MAIC",
        ] {
            let rule_text = match wanted {
                "unify_adj_subst" => surowce_text,
                "MAJA_MAIC" => beztlenowe_text,
                _ => text,
            };
            let indices: Vec<usize> = parsed
                .rule_ids()
                .iter()
                .enumerate()
                .filter(|(_, id)| **id == wanted)
                .map(|(i, _)| i)
                .collect();
            for idx in indices {
                eprintln!("== rule {wanted} (idx {idx}) in isolation ==");
                eprintln!("{}", parsed.rule_debug(idx));
                eprintln!("pattern: {}", parsed.rule_pattern_debug(idx));
                let mut sentence =
                    crate::pl::test_support::analyze_undisambiguated(rule_text, 0, rule_text.len());
                let before = snapshot(&sentence);
                let hits = parsed.debug_match_rule(&mut sentence, idx);
                eprintln!("   debug_match_rule positions: {hits:?}");
                parsed.apply_rule(&mut sentence, idx);
                let after = snapshot(&sentence);
                let mut diff = Vec::new();
                for (b, a) in before.iter().zip(after.iter()) {
                    if b != a {
                        diff.push(format!("{b} -> {a}"));
                    }
                }
                eprintln!("   isolated effect: {}", diff.join(" ;; "));
                if wanted == "badz_koordynacja_2" {
                    let with_comma = "Dojad\u{119} tam b\u{105}d\u{17a} rowerem, b\u{105}d\u{17a} hulajnog\u{105}.";
                    let mut sentence = crate::pl::test_support::analyze_undisambiguated(
                        with_comma,
                        0,
                        with_comma.len(),
                    );
                    let before = snapshot(&sentence);
                    let hits = parsed.debug_match_rule(&mut sentence, idx);
                    parsed.apply_rule(&mut sentence, idx);
                    let after = snapshot(&sentence);
                    let comma_diff: Vec<String> = before
                        .iter()
                        .zip(after.iter())
                        .filter(|(b, a)| b != a)
                        .map(|(b, a)| format!("{b} -> {a}"))
                        .collect();
                    eprintln!(
                        "   comma-ful text: match {hits:?} effect {}",
                        comma_diff.join(" ;; ")
                    );
                }
            }
        }
    }

    fn disambig_variant_effect(rule_id: &str, replacement_rule_xml: &str, text: &str) -> String {
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let open = "<rule ".to_string();
        let needle = format!("id=\"{rule_id}\"");
        let id_pos = xml.find(&needle).unwrap_or_else(|| panic!("{rule_id}"));
        let rule_start = xml[..id_pos].rfind(&open).expect("rule open");
        let rule_end = xml[rule_start..]
            .find("</rule>")
            .unwrap_or_else(|| panic!("{rule_id} end"))
            + rule_start
            + "</rule>".len();
        let mut variant = String::with_capacity(xml.len());
        variant.push_str(&xml[..rule_start]);
        variant.push_str(replacement_rule_xml);
        variant.push_str(&xml[rule_end..]);
        let normalized = crate::pl::normalize_disambiguation_xml(&variant);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse variant");
        let Some(idx) = parsed.rule_ids().iter().position(|id| *id == rule_id) else {
            return "VARIANT RULE NOT FOUND".to_string();
        };
        let snapshot = |s: &og_core::AnalyzedSentence| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .map(|t| {
                    format!(
                        "{:?}:{}",
                        t.token().token(),
                        t.readings()
                            .iter()
                            .map(|r| r.pos_tags().join("|"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect()
        };
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        let before = snapshot(&sentence);
        let hits = parsed.debug_match_rule(&mut sentence, idx);
        parsed.apply_rule(&mut sentence, idx);
        let after = snapshot(&sentence);
        let mut diff = Vec::new();
        for (b, a) in before.iter().zip(after.iter()) {
            if b != a {
                diff.push(format!("{b} -> {a}"));
            }
        }
        format!(
            "match_positions={hits:?} effect={}",
            if diff.is_empty() {
                "<none>".to_string()
            } else {
                diff.join(" ;; ")
            }
        )
    }

    #[ignore]
    #[test]
    fn probe_round16_badz_koordynacja_2_variant_bisect() {
        let _release_guard = crate::release_guard();
        let text = "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.";
        let original = r#"<rule name="Bądź i lub jako spójnik w konstrukcji „bądź x, bądź y”" id="badz_koordynacja_2">
        <pattern>
            <marker><token regexp="yes">bądź|lub</token></marker>
            <unify>
                <feature id="case"><type id="acc"/><type id="inst"/><type id="gen"/><type id="dat"/><type id="loc"/><type id="nom"/></feature>
                <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
                <unify-ignore>
                    <token min="0" max="1">,</token>
                    <token regexp="yes">bądź|lub</token>
                </unify-ignore>
                <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            </unify>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        let no_marker = r#"<rule name="v" id="badz_koordynacja_2">
        <pattern>
            <token regexp="yes">bądź|lub</token>
            <unify>
                <feature id="case"><type id="acc"/><type id="inst"/><type id="gen"/><type id="dat"/><type id="loc"/><type id="nom"/></feature>
                <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
                <unify-ignore>
                    <token min="0" max="1">,</token>
                    <token regexp="yes">bądź|lub</token>
                </unify-ignore>
                <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            </unify>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        let no_unify = r#"<rule name="v" id="badz_koordynacja_2">
        <pattern>
            <marker><token regexp="yes">bądź|lub</token></marker>
            <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            <token min="0" max="1">,</token>
            <token regexp="yes">bądź|lub</token>
            <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        let no_optional_comma = r#"<rule name="v" id="badz_koordynacja_2">
        <pattern>
            <marker><token regexp="yes">bądź|lub</token></marker>
            <unify>
                <feature id="case"><type id="acc"/><type id="inst"/><type id="gen"/><type id="dat"/><type id="loc"/><type id="nom"/></feature>
                <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
                <unify-ignore>
                    <token regexp="yes">bądź|lub</token>
                </unify-ignore>
                <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            </unify>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        let no_ignore = r#"<rule name="v" id="badz_koordynacja_2">
        <pattern>
            <marker><token regexp="yes">bądź|lub</token></marker>
            <unify>
                <feature id="case"><type id="acc"/><type id="inst"/><type id="gen"/><type id="dat"/><type id="loc"/><type id="nom"/></feature>
                <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
                <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            </unify>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        let marker_whole = r#"<rule name="v" id="badz_koordynacja_2">
        <pattern>
            <marker>
            <token regexp="yes">bądź|lub</token>
            <unify>
                <feature id="case"><type id="acc"/><type id="inst"/><type id="gen"/><type id="dat"/><type id="loc"/><type id="nom"/></feature>
                <token postag="(?:adj|pact|ppas|subst|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
                <unify-ignore>
                    <token min="0" max="1">,</token>
                    <token regexp="yes">bądź|lub</token>
                </unify-ignore>
                <token postag="(?:adj|pact|ppas|subs|ger).*" postag_regexp="yes"><exception negate_pos="yes" postag_regexp="yes" postag="(?:adj|pact|ppas|subst|ger).*"/></token>
            </unify>
            </marker>
        </pattern>
        <disambig action="filter" postag="conj"/>
    </rule>"#;
        for (label, xml) in [
            ("original", original),
            ("no-marker", no_marker),
            ("no-unify", no_unify),
            ("no-optional-comma", no_optional_comma),
            ("no-unify-ignore", no_ignore),
            ("marker-whole-pattern", marker_whole),
        ] {
            eprintln!(
                "== variant {label}: {}",
                disambig_variant_effect("badz_koordynacja_2", xml, text)
            );
        }
    }

    #[ignore]
    #[test]
    fn probe_round16_wladce_rules_matching_surowce() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        for (idx, id) in parsed.rule_ids().iter().enumerate() {
            let mut sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            let hits = parsed.debug_match_rule(&mut sentence, idx);
            let touches = hits.iter().any(|positions| positions.contains(&1))
                || hits.iter().any(|positions| positions.first() == Some(&1));
            if touches {
                let before: Vec<String> = sentence
                    .non_whitespace_tokens()
                    .iter()
                    .map(|t| {
                        format!(
                            "{:?}:{}",
                            t.token().token(),
                            t.readings()
                                .iter()
                                .map(|r| r.pos_tags().join("|"))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect();
                parsed.apply_rule(&mut sentence, idx);
                let after: Vec<String> = sentence
                    .non_whitespace_tokens()
                    .iter()
                    .map(|t| {
                        format!(
                            "{:?}:{}",
                            t.token().token(),
                            t.readings()
                                .iter()
                                .map(|r| r.pos_tags().join("|"))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect();
                let changed = before != after;
                eprintln!("   rule {id} (idx {idx}) matches-at-Surowce {hits:?} changed={changed}");
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_round16_zdania_fp_expansion_bisect() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let raw = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let with_expansion = crate::pl::normalize_disambiguation_xml(&raw);
        let escaped = raw.replace(
            "[\\p{Punct}-\u{2212}\u{2013}\u{2014}\u{2015}\u{201e}\u{00ab}\u{00bb}\u{2026}\u{201d}&amp;&amp;[^!\\.]",
            "[\\p{Punct}\\-\u{2212}\u{2013}\u{2014}\u{2015}\u{201e}\u{00ab}\u{00bb}\u{2026}\u{201d}&amp;&amp;[^!\\.]",
        );
        let and_form = escaped.replace(
            "<token><exception negate_pos=\"yes\" postag=\"SENT_END\">.</exception></token>",
            "<and><token>.</token><token postag=\"SENT_END\"/></and>",
        );
        let wd_form = and_form.replace(
            "<disambig postag=\"replace\">",
            "<disambig action=\"replace\">",
        );
        let filtered = crate::pl::append_sent_end_to_filter_postags(&wd_form).unwrap_or(wd_form);
        let class_fixed = crate::pl::rewrite_java_regex_classes(&filtered).unwrap_or(filtered);
        let maja_fixed = crate::pl::maja_maic_match_filter_to_remove(&class_fixed);
        let without_expansion = crate::pl::wildcard_lemma_only_remove_wds(&maja_fixed);
        let parsed_with =
            og_tagger::XmlDisambiguator::from_xml_with_english(&with_expansion, None).unwrap();
        let parsed_without =
            og_tagger::XmlDisambiguator::from_xml_with_english(&without_expansion, None).unwrap();
        let texts = [
            "Nast\u{105}pi\u{142}o g\u{142}osowanie i Wilczur zosta\u{142} wybrany.",
            "Musz\u{119} tam czeka\u{107} \u{15b}mierci, gdy\u{17c} jestem stary i nerwy moje s\u{105} chore.",
        ];
        for text in texts {
            eprintln!("=== '{text}' ===");
            let mut with_sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            let mut without_sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            let fmt = |s: &og_core::AnalyzedSentence| -> Vec<String> {
                s.non_whitespace_tokens()
                    .iter()
                    .map(|t| {
                        format!(
                            "{:?}:{}",
                            t.token().token(),
                            t.readings()
                                .iter()
                                .map(|r| r.pos_tags().join("|"))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect()
            };
            for (idx, id) in parsed_with.rule_ids().iter().enumerate() {
                let before = fmt(&with_sentence);
                parsed_with.apply_rule(&mut with_sentence, idx);
                let after = fmt(&with_sentence);
                if before != after {
                    let mut diffs = Vec::new();
                    for (b, a) in before.iter().zip(after.iter()) {
                        if b != a {
                            diffs.push(format!("{b} -> {a}"));
                        }
                    }
                    eprintln!("   [with] rule {id} (idx {idx}): {}", diffs.join(" ;; "));
                }
            }
            for (idx, _id) in parsed_without.rule_ids().iter().enumerate() {
                parsed_without.apply_rule(&mut without_sentence, idx);
            }
            let with_final = fmt(&with_sentence);
            let without_final = fmt(&without_sentence);
            for (w, wo) in with_final.iter().zip(without_final.iter()) {
                if w != wo {
                    eprintln!("   DIFF {w} vs {wo}");
                }
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_round16_wladce_surowce_disambig_trace() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let watched = "Surowce";
        let snapshot = |s: &og_core::AnalyzedSentence, w: &str| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .filter(|t| t.token().token() == w)
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| {
                            format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect()
        };
        eprintln!("=== '{text}' watching {watched:?} ===");
        let resource_dir2 = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        if let Some(dict) = og_tagger::generic_tagger::shared_dictionary(&resource_dir2, "polish") {
            for w in ["Surowce", "surowce", "Surowcem", "surowcem"] {
                let readings: Vec<String> = dict
                    .lookup(w)
                    .iter()
                    .map(|(lemma, pos)| format!("{lemma}/{pos}"))
                    .collect();
                eprintln!("   dict[{w}] = {}", readings.join(", "));
            }
        }
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        eprintln!("   pre  {watched:?}: {:?}", snapshot(&sentence, watched));
        for (idx, id) in parsed.rule_ids().iter().enumerate() {
            let before = snapshot(&sentence, watched);
            parsed.apply_rule(&mut sentence, idx);
            let after = snapshot(&sentence, watched);
            if before != after {
                eprintln!("   rule {id} (idx {idx}): {before:?} -> {after:?}");
            }
        }
        eprintln!("   post {watched:?}: {:?}", snapshot(&sentence, watched));
        eprintln!("   full pipeline post: {:?}", {
            let s = disambiguated_sentence(text);
            s.non_whitespace_tokens()
                .into_iter()
                .find(|t| t.token().token() == watched)
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| {
                            format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+"))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
        });
    }

    #[ignore]
    #[test]
    fn probe_round17_wladce_sequential_disambig_scan() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        for (i, t) in sentence.non_whitespace_tokens().iter().enumerate() {
            eprintln!("   token {i}: {:?}", t.token().token());
        }
        let surowce_reading = |s: &og_core::AnalyzedSentence| -> String {
            s.non_whitespace_tokens()
                .iter()
                .find(|t| t.token().token() == "Surowce")
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().join("+"))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default()
        };
        for (idx, id) in parsed.rule_ids().iter().enumerate() {
            let hits = parsed.debug_match_rule(&mut sentence, idx);
            let covers = hits.iter().any(|positions| positions.contains(&1));
            let before = surowce_reading(&sentence);
            parsed.apply_rule(&mut sentence, idx);
            let after = surowce_reading(&sentence);
            if covers || before != after {
                eprintln!(
                    "   rule {id} (idx {idx}) covers={covers} hits={hits:?} surowce_changed={}",
                    before != after
                );
                if covers {
                    eprintln!("     pattern: {}", parsed.rule_pattern_debug(idx));
                }
                if before != after {
                    eprintln!("     surowce: {before} -> {after}");
                }
            }
        }
        eprintln!("   final Surowce: {}", surowce_reading(&sentence));
    }

    #[ignore]
    #[test]
    fn probe_round18_unify_adj_subst_subrules() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        for (idx, id) in parsed.rule_ids().iter().enumerate() {
            if *id != "unify_adj_subst" {
                continue;
            }
            eprintln!("== idx {idx} ==\n{}", parsed.rule_debug(idx));
        }
    }

    #[ignore]
    #[test]
    fn probe_round18_wladce_control_examples() {
        let _release_guard = crate::release_guard();
        for text in [
            "Tylko rodzice potrafi\u{105} ich odr\u{f3}\u{17c}ni\u{107}.",
            "Wo\u{17a}nice odebrali ma\u{142}ym konikom worki z obrokiem.",
            "Uwielbiam tego gruzi\u{144}skiego tw\u{f3}rce ludowego.",
        ] {
            probe("WLADCE", text);
        }
    }

    #[ignore]
    #[test]
    fn probe_round18_wladce_individual_rule_effects() {
        let _release_guard = crate::release_guard();
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let snapshot = |s: &og_core::AnalyzedSentence| -> Vec<String> {
            s.non_whitespace_tokens()
                .iter()
                .map(|t| {
                    format!(
                        "{:?}:{}",
                        t.token().token(),
                        t.readings()
                            .iter()
                            .map(|r| r.pos_tags().join("|"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect()
        };
        for (idx, id) in parsed.rule_ids().iter().enumerate() {
            let mut sentence =
                crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            let hits = parsed.debug_match_rule(&mut sentence, idx);
            let before = snapshot(&sentence);
            parsed.apply_rule(&mut sentence, idx);
            let after = snapshot(&sentence);
            if before != after {
                let diffs = before
                    .iter()
                    .zip(after.iter())
                    .filter(|(b, a)| b != a)
                    .map(|(b, a)| format!("{b} -> {a}"))
                    .collect::<Vec<_>>()
                    .join(" ;; ");
                eprintln!("rule {id} (idx {idx}) hits={hits:?}: {diffs}");
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_round20_wladce_initial_readings() {
        let _release_guard = crate::release_guard();
        for text in [
            "Surowce zawieraj\u{105}ce te alkaloidy",
            "Wo\u{17a}nice odebrali ma\u{142}ym konikom worki z obrokiem.",
            "Uwielbiam tego gruzi\u{144}skiego tw\u{f3}rce ludowego.",
        ] {
            eprintln!("=== initial readings '{text}' ===");
            let sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
            for t in sentence.non_whitespace_tokens() {
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                    .collect();
                eprintln!("   {:?} => {}", t.token().token(), readings.join(", "));
            }
        }
    }

    fn adj_rule_variant_xml(rule_id: &str, strip: &[usize]) -> (String, usize) {
        let path = crate::pl::pl_rules_dir()
            .expect("pl rules dir")
            .join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("grammar.xml");
        let xml = crate::pl::normalize_pl_rules_xml(&xml);
        let needle = format!("<rule id=\"{rule_id}\"");
        let rule_start = xml
            .find(&needle)
            .unwrap_or_else(|| panic!("{rule_id} start"));
        let rule_end_rel = xml[rule_start..]
            .find("</rule>")
            .unwrap_or_else(|| panic!("{rule_id} end"));
        let rule_end = rule_start + rule_end_rel + "</rule>".len();
        let rule_text = &xml[rule_start..rule_end];
        let mut spans = Vec::new();
        let mut search = 0usize;
        while let Some(rel) = rule_text[search..].find("<antipattern") {
            let start = search + rel;
            let end_rel = rule_text[start..]
                .find("</antipattern>")
                .unwrap_or_else(|| panic!("antipattern end in {rule_id}"));
            let end = start + end_rel + "</antipattern>".len();
            spans.push((start, end));
            search = end;
        }
        let mut out = String::with_capacity(xml.len());
        out.push_str(&xml[..rule_start]);
        let mut cursor = 0usize;
        for (i, &(s, e)) in spans.iter().enumerate() {
            if strip.contains(&i) {
                out.push_str(&rule_text[cursor..s]);
                cursor = e;
            }
        }
        out.push_str(&rule_text[cursor..]);
        out.push_str(&xml[rule_end..]);
        (out, spans.len())
    }

    fn adj_variant_match_count(rule_id: &str, text: &str, strip: &[usize]) -> (usize, usize) {
        let (xml, span_count) = adj_rule_variant_xml(rule_id, strip);
        let path = crate::pl::pl_rules_dir()
            .expect("pl rules dir")
            .join("grammar.xml");
        let rule_set = XmlCompiler::new()
            .compile_file_with_base(xml.as_str(), path.parent())
            .expect("compile variant");
        let engine = trace_engine();
        let pipeline = PolishPipeline::shared();
        let mut count = 0usize;
        for rule in rule_set.rules.iter().filter(|r| r.id == rule_id) {
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                for m in engine.match_rule(rule, &sentence) {
                    eprintln!(
                        "      strip {strip:?}: match off {} len {} sub {:?}",
                        m.offset() + sentence_start,
                        m.length(),
                        rule.sub_id
                    );
                    count += 1;
                }
            }
        }
        (count, span_count)
    }

    #[ignore]
    #[test]
    fn probe_round16_adj_subst_adj_unify_antipattern_bisect() {
        let _release_guard = crate::release_guard();
        let rule_id = "ADJ_SUBST_ADJ_UNIFY";
        let text =
            "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.";
        let (base_count, span_count) = adj_variant_match_count(rule_id, text, &[]);
        eprintln!("== {rule_id}: {span_count} antipattern spans, intact matches: {base_count} ==");
        let (stripped_count, _) =
            adj_variant_match_count(rule_id, text, &(0..span_count).collect::<Vec<_>>());
        eprintln!("== all antipatterns stripped: {stripped_count} matches ==");
        if stripped_count > 0 && base_count == 0 {
            for i in 0..span_count {
                let keep_only = (0..span_count).filter(|j| *j != i).collect::<Vec<_>>();
                let (count, _) = adj_variant_match_count(rule_id, text, &keep_only);
                eprintln!("   antipattern #{i} kept alone (all others stripped): {count}");
            }
        }
        pattern_of(rule_id);
        probe_with_engine_trace(rule_id, text);
    }

    #[test]
    fn regex_crate_rejects_java_octal_and_inline_unicode_flag_negation() {
        let brak_kropki_exception_class = "[?\\.!,:;…\u{201d}\"\\02]";
        assert!(
            regex::Regex::new(brak_kropki_exception_class).is_err(),
            "the BRAK_KROPKI marker exception class must fail the regex crate (octal \\02)"
        );
        let dni_tygodnia_exception = "(?-iu)w.*";
        assert!(
            regex::Regex::new(dni_tygodnia_exception).is_err(),
            "the DNI_TYGODNIA antipattern exceptions must fail the regex crate (u cannot be \\
             disabled on &str regexes)"
        );
    }

    #[test]
    fn pl_disambiguation_carries_95_lemma_only_remove_actions() {
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None)
            .expect("parse disambiguation.xml");
        assert!(parsed.rule_ids().contains(&"K"), "the K rule must parse");
        assert_eq!(
            xml.matches("action=\"remove\"><wd lemma").count(),
            95,
            "the lemma-only remove action count the og-tagger wildcard-pos fix must unlock"
        );
    }

    #[test]
    fn sentence_final_word_carries_sent_end_before_disambiguation() {
        let text = "\u{15a}pieszcie si\u{119} po woli";
        let sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        let tokens = sentence.non_whitespace_tokens();
        let woli = tokens.last().expect("final token");
        assert_eq!(woli.token().token(), "woli");
        assert!(
            woli.is_sentence_end(),
            "the pipeline adds the SENT_END reading before disambiguation, so the PO_WOLI \\
             residue isolates the loss to a disambiguator path that Java guards with \\
             AnalyzedTokenReadings.java:377 / DisambiguationPatternRuleReplacer.java:176"
        );
    }

    #[test]
    fn wladce_residue_precondition_surowce_keeps_exact_case_depr_reading() {
        let text = "Surowce zawieraj\u{105}ce te alkaloidy";
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let dict = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "polish")
            .expect("polish.dict");
        let exact: Vec<String> = dict
            .lookup("Surowce")
            .iter()
            .map(|(lemma, pos)| format!("{lemma}/{pos}"))
            .collect();
        assert!(
            exact
                .iter()
                .any(|e| e.split('/').next().is_some_and(|l| l == "Surowiec")
                    && e.rsplit('/')
                        .next()
                        .is_some_and(|pos| pos.split('+').any(|p| p == "depr:pl:nom:m2"))),
            "the exact-case FSA hit that feeds the WLADCE false positive: {exact:?}"
        );
        let sentence = disambiguated_sentence(text);
        let surowce = sentence
            .non_whitespace_tokens()
            .into_iter()
            .find(|t| t.token().token() == "Surowce")
            .expect("Surowce token");
        assert!(
            surowce
                .readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|p| p == "depr:pl:nom:m2")),
            "the WLADCE fp precondition: the reading survives the whole module pipeline - \\
             the two unify_adj_subst sub-rules that cover the token keep m2 (the pact tag \\
                             'pact:pl:nom.acc.voc:m2.m3.f.n1.n2.p2.p3' satisfies the \".*\\bm2\\b.*\" gender \\
             equivalence), so the Java-side drop happens in shared engine code, not in pl XML"
        );
    }

    #[test]
    fn rymy_residue_compiles_the_match_reference_element_with_no_matchable_source() {
        let rules = compiled_rules();
        let rymy: Vec<&CompiledRule> = rules.rules.iter().filter(|r| r.id == "RYMY").collect();
        assert!(!rymy.is_empty(), "RYMY must compile");
        let sub1 = rymy
            .iter()
            .find(|r| r.sub_id.as_deref() == Some("1"))
            .expect("RYMY sub-rule 1");
        let and_members: Vec<&og_xml::compiler::CompiledPatternToken> = sub1
            .pattern
            .elements
            .iter()
            .filter_map(|e| match e {
                og_xml::compiler::CompiledPatternElement::AndGroup(members) => Some(members.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(
            and_members.len() >= 2,
            "RYMY sub-rule 1 must carry the <and> group: {:?}",
            sub1.pattern.tokens.len()
        );
        assert!(
            and_members.iter().all(|m| m.match_no == Some(0)),
            "both <and> members reference token 0 via <match no=\"0\">"
        );
        assert!(
            and_members.iter().any(|m| m
                .match_no_regexp_replace
                .as_deref()
                .is_some_and(|r| r == ".*$1")
                && m.compiled_match_no_regex.is_some()),
            "the token-level <match no=\"0\" regexp_match=\"...([aeuóiyęą]...)\" \
             regexp_replace=\".*$1\"> transform now compiles into the shared engine \
             (compiled_match_no_regex + match_no_regexp_replace), closing the gap this \
             pin used to document (the in-token <match> references port, 191a205, \
             mirrors PatternTokenMatcher resolveReference -> MatchState toFinalString)"
        );
    }
}
