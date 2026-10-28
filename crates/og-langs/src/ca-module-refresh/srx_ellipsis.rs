use og_tokenizer::SentenceTokenizer as _;
use og_tokenizer::srx::srx_splitter;

fn segments(text: &str) -> Vec<String> {
    let splitter = srx_splitter("ca").expect("ca SRX ruleset must resolve");
    splitter
        .split(text)
        .iter()
        .map(|s| s.text().to_string())
        .collect()
}

#[test]
fn spaced_out_ellipsis_does_not_break_between_the_dots() {
    let got = segments("Aix\u{f2} \u{e9}s veritat . . . i ara toca continuar.");
    assert_eq!(
        got.len(),
        1,
        "segment.srx:4813-4817 (break=no, afterbreak \\.(?=[\\s\\u00A0])) vetoes the \
         dot-space-dot positions of a spaced-out ellipsis: {got:?}"
    );
}

#[test]
fn spaced_out_ellipsis_keeps_a_lowercase_continuation_attached() {
    let got = segments("Va dir que no . . . per\u{f2} despr\u{e9}s va canviar d'opini\u{f3}.");
    assert_eq!(
        got.len(),
        1,
        "segment.srx:4818-4822 (break=no, beforebreak (?:\\.[\\s\\u00A0]){{2,}} afterbreak \
         [^\\p{{Lu}}]) keeps the tail sentence attached when it starts lowercase: {got:?}"
    );
}

#[test]
fn spaced_out_ellipsis_still_breaks_before_an_uppercase_restart() {
    let got = segments("Aix\u{f2} \u{e9}s veritat . . . I ara qu\u{e8} fem?");
    assert_eq!(
        got.len(),
        2,
        "after the last dot the [^\\p{{Lu}}] veto must not hold and the sentence breaks: {got:?}"
    );
    assert!(
        got[1].starts_with("I "),
        "the second sentence must start at the uppercase restart: {got:?}"
    );
}

#[test]
fn spaced_out_ellipsis_with_nbsp_separators_stays_too() {
    let got = segments("Aix\u{f2} \u{e9}s veritat .\u{00a0}.\u{00a0}.\u{00a0}i continua.");
    assert_eq!(
        got.len(),
        1,
        "the new rules cover U+00A0 separators exactly like plain spaces: {got:?}"
    );
}

#[test]
fn regular_sentence_breaks_are_untouched_by_the_new_rules() {
    let got = segments("Aix\u{f2} \u{e9}s una frase. Aquesta \u{e9}s una altra frase.");
    assert_eq!(got.len(), 2, "{got:?}");
    let got = segments("S\u{f3}n del s. III dC. S\u{f3}n importants les pintures.");
    assert_eq!(
        got.len(),
        2,
        "'s. III' abbreviation must not break the sentence: {got:?}"
    );
}
