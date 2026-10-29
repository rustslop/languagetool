use std::collections::HashSet;
use std::sync::Arc;

use og_core::Category;
use og_core::rule_match::{RuleMatch, RuleMatchContext, RuleMatchRule, SuggestedReplacement};

fn ca_engine() -> Arc<crate::engine::LanguageEngine> {
    crate::engine::LanguageEngine::shared_for_code("ca")
        .expect("Catalan engine must be registered via for_code")
}

const VALENCIAN_ZERO_CHECK_SENTENCES: [&str; 5] = [
    "Cal usar m\u{e8}todes d'an\u{e0}lisi adequats.",
    "Tant de bo s'imposen el di\u{e0}leg i la diplom\u{e0}cia",
    "Persones humanes",
    "Estos ganivets no corresponen amb estes forquilles.",
    "Aquests ganivets no corresponen amb aquestes forquilles.",
];

const ACCEPTED_DICT_DEPENDENT_IDS: [&str; 3] = [
    "CA_SPLIT_WORDS",
    "CA_MULTITOKEN_SPELLING_TWO",
    "APOSTROFACIO_OPCIONAL",
];

#[test]
fn valencian_variant_zero_match_checks_allow_only_dict_dependent_residue() {
    let _release_guard = crate::release_guard();
    let engine = ca_engine();
    let accepted: HashSet<&str> = ACCEPTED_DICT_DEPENDENT_IDS.into_iter().collect();
    for text in VALENCIAN_ZERO_CHECK_SENTENCES {
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        let residue: Vec<&&str> = ids.iter().filter(|id| !accepted.contains(**id)).collect();
        assert!(
            residue.is_empty(),
            "JLanguageToolTest.testValencianVariant (4cff7008) expects zero matches on '{text}'; \
             every match must be the dict-dependent residue (missing ca-ES.dict), got {residue:?} \
             within {ids:?}"
        );
    }
}

#[test]
fn engine_wires_the_refreshed_grammar_for_the_zero_checks() {
    let _release_guard = crate::release_guard();
    let engine = ca_engine();
    let ids = engine.rule_ids();
    for id in ["TAT_TANT", "D_E_DE", "VEN_BEN", "MAJOR_A", "CRISTALLINA"] {
        assert!(
            ids.contains(&id),
            "grammar.xml must be live-loaded into the engine, otherwise the zero-match checks \
             above are vacuous"
        );
    }
    assert!(
        !ids.contains(&"PHRASE_REPETITION"),
        "the removed rulegroup must not be registered"
    );
}

#[test]
fn vens_de_accent_fix_now_replaces_the_final_suggestion() {
    let rule = RuleMatchRule::new("X", "desc").with_category(Category::new("CAT", "Cat"));
    let m = RuleMatch::new("msg", 0, 4, rule, RuleMatchContext::new("v\u{e9}ns", 0, 4))
        .with_replacements(vec![SuggestedReplacement::new("v\u{e9}ns")]);
    let out = crate::ca_post_overlap_pipeline::catalan_filter_rule_matches(
        vec![m],
        "v\u{e9}ns",
        &HashSet::new(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].replacements()[0].value(),
        "vens",
        "Catalan.java:653 (4cff7008) turns the previously no-op v\u{e9}ns\u{2192}v\u{e9}ns \
         replace into a real de-accent when DIACRITICS_TRADITIONAL_RULES is off"
    );
}
