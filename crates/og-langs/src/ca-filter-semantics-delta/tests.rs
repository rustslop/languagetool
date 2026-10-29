use std::cell::RefCell;

use og_core::{
    AnalyzedToken, AnalyzedTokenReadings, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};

use super::adjust_verb_suggestions::{
    AdjustArgs, accept_rule_match as adjust_verb_accept, get_target_postag,
};
use super::apostophation::{
    all_need_same_apostrophation_ms, get_preposition_and_determiner, needs_apostrophation_ms,
};
use super::ca_adapt::adapt_suggestion_ca;
use super::consumers::{
    any_change_vowel_consonant, find_suggestions_es_verb3_allowed, oblidarse_word_after_suffix,
};
use super::convert_gender_number::{ConvertArgs, accept_rule_match as convert_accept};
use super::en_no_infinitiu::accept_rule_match as en_no_infinitiu_accept;
use super::oblidarse_suggestions::accept_rule_match as oblidarse_accept;
use super::portar_gerundi::accept_rule_match as portar_gerundi_accept;
use super::possessius_redundants::accept_rule_match as possessius_accept;
use super::pronoms_actions::{
    convert_pronouns_for_intransitive_verb, do_add_pronoun_reflexive, do_remove_pronoun_reflexive,
    fix_apostrophes, get_dative_pronoun, get_reflexive_pronoun,
};
use super::pronoms_davant::transform_davant;
use super::que_inici::{accept_rule_match as que_inici_accept, predicts_accent};
use super::simple_replace_lemmas::matched_lemma_and_postag;
use super::suppress_misspelled::is_misspelled;
use super::verb_classifier::is_intransitive;
use super::verb_synthesizer::{VerbSynthesizer, adjust_postag_to_lemma};

struct Spec<'a> {
    text: &'a str,
    readings: &'a [(&'a str, &'a str)],
    chunks: &'a [&'a str],
}

fn spec<'a>(text: &'a str, readings: &'a [(&'a str, &'a str)]) -> Spec<'a> {
    Spec {
        text,
        readings,
        chunks: &[],
    }
}

fn spec_gv<'a>(text: &'a str, readings: &'a [(&'a str, &'a str)]) -> Spec<'a> {
    Spec {
        text,
        readings,
        chunks: &["GV"],
    }
}

fn build(specs: &[Spec]) -> (String, Vec<AnalyzedTokenReadings>) {
    let mut text = String::new();
    let mut tokens = Vec::with_capacity(specs.len() + 1);
    tokens.push(
        AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )
        .with_readings(vec![
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        ]),
    );
    let mut offset = 0usize;
    for s in specs {
        if !text.is_empty() {
            text.push(' ');
            offset += 1;
        }
        let start = offset;
        let end = start + s.text.len();
        offset = end;
        text.push_str(s.text);
        let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(s.text, start, end));
        if !s.chunks.is_empty() {
            atr.set_chunks(s.chunks.iter().map(|c| c.to_string()).collect());
        }
        let readings = s
            .readings
            .iter()
            .map(|(lemma, tag)| {
                AnalyzedToken::new(s.text, start, end)
                    .with_lemma(*lemma)
                    .with_pos_tags(vec![tag.to_string()])
            })
            .collect();
        tokens.push(atr.with_readings(readings));
    }
    (text, tokens)
}

fn refs(tokens: &[AnalyzedTokenReadings]) -> Vec<&AnalyzedTokenReadings> {
    tokens.iter().collect()
}

fn make_match(id: &str, offset: usize, length: usize, context: &str) -> RuleMatch {
    RuleMatch::new(
        "msg".to_string(),
        offset,
        length,
        RuleMatchRule::new(id, "desc"),
        RuleMatchContext::new(context.to_string(), offset.saturating_sub(10), length + 20),
    )
}

fn replacement_values(rm: &RuleMatch) -> Vec<String> {
    rm.replacements()
        .iter()
        .map(|r| r.value().to_string())
        .collect()
}

