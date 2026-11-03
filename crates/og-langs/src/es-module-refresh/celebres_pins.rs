use og_core::AnalyzedSentence;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use og_xml::compiler::CompiledPatternToken;
use og_xml::types::XmlExampleType;

use super::pipeline::RefreshPipeline;
use super::pipeline::celebres_group;
use super::pipeline::celebres_quote_sub_rule;
use super::pipeline::compile_fragment;
use super::pipeline::compiled_rules;
use super::pipeline::pattern_engine;

const PRE_REFRESH_FORM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="es">
<rule id="CELEBRE_QM_OPEN" name="celebre -> célebre">
<pattern raw_pos="yes">
<token postag="D.*" postag_regexp="yes"/>
<marker><token regexp="yes">celebres?</token></marker>
<token postag="_QM_OPEN"/>
</pattern>
<message>Si es un adjetivo, se escribe con tilde.</message>
<suggestion><match no="2" regexp_match="(?iu)ce" regexp_replace="cé" case_conversion="preserve"/></suggestion>
</rule>
</rules>
"#;

fn quote_class_matches(quote: &CompiledPatternToken, surface: &str) -> bool {
    if let Some(re) = &quote.compiled_regexp_fancy {
        re.is_match(surface).unwrap_or(false)
    } else if quote.case_sensitive {
        quote
            .compiled_regexp
            .as_ref()
            .is_some_and(|re| re.is_match(surface))
    } else {
        quote
            .compiled_regexp_ci
            .as_ref()
            .or(quote.compiled_regexp.as_ref())
            .is_some_and(|re| re.is_match(surface))
    }
}

fn sub_rule_matches(
    text: &str,
    rule: &og_xml::compiler::CompiledRule,
) -> Vec<(usize, usize, Vec<String>)> {
    let engine = pattern_engine();
    let pipeline = RefreshPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut found = Vec::new();
    for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
        for m in engine.match_rule(rule, &sentence) {
            let replacements = m
                .replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect();
            found.push((m.offset() + sentence_start, m.length(), replacements));
        }
    }
    found
}

pub(crate) fn synthetic_sentence(tokens: &[(&str, &[&str])]) -> AnalyzedSentence {
    let mut offset = 0usize;
    let mut analyzed = vec![AnalyzedTokenReadings::new(
        AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
    )];
    for &(text, postags) in tokens {
        let start = offset;
        let end = offset + text.len();
        let base = AnalyzedToken::new(text, start, end).with_lemma(text);
        let readings: Vec<AnalyzedToken> = postags
            .iter()
            .map(|pos| {
                AnalyzedToken::new(text, start, end)
                    .with_pos_tags(vec![pos.to_string()])
                    .with_lemma(text)
            })
            .collect();
        let atr = if readings.is_empty() {
            AnalyzedTokenReadings::new(base)
        } else {
            AnalyzedTokenReadings::new(base).with_readings(readings)
        };
        analyzed.push(atr);
        offset = end + 1;
    }
    let mut sentence = AnalyzedSentence::new("synthetic", 0, offset.saturating_sub(1));
    sentence.set_tokens(analyzed);
    sentence
}

