use og_core::AnalyzedTokenReadings;

use super::celebres_pins::synthetic_sentence;
use super::pipeline::RefreshPipeline;
use super::pipeline::celebres_quote_sub_rule;
use super::pipeline::compile_fragment;
use super::pipeline::compiled_rules;
use super::pipeline::pattern_engine;

const RAW_POS_OFF_TWIN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="es">
<rule id="CELEBRE_QUOTE_NO_RAW" name="celebre -> célebre">
<pattern>
<token postag="D.*" postag_regexp="yes"/>
<marker><token regexp="yes">celebres?</token></marker>
<token regexp="yes" spacebefore="yes">[“‘«'"]</token>
</pattern>
<message>Si es un adjetivo, se escribe con tilde.</message>
<suggestion><match no="2" regexp_match="(?iu)ce" regexp_replace="cé" case_conversion="preserve"/></suggestion>
</rule>
</rules>
"#;

fn post_disambiguation_only_la() -> og_core::AnalyzedSentence {
    synthetic_sentence(&[("la", &["PP3FS000"]), ("celebre", &["AQ0CS0"]), ("\"", &[])])
}

fn snapshot_with_determiner_la() -> Vec<AnalyzedTokenReadings> {
    synthetic_sentence(&[
        ("la", &["PP3FS000", "D0FS0"]),
        ("celebre", &["AQ0CS0"]),
        ("\"", &[]),
    ])
    .tokens()
    .to_vec()
}

#[test]
fn raw_pos_sub_rule_matches_the_pre_disambiguation_snapshot() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    assert!(rule.pattern.raw_pos);
    let engine = pattern_engine();
    let mut sentence = post_disambiguation_only_la();
    sentence.set_pre_disambiguation_tokens(snapshot_with_determiner_la());
    assert_eq!(
        engine.match_rule(rule, &sentence).len(),
        1,
        "raw_pos=\"yes\" must read the D.* reading from the pre-disambiguation snapshot even \
         though the live tokens only keep the pronoun reading"
    );
}

#[test]
fn the_same_pattern_without_raw_pos_reads_the_disambiguated_tokens() {
    let twin = compile_fragment(RAW_POS_OFF_TWIN);
    assert_eq!(twin.len(), 1);
    assert!(!twin[0].pattern.raw_pos);
    let engine = pattern_engine();
    let mut sentence = post_disambiguation_only_la();
    sentence.set_pre_disambiguation_tokens(snapshot_with_determiner_la());
    assert!(
        engine.match_rule(&twin[0], &sentence).is_empty(),
        "without raw_pos the D.* element must read the disambiguated tokens and lose the \
         determiner reading"
    );
}

#[test]
fn without_a_snapshot_raw_pos_falls_back_to_the_current_tokens() {
    let rule_set = compiled_rules();
    let rule = celebres_quote_sub_rule(&rule_set);
    let engine = pattern_engine();
    let sentence = post_disambiguation_only_la();
    assert!(
        engine.match_rule(rule, &sentence).is_empty(),
        "with no snapshot set the pre-disambiguation view falls back to the current tokens, \
         which carry no D reading"
    );
}

fn token_readings(tokens: &[og_core::AnalyzedTokenReadings], surface: &str) -> Vec<String> {
    tokens
        .iter()
        .filter(|t| t.token().token() == surface)
        .flat_map(|t| {
            t.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn refresh_pipeline_snapshots_readings_before_disambiguation_like_the_checker() {
    let pipeline = RefreshPipeline::shared();
    let mut sentences = pipeline.analyze_offset_text("Fue la celebre \"plegariá\".");
    let (sentence, _) = sentences.remove(0);
    let pre_la = token_readings(sentence.pre_disambiguation_tokens(), "la");
    assert!(
        pre_la
            .iter()
            .any(|tags| tags.split('|').any(|t| t.starts_with('D'))),
        "the pre-disambiguation snapshot must keep a determiner reading for 'la': {pre_la:?}"
    );
    assert!(
        sentence
            .pre_disambiguation_tokens()
            .iter()
            .any(|t| t.token().token() == "\""),
        "the snapshot must contain the literal quote token the refreshed sub-rule matches on"
    );
    let post_la = token_readings(sentence.tokens(), "la");
    let post_quote = token_readings(sentence.tokens(), "\"");
    eprintln!("es la pre={pre_la:?} post={post_la:?} quote post={post_quote:?}");
}
