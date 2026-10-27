#[test]
fn probe_klanv_compiles_the_token_level_match_transform() {
    let (rule_count, sentences) =
        crate::br::example_suite::trace_rule_matches("KLANV_PE_GLANVOCH", "Klañv pe klañvoc’h.");
    assert!(rule_count > 0, "KLANV_PE_GLANVOCH sub-rules must compile");
    let tokens = sentences[0].non_whitespace_tokens();
    let klañvoc_h = tokens
        .iter()
        .find(|t| t.token().token() == "klañvoc’h")
        .expect("klañvoc’h token");
    assert!(
        klañvoc_h
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t == "J cmp")),
        "the dictionary carries the J cmp reading, so the transform is the sole gate: {:?}",
        klañvoc_h.readings()
    );
    let matched = crate::br::example_suite::all_grammar_matches("Klañv pe klañvoc’h.");
    assert!(
        matched.iter().any(|(id, _, _)| id == "KLANV_PE_GLANVOCH"),
        "Java PatternToken.doCompile substitutes matchState.toTokenString() for the \\3 \
         reference: ‘Klañv’ through ^.(.*)→(k|c’h)$1oc’h becomes the regexp alternation \
         ‘(k|c’h)lañvoc’h’, which ‘klañvoc’h’ must match: {matched:?}"
    );
    let matched = crate::br::example_suite::all_grammar_matches("Klañv pe c’hlañvoc’h.");
    assert!(
        matched.iter().any(|(id, _, _)| id == "KLANV_PE_GLANVOCH"),
        "the c’h alternative of the transformed regexp must match ‘c’hlañvoc’h’: {matched:?}"
    );
}

#[test]
fn probe_och_an_compiles_the_token_level_match_transform() {
    let (rule_count, _) = crate::br::example_suite::trace_rule_matches(
        "OCH_AN",
        "Diaesoc’h diaesañ eo kavout labour.",
    );
    assert!(rule_count > 0, "OCH_AN sub-rules must compile");
    let matched =
        crate::br::example_suite::all_grammar_matches("Diaesoc’h diaesañ eo kavout labour.");
    assert!(
        matched.iter().any(|(id, _, _)| id == "OCH_AN"),
        "Java compiles ‘Diaesoc’h’ through oc’h$→añ into the literal ‘Diaesañ’, which the \
         non-regexp second element must equal case-insensitively: {matched:?}"
    );
}

#[test]
fn probe_deiz_deiziad_month_with_u2019_survives_the_shared_date_filter_trim() {
    let (_, sentences) = crate::br::example_suite::trace_rule_matches(
        "DEIZ_DEIZIAD",
        "D’ar Merc’her 6 a viz C’hwevrer 2014.",
    );
    let tokens = sentences[0].non_whitespace_tokens();
    let month = tokens
        .iter()
        .find(|t| t.token().token().contains("hwevrer"))
        .expect("month token");
    assert_eq!(
        month.token().token(),
        "C’hwevrer",
        "the tokenizer restores the U+2019 form that Java’s getMonth compares against"
    );
    let curly =
        crate::br::example_suite::all_grammar_matches("D’ar Merc’her 6 a viz C’hwevrer 2014.");
    assert!(
        curly.iter().any(|(id, _, _)| id == "DEIZ_DEIZIAD"),
        "Java’s StringTools.trimSpecialCharacters (StringTools.java:91) keeps U+2019 via \
         (?U)\\p{{Punct}} (Unicode Pf), so the shared trim_special_chars helper must keep \
         Unicode punctuation and the C’hwevrer month must parse like the ASCII variant"
    );
    let ascii =
        crate::br::example_suite::all_grammar_matches("D’ar Merc’her 6 a viz Gwengolo 2014.");
    assert!(
        ascii.iter().any(|(id, _, _)| id == "DEIZ_DEIZIAD"),
        "the Gwengolo variant differs only in the month word, which the same \
         alternation element covers, so the pattern side is proven working and the \
         drop sits in the shared date filter's month parse: {ascii:?}"
    );
}
