use super::*;
use og_core::rule_match::{RuleMatch, RuleMatchContext, RuleMatchRule};
use og_core::{Category, SuggestedReplacement};
use std::collections::HashSet;

fn make(id: &str, from: usize, to: usize, replacements: &[&str]) -> RuleMatch {
    let rule = RuleMatchRule::new(id, "desc").with_category(Category::new("CAT", "Cat"));
    RuleMatch::new(
        "msg",
        from,
        to - from,
        rule,
        RuleMatchContext::new("ctx", from, to - from),
    )
    .with_replacements(
        replacements
            .iter()
            .map(|r| SuggestedReplacement::new(*r))
            .collect(),
    )
}

fn make_sub(id: &str, sub: &str, from: usize, to: usize, replacements: &[&str]) -> RuleMatch {
    let rule = RuleMatchRule::new(id, "desc").with_sub_id(sub);
    RuleMatch::new(
        "msg",
        from,
        to - from,
        rule,
        RuleMatchContext::new("ctx", from, to - from),
    )
    .with_replacements(
        replacements
            .iter()
            .map(|r| SuggestedReplacement::new(*r))
            .collect(),
    )
}

fn make_with_sentence(
    id: &str,
    from: usize,
    to: usize,
    replacements: &[&str],
    sentence: &str,
) -> RuleMatch {
    make(id, from, to, replacements).with_sentence(sentence)
}

fn enabled(rules: &[&str]) -> HashSet<String> {
    rules.iter().map(|r| r.to_string()).collect()
}

fn values(m: &RuleMatch) -> Vec<&str> {
    m.replacements().iter().map(|r| r.value()).collect()
}

#[test]
fn after_overlapping_trims_then_re_sorts_by_new_offsets() {
    let text = "aaaa xx bb c";
    let big = make("R1", 0, 10, &["aaaa yy bb"]);
    let inner = make("R2", 1, 2, &["q"]);
    let out = catalan_filter_rule_matches_after_overlapping(vec![big, inner], text);
    assert_eq!(
        out.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
        vec!["R2", "R1"],
        "the start trim moved R1 from offset 0 to 5, past R2 at 1"
    );
    assert_eq!(out[0].offset(), 1);
    assert_eq!(out[1].offset(), 5);
    assert_eq!(out[1].length(), 2);
    assert_eq!(values(&out[1]), vec!["yy"]);
}

#[test]
fn after_overlapping_keeps_untrimmable_matches_verbatim() {
    let text = "aa bb";
    let m = make("R", 0, 5, &["xx bb", "yy zz"]);
    let out = catalan_filter_rule_matches_after_overlapping(vec![m.clone()], text);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].offset(), m.offset());
    assert_eq!(out[0].length(), m.length());
}

#[test]
fn no_change_skip_drops_identity_single_suggestion() {
    let text = "abc def";
    let identity = make("SOME_RULE", 0, 3, &["abc"]);
    let different = make("OTHER_RULE", 4, 7, &["deg"]);
    let out = catalan_filter_rule_matches(vec![identity, different], &text, &enabled(&[]));
    assert_eq!(
        out.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
        vec!["OTHER_RULE"]
    );
}

#[test]
fn no_change_skip_spares_morfologik_and_multi_suggestion_matches() {
    let text = "abc def";
    let morfo = make("MORFOLOGIK_RULE_CA_ES", 0, 3, &["abc"]);
    let multi = make("MULTI", 4, 7, &["deg", "abc"]);
    let out = catalan_filter_rule_matches(vec![morfo, multi], &text, &enabled(&[]));
    assert_eq!(out.len(), 2);
    assert_eq!(values(&out[1]), vec!["deg", "abc"]);
}

#[test]
fn ignore_proper_nouns_after_morfologik_at_same_position_removes_both() {
    let text = "abc def";
    let morfo = make("MORFOLOGIK_RULE_CA_ES", 0, 3, &["abd"]);
    let ignore = make("IGNORE_PROPER_NOUNS", 0, 3, &[]);
    let out = catalan_filter_rule_matches(vec![morfo, ignore], &text, &enabled(&[]));
    assert!(out.is_empty());
}