#[test]
fn celebres_quote_sub_rule_compiles_to_the_literal_quote_token() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    assert_eq!(
        rule.sub_id.as_deref(),
        Some("2"),
        "the rewritten sub-rule keeps its rulegroup slot"
    );
    assert!(
        rule.pattern.raw_pos,
        "the rewritten sub-rule keeps raw_pos=\"yes\""
    );
    let quotes: Vec<&CompiledPatternToken> = rule
        .pattern
        .tokens
        .iter()
        .filter(|t| t.space_before.as_deref() == Some("yes"))
        .collect();
    assert_eq!(
        quotes.len(),
        1,
        "exactly one spacebefore token in the refreshed sub-rule"
    );
    let quote = quotes[0];
    assert!(quote.is_regexp, "the quote token is a regexp token");
    assert!(
        quote.postag.is_none(),
        "the _QM_OPEN postag is gone from the token"
    );
    for open in ["\u{201c}", "\u{2018}", "\u{ab}", "'", "\""] {
        assert!(
            quote_class_matches(quote, open),
            "the class [“‘«'\"] must match {open:?}"
        );
    }
    for close in ["\u{201d}", "\u{2019}", "\u{bb}"] {
        assert!(
            !quote_class_matches(quote, close),
            "closing quote {close:?} must stay outside the class"
        );
    }
    assert_eq!(
        rule.pattern.tokens.len(),
        3,
        "determiner + marked celebres? + quote"
    );
    assert_eq!(rule.pattern.tokens[0].postag.as_deref(), Some("D.*"));
    assert!(
        rule.pattern.tokens[1].is_regexp,
        "the marked token stays a regexp token"
    );
    assert!(
        ["celebre", "celebres"].iter().all(|form| {
            rule.pattern.tokens[1]
                .compiled_regexp
                .as_ref()
                .or(rule.pattern.tokens[1].compiled_regexp_ci.as_ref())
                .is_some_and(|re| re.is_match(form))
        }),
        "the marked token regex must keep matching celebre/celebres"
    );
    assert_eq!(
        (rule.pattern.marker_elem_start, rule.pattern.marker_elem_end),
        (Some(1), Some(2)),
        "the marker wraps only the celebres? element"
    );
}

#[test]
fn celebres_quote_sub_rule_fires_on_its_example_with_the_célebre_fix() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    let text = "Fue la celebre \"plegariá\".";
    let matches = sub_rule_matches(text, rule);
    assert_eq!(
        matches.len(),
        1,
        "the refreshed sub-rule must fire exactly once on its example '{text}'"
    );
    let (offset, length, replacements) = &matches[0];
    assert_eq!(&text[*offset..*offset + *length], "celebre");
    assert_eq!(
        replacements,
        &vec!["c\u{e9}lebre".to_string()],
        "the match no=\"2\" ce->cé rewrite must build the suggestion"
    );
    assert_eq!(rule.examples.len(), 1);
    assert_eq!(
        rule.examples[0].example_type,
        XmlExampleType::Incorrect,
        "the example stays an incorrect one"
    );
    assert_eq!(
        rule.examples[0].corrections,
        vec!["c\u{e9}lebre".to_string()],
        "the example correction attribute stays célebre"
    );
}

#[test]
fn celebres_quote_sub_rule_covers_every_opening_quote_shape() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    for (open, close) in [
        ("\u{201c}", "\u{201d}"),
        ("\u{2018}", "\u{2019}"),
        ("\u{ab}", "\u{bb}"),
        ("'", "'"),
    ] {
        let text = format!("Fue la celebre {open}plegariá{close}.");
        let matches = sub_rule_matches(&text, rule);
        assert_eq!(
            matches.len(),
            1,
            "opening quote {open:?} must drive the sub-rule on '{text}'"
        );
        let (offset, length, _) = &matches[0];
        assert_eq!(&text[*offset..*offset + *length], "celebre");
    }
}

#[test]
fn celebres_quote_sub_rule_requires_a_space_before_the_quote() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    let glued = "Fue la celebre\"plegariá\".";
    assert!(
        sub_rule_matches(glued, rule).is_empty(),
        "spacebefore=\"yes\" must reject a quote glued to celebre: '{glued}'"
    );
    let spaced = "Fue la celebre \"plegariá\".";
    assert_eq!(
        sub_rule_matches(spaced, rule).len(),
        1,
        "the spaced control keeps matching"
    );
}