#[test]
fn needs_apostrophation_ms_follows_the_java_patterns() {
    assert!(needs_apostrophation_ms("avi"), "l'avi");
    assert!(needs_apostrophation_ms("home"), "l'home");
    assert!(needs_apostrophation_ms("ús"));
    assert!(needs_apostrophation_ms("enterramorts"));
    assert!(
        !needs_apostrophation_ms("iola"),
        "h?[ui][aeioàèéóò].+ blocks it"
    );
    assert!(!needs_apostrophation_ms("uixó"));
    assert!(!needs_apostrophation_ms("hip-hop"), "hac aspirada");
    assert!(!needs_apostrophation_ms("Utah"), "hac aspirada");
    assert!(!needs_apostrophation_ms("haiku"), "hac aspirada");
    assert!(!needs_apostrophation_ms("hàndicap"), "hac aspirada");
    assert!(
        needs_apostrophation_ms("ham"),
        "plain h+vowel is not in the hac list"
    );
}

#[test]
fn all_need_same_apostrophation_ms_compares_lemmas_against_the_first_word() {
    let sec = AnalyzedToken::new("sec", 0, 3).with_lemma("sec");
    let hip = AnalyzedToken::new("hip-hop", 0, 7).with_lemma("hip-hop");
    assert!(all_need_same_apostrophation_ms("avi", &[]));
    assert!(all_need_same_apostrophation_ms("gos", &[&sec]));
    assert!(!all_need_same_apostrophation_ms("avi", &[&sec]));
    assert!(!all_need_same_apostrophation_ms("avi", &[&sec, &hip]));
    assert!(!all_need_same_apostrophation_ms("home", &[&sec]));
}

#[test]
fn get_preposition_and_determiner_pins_the_prep_det_table() {
    assert_eq!(get_preposition_and_determiner("avi", "MS", ""), Some("l'"));
    assert_eq!(get_preposition_and_determiner("home", "MS", ""), Some("l'"));
    assert_eq!(
        get_preposition_and_determiner("hip-hop", "MS", ""),
        Some("el ")
    );
    assert_eq!(get_preposition_and_determiner("gat", "MS", ""), Some("el "));
    assert_eq!(
        get_preposition_and_determiner("casa", "FS", ""),
        Some("la ")
    );
    assert_eq!(get_preposition_and_determiner("ira", "FS", ""), Some("la "));
    assert_eq!(
        get_preposition_and_determiner("avi", "MS", "de"),
        Some("de l'")
    );
    assert_eq!(
        get_preposition_and_determiner("hip-hop", "MS", "de"),
        Some("del ")
    );
    assert_eq!(
        get_preposition_and_determiner("casa", "FS", "a"),
        Some("a la ")
    );
    assert_eq!(get_preposition_and_determiner("avi", "CN", ""), Some("l'"));
    assert_eq!(
        get_preposition_and_determiner("cases", "MP", "per"),
        Some("pels ")
    );
}

#[test]
fn transform_davant_uses_needs_apostrophation_ms() {
    assert_eq!(transform_davant("en", "anar"), "n'");
    assert_eq!(transform_davant("en", "menjar"), "en ");
    assert_eq!(transform_davant("en", "iogurt"), "en ");
    assert_eq!(transform_davant("en", "hip-hop"), "en ");
    assert_eq!(transform_davant("es", "seguir"), "se ");
    assert_eq!(transform_davant("es", "cendre"), "se ");
    assert_eq!(transform_davant("es", "passar"), "es ");
}

#[test]
fn verb_classifier_loads_the_java_table() {
    assert!(is_intransitive("ser"));
    assert!(is_intransitive("agradar"));
    assert!(!is_intransitive("passar"));
    assert!(!is_intransitive("dir"));
    assert!(!is_intransitive("fer"));
    assert!(!is_intransitive("verb inexistent"));
}