#[test]
fn morfologik_after_ignore_proper_nouns_at_same_position_removes_both() {
    let text = "abc def";
    let ignore = make("IGNORE_PROPER_NOUNS", 0, 3, &[]);
    let morfo = make("MORFOLOGIK_RULE_CA_ES", 0, 3, &["abd"]);
    let out = catalan_filter_rule_matches(vec![ignore, morfo], &text, &enabled(&[]));
    assert!(out.is_empty());
}

#[test]
fn ignore_proper_nouns_alone_never_reaches_output() {
    let text = "abc def";
    let ignore = make("IGNORE_PROPER_NOUNS", 0, 3, &[]);
    let other = make("SOME_RULE", 4, 7, &["deg"]);
    let out = catalan_filter_rule_matches(vec![ignore, other], &text, &enabled(&[]));
    assert_eq!(
        out.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
        vec!["SOME_RULE"]
    );
}

#[test]
fn morfologik_at_ignored_position_is_dropped_once() {
    let text = "abc def abc";
    let ignore = make("IGNORE_PROPER_NOUNS", 0, 3, &[]);
    let morfo = make("MORFOLOGIK_RULE_CA_ES", 0, 3, &["abd"]);
    let kept = make("MORFOLOGIK_RULE_CA_ES", 8, 11, &["abd"]);
    let out = catalan_filter_rule_matches(vec![ignore, morfo, kept], &text, &enabled(&[]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].offset(), 8);
}

#[test]
fn falta_3_dropped_when_next_match_is_close_and_sentence_scoped() {
    let text = "aaa bbb ccc ddd eee fff";
    let falta = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "3", 0, 3, &["zzz"]);
    let next = make_with_sentence("OTHER", 10, 13, &["ddd"], "aaa bbb ccc ddd eee fff");
    let out = catalan_filter_rule_matches(vec![falta, next], &text, &enabled(&[]));
    assert_eq!(
        out.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
        vec!["OTHER"],
        "gap 10 - 3 = 7 < 20 drops the [3] subrule match"
    );
}

#[test]
fn falta_4_kept_when_next_match_is_20_or_more_away() {
    let text = "aaaaaaaaaaaaaaaaaaaaaa bbbb";
    let falta = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "4", 0, 3, &["zzz"]);
    let next = make_with_sentence("OTHER", 23, 27, &["cccc"], "aaaaaaaaaaaaaaaaaaaaaa bbbb");
    let out = catalan_filter_rule_matches(vec![falta, next], &text, &enabled(&[]));
    assert_eq!(out.len(), 2, "gap 23 - 3 = 20 is not < 20");
}

#[test]
fn falta_3_kept_when_next_is_subrule_5_or_lacks_a_sentence() {
    let text = "aaa bbb ccc";
    let falta = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "3", 0, 3, &["zzz"]);
    let next5 = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "5", 4, 7, &["yyy"]);
    let out = catalan_filter_rule_matches(vec![falta.clone(), next5], &text, &enabled(&[]));
    assert_eq!(
        out.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
        vec!["FALTA_ELEMENT_ENTRE_VERBS"],
        "[3] survives the ![5] guard while [5] itself falls to the previous-sibling drop"
    );
    let no_sentence = make("OTHER", 4, 7, &["yyy"]);
    let out = catalan_filter_rule_matches(vec![falta, no_sentence], &text, &enabled(&[]));
    assert_eq!(out.len(), 2);
}

#[test]
fn falta_5_dropped_when_previous_match_belongs_to_the_same_rule() {
    let text = "aaa bbb ccc";
    let first = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "1", 0, 3, &["zzz"]);
    let falta5 = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "5", 4, 7, &["yyy"]);
    let out = catalan_filter_rule_matches(vec![first, falta5], &text, &enabled(&[]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].offset(), 0);
    let other_prev = make("OTHER", 0, 3, &["zzz"]);
    let falta5 = make_sub("FALTA_ELEMENT_ENTRE_VERBS", "5", 4, 7, &["yyy"]);
    let out = catalan_filter_rule_matches(vec![other_prev, falta5], &text, &enabled(&[]));
    assert_eq!(out.len(), 2, "previous non-FALTA match keeps [5]");
}