#[test]
fn celebres_old_qm_open_form_stays_dead_in_the_es_pipeline() {
    let old_form = compile_fragment(PRE_REFRESH_FORM);
    assert_eq!(old_form.len(), 1);
    let rule = &old_form[0];
    assert!(rule.pattern.raw_pos);
    let text = "Fue la celebre \"plegariá\".";
    assert!(
        sub_rule_matches(text, rule).is_empty(),
        "raw_pos=\"yes\" reads the pre-disambiguation snapshot, where _QM_OPEN does not exist \
         yet, so the pre-refresh form could never match"
    );
    let mut without_raw_pos = PRE_REFRESH_FORM.replace("<pattern raw_pos=\"yes\">", "<pattern>");
    without_raw_pos =
        without_raw_pos.replace("id=\"CELEBRE_QM_OPEN\"", "id=\"CELEBRE_QM_OPEN_NO_RAW\"");
    let twin = compile_fragment(&without_raw_pos);
    assert_eq!(twin.len(), 1);
    assert!(!twin[0].pattern.raw_pos);
    assert_eq!(
        sub_rule_matches(text, &twin[0]).len(),
        1,
        "the same pattern reading the disambiguated tokens finds the _QM_OPEN the es \
         disambiguator adds, proving the old form died from the raw_pos + postag combination \
         and not from missing engine support"
    );
}

#[test]
fn qm_open_only_exists_after_disambiguation_on_live_quotes() {
    let pipeline = RefreshPipeline::shared();
    let mut sentences = pipeline.analyze_offset_text("Fue la celebre \"plegariá\".");
    let (sentence, _) = sentences.remove(0);
    let has_qm_open = |tokens: &[og_core::AnalyzedTokenReadings]| {
        tokens
            .iter()
            .filter(|t| t.token().token() == "\"")
            .flat_map(|t| t.readings())
            .any(|r| r.pos_tags().iter().any(|tag| tag == "_QM_OPEN"))
    };
    assert!(
        !has_qm_open(sentence.pre_disambiguation_tokens()),
        "the snapshot the raw_pos sub-rule reads cannot carry _QM_OPEN"
    );
    assert!(
        has_qm_open(sentence.tokens()),
        "the es disambiguation QM_OPEN rule adds the tag to the live quote token"
    );
}

#[test]
fn qm_open_readings_still_drive_a_postag_token_when_present() {
    let old_form = compile_fragment(PRE_REFRESH_FORM);
    let rule = &old_form[0];
    let engine = pattern_engine();
    let sentence = synthetic_sentence(&[
        ("la", &["D0FS0"]),
        ("celebre", &["AQ0CS0"]),
        ("\"", &["_QM_OPEN"]),
        ("plegariá", &["NCFS000"]),
        (".", &["Fp"]),
    ]);
    let matches = engine.match_rule(rule, &sentence);
    assert_eq!(
        matches.len(),
        1,
        "with a real _QM_OPEN reading the postag machinery matches, so the refresh is a \
         data-flow change and not an engine capability"
    );
    assert_eq!(
        (matches[0].offset(), matches[0].length()),
        (3, 7),
        "the marker must cover the synthetic celebre token span"
    );
}

#[test]
fn celebres_group_keeps_its_sibling_sub_rules() {
    let rule_set = compiled_rules();
    let group = celebres_group(&rule_set);
    assert_eq!(
        group.len(),
        4,
        "CELEBRE compiles its four rulegroup members (adverb, quote, por/de/del, y-concordance)"
    );
    let sub_ids: Vec<Option<&str>> = group.iter().map(|r| r.sub_id.as_deref()).collect();
    assert_eq!(
        sub_ids,
        vec![Some("1"), Some("2"), Some("3"), Some("4")],
        "the rewritten quote sub-rule sits in slot 2"
    );
    assert!(
        group
            .iter()
            .filter(|r| r.sub_id.as_deref() == Some("3"))
            .all(|r| !r.antipatterns.is_empty()),
        "the por/de/del sub-rule keeps its antipatterns"
    );
    assert!(
        group
            .iter()
            .filter(|r| r.sub_id.as_deref() == Some("4"))
            .all(|r| r
                .filter
                .as_ref()
                .is_some_and(|f| f.class.ends_with("ConfusionCheckFilter"))),
        "the y-concordance sub-rule keeps its ConfusionCheckFilter"
    );
    let total_examples: usize = group.iter().map(|r| r.examples.len()).sum();
    assert_eq!(
        total_examples, 7,
        "the refresh left the CELEBRE group example mass unchanged (1+1+3+2)"
    );
}