#[test]
fn verb_synthesizer_set_indexes_guard_keeps_out_of_range_starts_undefined() {
    let (_t, tokens) = build(&[
        spec("ha", &[("haver", "VAIP3S0")]),
        spec("passat", &[("passar", "VMP00SM")]),
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    assert!(VerbSynthesizer::new(&r, -1).is_undefined());
    assert!(VerbSynthesizer::new(&r, r.len() as isize).is_undefined());
    assert!(VerbSynthesizer::new(&r, r.len() as isize + 4).is_undefined());
    let forward = VerbSynthesizer::new(&r, 2);
    assert!(!forward.is_undefined());
    assert_eq!(forward.first_verb_index(), 2);
    assert_eq!(forward.last_verb_index(), 2);
}

#[test]
fn verb_synthesizer_finds_multitoken_groups_and_pronouns() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec("ho", &[("ho", "PP3CN000")]),
        spec_gv("ha", &[("haver", "VAIP3S0")]),
        spec_gv("passat", &[("passar", "VMP00SM")]),
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    let vs = VerbSynthesizer::new(&r, 2);
    assert!(!vs.is_undefined());
    assert_eq!(vs.first_verb_index(), 3);
    assert_eq!(vs.last_verb_index(), 4);
    assert_eq!(vs.num_pronouns_before(), 1);
    assert_eq!(vs.get_pronouns_str_before(), "ho");
    assert_eq!(vs.get_verb_str(), "ha passat");
    assert_eq!(vs.get_whole_original_str(), "ho ha passat");
    assert_eq!(vs.get_first_verb_is_postag().as_deref(), Some("VAIP3S0"));
    assert_eq!(vs.get_first_verb_persona_number(), "3S");
}

#[test]
fn verb_synthesizer_last_verb_null_falls_back_to_the_first_verb_reading() {
    let (_t, tokens) = build(&[
        spec_gv("he", &[("haver", "VAIP1S0")]),
        Spec {
            text: "anat",
            readings: &[("anar", "_GV_")],
            chunks: &["GV"],
        },
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    let vs = VerbSynthesizer::new(&r, 1);
    assert_eq!((vs.first_verb_index(), vs.last_verb_index()), (1, 2));
    let calls: RefCell<Vec<(String, String)>> = RefCell::new(Vec::new());
    let synth = |lemma: &str, pattern: &str| {
        calls
            .borrow_mut()
            .push((lemma.to_string(), pattern.to_string()));
        match (lemma, pattern) {
            ("haver", "VAIP1S0") => vec!["he".to_string()],
            ("fer", "VAIP1S0") => vec!["vaig".to_string()],
            _ => vec![],
        }
    };
    let adapt = |s: &str| s.to_string();
    let out = vs.synthesize("fer", "VMIP1S0", &synth, &adapt);
    assert_eq!(out, "he vaig");
    assert_eq!(
        calls.borrow().last().unwrap(),
        &("fer".to_string(), "VAIP1S0".to_string())
    );
}

#[test]
fn adjust_postag_follows_the_java_lemma_swaps() {
    assert_eq!(adjust_postag_to_lemma("haver", "VMIP3S0"), "VAIP3S0");
    assert_eq!(adjust_postag_to_lemma("ser", "VMIP3S0"), "VSIP3S0");
    assert_eq!(adjust_postag_to_lemma("fer", "VMIP3S0"), "VMIP3S0");
}

#[test]
fn que_inici_voler_dir_takes_the_accent() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec_gv("vol", &[("voler", "VMIP3S0")]),
        spec_gv("dir", &[("dir", "VMN0000")]),
        spec("això", &[("això", "PD3CS000")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), false), Some(true));
    assert!(que_inici_accept(
        "QUE_INICIAL_AMBACCENT_VERB",
        Some("volerDir"),
        &refs(&tokens)
    ));
    assert!(!que_inici_accept(
        "QUE_INICIAL_SENSEACCENT_VERB",
        Some("volerDir"),
        &refs(&tokens)
    ));
}

#[test]
fn que_inici_voler_dir_with_indefinite_object_stays_unaccented() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec_gv("vols", &[("voler", "VMIP2S0")]),
        spec_gv("dir", &[("dir", "VMN0000")]),
        spec("res", &[("res", "PI0CS000")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), false), Some(false));
    assert!(!que_inici_accept(
        "QUE_INICIAL_AMBACCENT_VERB",
        Some("volerDir"),
        &refs(&tokens)
    ));
}