#[test]
fn results_are_stably_sorted_by_offset() {
    let text = "abc def";
    let late = make("LATE", 4, 7, &["deg"]);
    let early = make("EARLY", 0, 3, &["abd"]);
    let out = catalan_filter_rule_matches(vec![late, early], &text, &enabled(&[]));
    assert_eq!(
        out.iter().map(|m| m.offset()).collect::<Vec<_>>(),
        vec![0, 4]
    );
}

#[test]
fn adjust_appends_space_to_suggestion_for_apostrophe_error() {
    let text = "que l' aigua";
    let m = make("R", 0, 6, &["que la"]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["que la "]);
    let m = make("R", 0, 6, &["que l'", "que l’"]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(
        values(&out[0]),
        vec!["que l'", "que l’"],
        "suggestions already ending with an apostrophe are untouched"
    );
}

#[test]
fn general_accentuation_drops_the_acute_variant_when_the_grave_one_exists() {
    let text = "café";
    let m = make("R", 0, 5, &["café", "cafè"]);
    let out =
        catalan_filter_rule_matches(vec![m], &text, &enabled(&["EXIGEIX_ACCENTUACIO_GENERAL"]));
    assert_eq!(values(&out[0]), vec!["cafè"]);
    let m = make("R", 0, 5, &["CAFÉ", "CAFÈ"]);
    let out =
        catalan_filter_rule_matches(vec![m], &text, &enabled(&["EXIGEIX_ACCENTUACIO_GENERAL"]));
    assert_eq!(
        values(&out[0]),
        vec!["CAFÈ"],
        "only the É-carrying twin maps to its È twin, so the grave variant stays"
    );
    let m = make("R", 0, 4, &["café"]);
    let out =
        catalan_filter_rule_matches(vec![m], "cafe", &enabled(&["EXIGEIX_ACCENTUACIO_GENERAL"]));
    assert_eq!(
        values(&out[0]),
        vec!["café"],
        "no grave twin keeps the acute one"
    );
}

#[test]
fn despres_is_exempt_from_the_general_branch_in_any_case() {
    let text = "després";
    let m = make("R", 0, 8, &["DESPRÉS", "DESPRÈS"]);
    let out =
        catalan_filter_rule_matches(vec![m], &text, &enabled(&["EXIGEIX_ACCENTUACIO_GENERAL"]));
    assert_eq!(
        values(&out[0]),
        vec!["DESPRÉS", "DESPRÈS"],
        "equalsIgnoreCase(\"després\") flips to the else-branch, and Valencian is off"
    );
}

#[test]
fn valencian_accentuation_drops_the_grave_variant_when_the_acute_one_exists() {
    let text = "cafè";
    let m = make("R", 0, 5, &["cafè", "café"]);
    let out = catalan_filter_rule_matches(
        vec![m],
        &text,
        &enabled(&["EXIGEIX_ACCENTUACIO_VALENCIANA"]),
    );
    assert_eq!(values(&out[0]), vec!["café"]);
    let m = make("R", 0, 5, &["CAFÈ", "CAFÉ"]);
    let out = catalan_filter_rule_matches(
        vec![m],
        &text,
        &enabled(&["EXIGEIX_ACCENTUACIO_VALENCIANA"]),
    );
    assert_eq!(values(&out[0]), vec!["CAFÉ"]);
}

#[test]
fn typographic_apostrophe_rule_rewrites_straight_apostrophes() {
    let text = "la aigua";
    let m = make("R", 0, 8, &["l'aigua"]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&["APOSTROF_TIPOGRAFIC"]));
    assert_eq!(values(&out[0]), vec!["l’aigua"]);
    let m = make("R", 0, 8, &["l'aigua"]);
    let out = catalan_filter_rule_matches(
        vec![m],
        &text,
        &enabled(&["APOSTROF_TIPOGRAFIC", "APOSTROF_RECTE"]),
    );
    assert_eq!(values(&out[0]), vec!["l'aigua"], "APOSTROF_RECTE wins");
}

#[test]
fn sentence_typographic_apostrophe_alone_triggers_the_rewrite() {
    let text = "diu l'aigua";
    let m = make_with_sentence("R", 0, 3, &["l'aigua"], "que l’aigua");
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["l’aigua"]);
    let m = make("R", 0, 3, &["l'aigua"]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["l'aigua"]);
}

#[test]
fn possessiu_u_rewrites_possessives_and_feina_words() {
    let m = make("R", 3, 7, &["meva"]);
    let out = catalan_filter_rule_matches(vec![m], "sa muva", &enabled(&["EXIGEIX_POSSESSIUS_U"]));
    assert_eq!(values(&out[0]), vec!["meua"]);
    let m = make("R", 3, 8, &["seves"]);
    let out = catalan_filter_rule_matches(vec![m], "sa sevus", &enabled(&["EXIGEIX_POSSESSIUS_U"]));
    assert_eq!(values(&out[0]), vec!["seues"]);
    let m = make("R", 3, 7, &["TEVA"]);
    let out = catalan_filter_rule_matches(vec![m], "el TEVO", &enabled(&["EXIGEIX_POSSESSIUS_U"]));
    assert_eq!(values(&out[0]), vec!["TEUA"]);
    let m = make("R", 3, 8, &["feina"]);
    let out = catalan_filter_rule_matches(vec![m], "la feino", &enabled(&["EXIGEIX_POSSESSIUS_U"]));
    assert_eq!(values(&out[0]), vec!["faena"]);
    let m = make("R", 3, 7, &["meva"]);
    let out = catalan_filter_rule_matches(vec![m], "sa muva", &enabled(&[]));
    assert_eq!(
        values(&out[0]),
        vec!["meva"],
        "rule disabled keeps the original"
    );
}

#[test]
fn old_diacritics_are_removed_unless_traditional_rules_are_enabled() {
    let m = make("R", 0, 3, &["sóc"]);
    let out = catalan_filter_rule_matches(vec![m], "soc", &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["soc"]);
    let m = make("R", 0, 6, &["El vés"]);
    let out = catalan_filter_rule_matches(vec![m], "El ves", &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["El ves"]);
    let m = make("R", 0, 6, &["El vés"]);
    let out = catalan_filter_rule_matches(
        vec![m],
        "El ves",
        &enabled(&["DIACRITICS_TRADITIONAL_RULES"]),
    );
    assert_eq!(values(&out[0]), vec!["El vés"]);
    let m = make("R", 0, 15, &["porta sóc fóra"]);
    let out = catalan_filter_rule_matches(vec![m], "porta soc fora", &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["porta soc fora"]);
}

#[test]
fn adjusted_suggestions_are_deduplicated_by_value() {
    let m = make("R", 0, 4, &["sóc", "soc"]);
    let out = catalan_filter_rule_matches(vec![m], "sóc", &enabled(&[]));
    assert_eq!(values(&out[0]), vec!["soc"]);
}

#[test]
fn empty_single_suggestion_extends_the_span_over_the_following_space() {
    let text = "el gat a casa";
    let m = make_with_sentence("R", 3, 6, &[""], "el gat a casa");
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(out[0].offset(), 3);
    assert_eq!(out[0].length(), 4);
    assert_eq!(values(&out[0]), vec![""]);
    let m = make("R", 3, 6, &[""]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(
        out[0].length(),
        3,
        "without a sentence the span stays untouched"
    );
}

#[test]
fn empty_single_suggestion_needs_a_space_before_and_after() {
    let text = "el,gat a";
    let m = make_with_sentence("R", 3, 6, &[""], "el,gat a");
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(
        out[0].length(),
        3,
        "no space before and not sentence-initial"
    );
    let text = "el gat.";
    let m = make_with_sentence("R", 3, 6, &[""], "el gat.");
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(out[0].length(), 3, "no space after the removed word");
}

#[test]
fn adjust_preserves_rule_message_and_positions() {
    let text = "café";
    let m = make("MY_RULE", 0, 5, &["cafè"]);
    let out = catalan_filter_rule_matches(vec![m], &text, &enabled(&[]));
    assert_eq!(out[0].rule().id(), "MY_RULE");
    assert_eq!(out[0].message(), "msg");
    assert_eq!(out[0].offset(), 0);
    assert_eq!(out[0].length(), 5);
}

#[test]
fn java_equals_ignore_case_matches_java_semantics() {
    assert!(java_equals_ignore_case("després", "DESPRÉS"));
    assert!(java_equals_ignore_case("després", "Després"));
    assert!(!java_equals_ignore_case("després", "despres"));
    assert!(!java_equals_ignore_case("després", "despress"));
}