#[test]
fn que_inici_si_no_tail_uses_the_length_minus_three_index() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec_gv("vol", &[("voler", "VMIP3S0")]),
        spec_gv("dir", &[("dir", "VMN0000")]),
        spec("això", &[("això", "PD3CS000")]),
        spec(",", &[]),
        spec("no", &[]),
        spec("?", &[]),
    ]);
    assert_eq!(
        predicts_accent(&refs(&tokens), false),
        Some(false),
        "', no?' tail flips voler dir to the si/no reading via tokens[len-3]"
    );
}

#[test]
fn que_inici_passar_subject_keeps_the_accent() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec_gv("ha", &[("haver", "VAIP3S0")]),
        spec_gv("passat", &[("passar", "VMP00SM")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), false), Some(true));
    assert!(que_inici_accept(
        "QUE_INICIAL_AMBACCENT_VERB",
        Some("completive"),
        &refs(&tokens)
    ));
}

#[test]
fn que_inici_saturated_completive_drops_the_accent() {
    let (_t, tokens) = build(&[
        spec("Que", &[]),
        spec("creus", &[("creure", "VMIP2S0")]),
        spec("que", &[("que", "CS")]),
        spec("té", &[("tenir", "VMIP3S0")]),
        spec("raó", &[("raó", "NCFS000")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), true), Some(false));
}

#[test]
fn que_inici_completive_gap_keeps_the_accent() {
    let (_t, tokens) = build(&[
        spec("Què", &[]),
        spec("creus", &[("creure", "VMIP2S0")]),
        spec("que", &[("que", "CS")]),
        spec_gv("ha", &[("haver", "VAIP3S0")]),
        spec_gv("passat", &[("passar", "VMP00SM")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), true), Some(true));
}

#[test]
fn que_inici_without_que_is_undecidable_and_keeps_the_match() {
    let (_t, tokens) = build(&[
        spec("Com", &[]),
        spec("anem", &[("anar", "VMIP1P0")]),
        spec("?", &[]),
    ]);
    assert_eq!(predicts_accent(&refs(&tokens), false), None);
    assert!(que_inici_accept(
        "QUE_INICIAL_AMBACCENT_VERB",
        None,
        &refs(&tokens)
    ));
}

#[test]
fn lemma_replace_skips_sentence_end_only_readings() {
    let mut wrong: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    wrong.insert("dia".to_string(), vec!["dia".to_string()]);
    let readings = vec![
        AnalyzedToken::new("?", 0, 1)
            .with_lemma("?")
            .with_pos_tags(vec!["SENT_END".to_string()]),
        AnalyzedToken::new("dia", 0, 3)
            .with_lemma("dia")
            .with_pos_tags(vec!["NCMS000".to_string()]),
    ];
    let matched = matched_lemma_and_postag(&readings, &wrong);
    assert_eq!(matched, Some(("dia", Some("NCMS000"))));
    let only_end = vec![readings[0].clone()];
    assert_eq!(matched_lemma_and_postag(&only_end, &wrong), None);
    let mut end_key = std::collections::HashMap::new();
    end_key.insert("?".to_string(), vec!["x".to_string()]);
    assert_eq!(matched_lemma_and_postag(&only_end, &end_key), None);
    let no_postag = vec![AnalyzedToken::new("dia", 0, 3).with_lemma("dia")];
    assert_eq!(
        matched_lemma_and_postag(&no_postag, &wrong),
        Some(("dia", None)),
        "a lemma reading without a POSTag is not a SENT_END reading and still matches"
    );
}

#[test]
fn en_no_infinitiu_clamps_the_start_pos_guard() {
    let (text, tokens) = build(&[
        spec("Estudiar", &[("estudiar", "VMN0000")]),
        spec("així", &[("així", "RG")]),
        spec("no", &[("no", "RN")]),
        spec("aprèn", &[("aprendre", "VMIP3S0")]),
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let rm = make_match("EN_NO_INFINITIU", start, "Estudiar".len(), &text);
    let synth = |lemma: &str, pattern: &str| {
        assert_eq!((lemma, pattern), ("estudiar", "VMIP3S0"));
        vec!["estudia".to_string()]
    };
    let adapt = |s: &str| s.to_string();
    let out = en_no_infinitiu_accept(
        &rm,
        &text,
        start,
        tokens[1].token().end(),
        &r,
        &synth,
        &adapt,
    )
    .expect("kept");
    assert_eq!(
        replacement_values(&out),
        vec!["Com que no estudia".to_string()]
    );
    assert_eq!(out.offset(), tokens[0].token().start());
}

#[test]
fn portar_gerundi_synthesizes_the_java_suggestions() {
    let (text, tokens) = build(&[
        spec("Porto", &[("portar", "VMIP1S0")]),
        spec("fent", &[("fer", "VMG0000")]),
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let rm = make_match("PORTAR_GERUNDI", start, "Porto fent".len(), &text);
    let synth = |lemma: &str, pattern: &str| match (lemma, pattern) {
        ("haver", "VAIP1S0") => vec!["he".to_string()],
        ("fer", "V.P..SM.") => vec!["fet".to_string()],
        ("fer", "V.IP1S0") => vec!["faig".to_string()],
        _ => vec![],
    };
    let out = portar_gerundi_accept(&rm, start, &r, "", &synth).expect("kept");
    assert_eq!(
        replacement_values(&out),
        vec!["He fet".to_string(), "Faig".to_string()]
    );
}

#[test]
fn portar_gerundi_empty_synthesis_drops_the_match() {
    let (text, tokens) = build(&[
        spec("Porto", &[("portar", "VMIP1S0")]),
        spec("fent", &[("fer", "VMG0000")]),
        spec("?", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let rm = make_match("PORTAR_GERUNDI", start, "Porto fent".len(), &text);
    let synth = |_lemma: &str, _pattern: &str| Vec::<String>::new();
    assert!(portar_gerundi_accept(&rm, start, &r, "", &synth).is_none());
}

#[test]
fn suppress_misspelled_takes_the_raw_sentence_fast_path_for_single_tokens() {
    let full_calls = RefCell::new(0usize);
    let raw = |s: &str| s == "menjar";
    let full = |_s: &str| {
        *full_calls.borrow_mut() += 1;
        super::suppress_misspelled::FullAnalysis {
            has_incorrect_verb_chunk: false,
            speller_flags: false,
        }
    };
    assert!(is_misspelled("menjar", &raw, &full));
    assert_eq!(
        *full_calls.borrow(),
        0,
        "fast path avoids the disambiguator"
    );
    assert!(!is_misspelled("parlar", &raw, &full));
    assert_eq!(
        *full_calls.borrow(),
        1,
        "single token that passes raw goes full"
    );
    *full_calls.borrow_mut() = 0;
    assert!(!is_misspelled("menjar ràpid", &raw, &full));
    assert_eq!(
        *full_calls.borrow(),
        1,
        "multi-token suggestions skip the fast path per CatalanSuppressMisspelledSuggestionsFilter:49"
    );
}

#[test]
fn possessius_redundants_builds_the_l_apostrophe_suggestion() {
    let (text, tokens) = build(&[
        spec("El", &[("el", "DA0MS0")]),
        spec("metge", &[("metge", "NCMS000")]),
        spec("mira", &[("mirar", "VMIP3S0")]),
        spec("el", &[("el", "DA0MS0")]),
        spec("teu", &[("teu", "PX2MS0S0")]),
        spec("ull", &[("ull", "NCMS000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[3].token().start();
    let mut rm = make_match("POSSESSIUS_PARTS_COS", start, 13, &text);
    assert_eq!(possessius_accept(&mut rm, 3, &r), Some(()));
    assert_eq!(
        replacement_values(&rm),
        vec!["et mira l'ull".to_string()],
        "grammar.xml POSSESSIUS_PARTS_COS: correction 'et mira l'ull'"
    );
    assert_eq!(rm.offset(), tokens[3].token().start());
}

#[test]
fn possessius_redundants_with_pronoun_deletes_the_possessive() {
    let (text, tokens) = build(&[
        spec("Em", &[("em", "PP1CS000")]),
        spec("toquí", &[("tocar", "VMIS1S0")]),
        spec("el", &[("el", "DA0MS0")]),
        spec("meu", &[("meu", "PX1MS0S0")]),
        spec("braç", &[("braç", "NCMS000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let mut rm = make_match("POSSESSIUS_PARTS_COS", start, 7, &text);
    assert_eq!(possessius_accept(&mut rm, 2, &r), Some(()));
    assert_eq!(replacement_values(&rm), vec![String::new()]);
    assert_eq!(rm.offset(), tokens[4].token().start());
    assert_eq!(
        rm.length(),
        tokens[4].token().end() - tokens[4].token().start()
    );
}

#[test]
fn convert_gender_number_rebuilds_the_determiner_with_the_synth() {
    let (text, tokens) = build(&[
        spec("el", &[("el", "DA0MS0")]),
        spec("gat", &[("gat", "NCMS000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let rm = make_match("CONVERT_GN", start, 3, &text);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |lemma: &str, pattern: &str| {
        if lemma == "gat" && pattern == "NC[MC][SN]000" {
            vec!["gos".to_string()]
        } else {
            vec![]
        }
    };
    let out = convert_accept(
        &rm,
        &text,
        start,
        &r,
        &ConvertArgs {
            gender: "",
            number: "",
            lemma_select: "N.*",
            new_lemma: "",
            keep_original: false,
        },
        &tagger,
        &synth,
    )
    .expect("kept");
    assert_eq!(replacement_values(&out), vec!["el gos".to_string()]);
    assert_eq!(out.offset(), tokens[1].token().start());
}

#[test]
fn convert_gender_number_captures_de_when_apostrophation_disagrees() {
    let (text, tokens) = build(&[
        spec("d'", &[("de", "SPS00")]),
        spec("gos", &[("gos", "NCMS000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let rm = make_match("CONVERT_GN", start, 3, &text);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |_lemma: &str, _pattern: &str| Vec::<String>::new();
    let out = convert_accept(
        &rm,
        &text,
        start,
        &r,
        &ConvertArgs {
            gender: "",
            number: "",
            lemma_select: "N.*",
            new_lemma: "àcid",
            keep_original: true,
        },
        &tagger,
        &synth,
    )
    .expect("kept");
    assert_eq!(replacement_values(&out), vec!["de gos".to_string()]);
    assert_eq!(out.offset(), tokens[1].token().start());
}

#[test]
fn convert_gender_number_apostrophates_de_before_vowel_initial_suggestions() {
    let (text, tokens) = build(&[
        spec("de", &[("de", "SPS00")]),
        spec("àcid", &[("àcid", "AQ0MS0")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let rm = make_match("CONVERT_GN", start, 4, &text);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |_lemma: &str, _pattern: &str| Vec::<String>::new();
    let out = convert_accept(
        &rm,
        &text,
        start,
        &r,
        &ConvertArgs {
            gender: "",
            number: "",
            lemma_select: "A.*",
            new_lemma: "sec",
            keep_original: true,
        },
        &tagger,
        &synth,
    )
    .expect("kept");
    assert_eq!(
        replacement_values(&out),
        vec!["d'àcid".to_string()],
        "de -> d' before a vowel-initial builder word"
    );
}

#[test]
fn consumers_pin_the_changed_predicates() {
    assert!(any_change_vowel_consonant("ha", &["menjava".to_string()]));
    assert!(!any_change_vowel_consonant("ha", &["havia".to_string()]));
    assert!(!find_suggestions_es_verb3_allowed("ha"));
    assert!(find_suggestions_es_verb3_allowed("menjava"));
    assert_eq!(oblidarse_word_after_suffix("ull"), (" d'", 1));
    assert_eq!(oblidarse_word_after_suffix("braç"), (" de", 0));
}

#[test]
fn pronoms_actions_pin_the_java_do_family() {
    assert_eq!(get_reflexive_pronoun("3S"), "es");
    assert_eq!(get_reflexive_pronoun("1P"), "ens");
    assert_eq!(get_reflexive_pronoun("3C"), "");
    assert_eq!(get_dative_pronoun("3S"), "li");
    assert_eq!(get_dative_pronoun("3P"), "els");
    assert_eq!(get_dative_pronoun("3C"), "li");
    assert_eq!(
        do_add_pronoun_reflexive("", "menja", "3S", false),
        "es menja"
    );
    assert_eq!(do_remove_pronoun_reflexive("em", "menjo", false), "menjo");
    assert_eq!(
        do_remove_pronoun_reflexive("em", "menjo", true),
        "menjo",
        "no pronouns survive the strip, darrere stays empty"
    );
    assert_eq!(
        convert_pronouns_for_intransitive_verb("se'l menja"),
        "se li menja"
    );
    assert_eq!(fix_apostrophes("em oblidat"), "m'oblidat");
    assert_eq!(fix_apostrophes("d'viatge"), "de viatge");
    assert_eq!(fix_apostrophes("t'comença"), "et comença");
}

#[test]
fn adapt_suggestion_ca_pins_the_catalan_rewrites() {
    assert_eq!(adapt_suggestion_ca("de el gos", ""), "del gos");
    assert_eq!(adapt_suggestion_ca("per els gossos", ""), "pels gossos");
    assert_eq!(adapt_suggestion_ca("l' avi", ""), "l'avi");
    assert_eq!(adapt_suggestion_ca("t'comença", ""), "et comença");
    assert_eq!(adapt_suggestion_ca("la avi", ""), "l'avi");
    assert_eq!(
        adapt_suggestion_ca("la una cosa", ""),
        "la una cosa",
        "CA_APOSTROPHES4 vowel class has no plain u"
    );
    assert_eq!(adapt_suggestion_ca("d'viatge", ""), "de viatge");
    assert_eq!(adapt_suggestion_ca("de ha perdut", ""), "d'ha perdut");
    assert_eq!(adapt_suggestion_ca("a ls coses", ""), "als coses");
    assert_eq!(
        adapt_suggestion_ca("a l'avi", ""),
        "a l'avi",
        "CA_REMOVE_SPACES lookahead keeps l' untouched"
    );
    assert_eq!(adapt_suggestion_ca("menjava", "menjava"), "menjava");
    assert_eq!(adapt_suggestion_ca("De el Gos", "El gos"), "Del gos");
}

#[test]
fn get_target_postag_sorts_like_the_java_comparator() {
    assert_eq!(get_target_postag(&[], ""), "");
    assert_eq!(
        get_target_postag(&["VMIP1S00".to_string(), "VMIP3S00".to_string()], ""),
        "VMIP3S00"
    );
    assert_eq!(
        get_target_postag(&["VMSP3S00".to_string(), "VMIP3S00".to_string()], ""),
        "VMIP3S00",
        "indicative outranks subjunctive"
    );
    assert_eq!(
        get_target_postag(&["VMIS3S00".to_string(), "VMIP2P00".to_string()], ""),
        "VMIP2P00",
        "feu present beats passat"
    );
}

#[test]
fn adjust_verb_suggestions_prefixes_de_when_apostrophation_flips() {
    let (text, tokens) = build(&[
        spec("De", &[("de", "SPS00")]),
        spec_gv("menjava", &[("menjava", "VMII3S00")]),
        spec("pomes", &[("pomes", "NCFP000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let rm = make_match("VERB_SUGG", start, "menjava".len(), &text)
        .with_replacements(vec![SuggestedReplacement::new("ha perdut")]);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |lemma: &str, postag: &str| {
        assert_eq!((lemma, postag), ("ha", "VMII3S00"));
        vec!["ha".to_string()]
    };
    let out = adjust_verb_accept(
        &rm,
        &text,
        start,
        tokens[2].token().end(),
        &AdjustArgs {
            actions: "",
            number_from_next_words: false,
            force_number: "",
        },
        &r,
        &tagger,
        &synth,
    )
    .expect("kept");
    assert_eq!(replacement_values(&out), vec!["D'ha perdut".to_string()]);
    assert_eq!(out.offset(), tokens[1].token().start());
    assert_eq!(
        out.length(),
        tokens[2].token().end() - tokens[1].token().start()
    );
}

#[test]
fn adjust_verb_suggestions_synthesizes_the_reflexive_suffix_action() {
    let (text, tokens) = build(&[
        spec("Se", &[("se", "PP3CS000")]),
        spec_gv("menja", &[("menja", "VMIP3S00")]),
        spec("pomes", &[("pomes", "NCFP000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[2].token().start();
    let rm = make_match("VERB_SUGG", start, "menja".len(), &text)
        .with_replacements(vec![SuggestedReplacement::new("menjar-se")]);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |lemma: &str, postag: &str| {
        assert_eq!((lemma, postag), ("menjar", "VMIP3S00"));
        vec!["menja".to_string()]
    };
    let out = adjust_verb_accept(
        &rm,
        &text,
        start,
        tokens[2].token().end(),
        &AdjustArgs {
            actions: "",
            number_from_next_words: false,
            force_number: "",
        },
        &r,
        &tagger,
        &synth,
    )
    .expect("kept");
    assert_eq!(replacement_values(&out), vec!["es menja".to_string()]);
    assert_eq!(out.offset(), tokens[2].token().start());
}

#[test]
fn adjust_verb_suggestions_drops_the_match_without_postags() {
    let (text, tokens) = build(&[spec_gv("menjava", &[]), spec(".", &[])]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let rm = make_match("VERB_SUGG", start, "menjava".len(), &text)
        .with_replacements(vec![SuggestedReplacement::new("menjava")]);
    let tagger = |_w: &str| Vec::<AnalyzedTokenReadings>::new();
    let synth = |_lemma: &str, _postag: &str| Vec::<String>::new();
    assert!(
        adjust_verb_accept(
            &rm,
            &text,
            start,
            tokens[1].token().end(),
            &AdjustArgs {
                actions: "",
                number_from_next_words: false,
                force_number: "",
            },
            &r,
            &tagger,
            &synth,
        )
        .is_none()
    );
}

#[test]
fn oblidarse_builds_the_reflexive_and_en_suggestions() {
    let (text, tokens) = build(&[
        spec("se", &[("se", "PP3CS000")]),
        spec("m'", &[("m'", "PP1CS000")]),
        spec_gv("ha", &[("haver", "VAIP3S00")]),
        spec_gv("oblidat", &[("oblidar", "VMP00SM")]),
        spec("de", &[("de", "SPS00")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let mut rm = make_match("OBLIDARSE", start, 16, &text)
        .with_replacements(vec![SuggestedReplacement::new("me n'he oblidat")]);
    rm.set_message("oblidar o passar");
    let synth = |lemma: &str, postag: &str| {
        assert_eq!((lemma, postag), ("haver", "VAIP1S00"));
        vec!["he".to_string()]
    };
    let out = oblidarse_accept(&rm, start, &r, &synth).expect("kept");
    assert_eq!(
        replacement_values(&out),
        vec!["m'he oblidat".to_string(), "me n'he oblidat".to_string()]
    );
    assert_eq!(out.offset(), tokens[1].token().start());
    assert_eq!(
        out.length(),
        tokens[4].token().end() - tokens[1].token().start()
    );
    assert_eq!(out.message(), "oblidar o descuidar");
}

#[test]
fn oblidarse_apostrophates_the_word_after_and_extends_the_match() {
    let (text, tokens) = build(&[
        spec("se", &[("se", "PP3CS000")]),
        spec("m'", &[("m'", "PP1CS000")]),
        spec_gv("ha", &[("haver", "VAIP3S00")]),
        spec_gv("oblidat", &[("oblidar", "VMP00SM")]),
        spec("ull", &[("ull", "NCMS000")]),
        spec(".", &[]),
    ]);
    let r = refs(&tokens);
    let start = tokens[1].token().start();
    let rm = make_match("OBLIDARSE", start, 16, &text)
        .with_replacements(vec![SuggestedReplacement::new("x")]);
    let synth = |_lemma: &str, _postag: &str| vec!["he".to_string()];
    let out = oblidarse_accept(&rm, start, &r, &synth).expect("kept");
    assert_eq!(
        replacement_values(&out),
        vec!["m'he oblidat d'".to_string(), "x ".to_string()],
        "Java appends just \" d'\"; the word after stays in the text covered by the +1 extension"
    );
    assert_eq!(
        out.length(),
        tokens[4].token().end() - tokens[1].token().start() + 1
    );
}
