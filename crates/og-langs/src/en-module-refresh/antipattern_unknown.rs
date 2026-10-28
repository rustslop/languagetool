use super::pipeline::EnRefreshPipeline;
use super::pipeline::compiled_rules;
use super::pipeline::grammar_available;
use super::pipeline::grammar_path;
use super::pipeline::pattern_engine;
use super::pipeline::strip_marker;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;

const GROUP_ID: &str = "HAVE_PART_AGREEMENT";
const SUPPRESSED: &str = "The tools Graaag should have are all broken.";
const ALL_TOOLS: &str = "All tools Graaag should have are broken.";
const FIXED_TOKEN: &str = "postag=\"NN.*|UNKNOWN\" postag_regexp=\"yes\" chunk_re=\"E-NP.*\"";

fn group_match_spans(
    rule_set: &CompiledRuleSet,
    engine: &PatternRuleEngine,
    pipeline: &EnRefreshPipeline,
    text: &str,
) -> Vec<(usize, usize)> {
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut spans = Vec::new();
    for rule in rule_set.rules.iter().filter(|rule| rule.id == GROUP_ID) {
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for rule_match in engine.match_rule(rule, &sentence) {
                let start = rule_match.offset() + sentence_start;
                spans.push((start, start + rule_match.length()));
            }
        }
    }
    spans
}

fn assert_graaag_is_an_unknown_np_head(pipeline: &EnRefreshPipeline, text: &str) {
    let sentence = pipeline.analyze_sentence(text);
    let graaag = sentence
        .tokens()
        .iter()
        .find(|token| token.token().token() == "Graaag")
        .unwrap_or_else(|| panic!("Graaag token in '{text}'"));
    assert!(
        og_xml::unifier::has_no_tag_reading(graaag),
        "'Graaag' carries no dictionary reading, so only the refreshed UNKNOWN alternative of \
         the HAVE_PART_AGREEMENT antipattern can match it: {:?}",
        graaag.readings()
    );
    assert!(
        graaag
            .chunks()
            .iter()
            .any(|chunk| chunk.starts_with("E-NP")),
        "'Graaag' must close the NP chunk the antipattern requires: {:?}",
        graaag.chunks()
    );
}

#[test]
fn antipattern_example_sentence_stays_unflagged() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let pipeline = EnRefreshPipeline::shared();
    assert_graaag_is_an_unknown_np_head(&pipeline, SUPPRESSED);
    let spans = group_match_spans(&compiled_rules(), &pattern_engine(), &pipeline, SUPPRESSED);
    assert!(
        spans.is_empty(),
        "HAVE_PART_AGREEMENT must stay silent on the antipattern example '{SUPPRESSED}', got \
         matches at {spans:?}"
    );
}

#[test]
fn unknown_arm_of_the_refreshed_antipattern_is_load_bearing() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let pipeline = EnRefreshPipeline::shared();
    assert_graaag_is_an_unknown_np_head(&pipeline, ALL_TOOLS);
    let live_spans = group_match_spans(&compiled_rules(), &pattern_engine(), &pipeline, ALL_TOOLS);
    assert!(
        live_spans.is_empty(),
        "the refreshed antipattern must suppress the rulegroup on the untagged-head shape \
         '{ALL_TOOLS}', got matches at {live_spans:?}"
    );

    let path = grammar_path().expect("en grammar.xml path");
    let xml = std::fs::read_to_string(&path).expect("en grammar.xml");
    // og-xml's anchor_regex trims postag patterns, so the pre-4cff7008 trailing space never
    // compiled to a dead UNKNOWN arm here; deleting the antipattern is the only observable.
    let example_at = xml
        .find(SUPPRESSED)
        .expect("the upstream antipattern example sentence");
    let block_start = xml[..example_at]
        .rfind("<antipattern")
        .expect("the antipattern wrapping the example");
    let block_end = xml[block_start..]
        .find("</antipattern>")
        .expect("the antipattern close")
        + block_start
        + "</antipattern>".len();
    assert!(
        xml[block_start..block_end].contains(FIXED_TOKEN),
        "the antipattern wrapping the example must be the refreshed UNKNOWN one"
    );
    let mut without_it = xml;
    without_it.replace_range(block_start..block_end, "");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&without_it, path.parent())
        .expect("compile without the refreshed antipattern");
    assert_eq!(
        rule_set.rules.len(),
        compiled_rules().rules.len(),
        "removing one antipattern must not change the rule inventory"
    );
    let are_at = ALL_TOOLS.find("are").expect("the 'are' token");
    let spans = group_match_spans(&rule_set, &pattern_engine(), &pipeline, ALL_TOOLS);
    assert!(
        spans
            .iter()
            .any(|(start, end)| *start == are_at && *end == are_at + "are".len()),
        "without the refreshed antipattern, sub-rule [1] must flag 'are' in '{ALL_TOOLS}' — \
         the suppression above is carried by the UNKNOWN alternative matching the untagged \
         'Graaag'; got {spans:?}"
    );
}

#[test]
fn have_part_agreement_still_fires_its_sub_rule_example() {
    if !grammar_available() {
        eprintln!("Skipping: en grammar.xml not found");
        return;
    }
    let _guard = crate::release_guard();
    let (clean, marker) = strip_marker("I must have <marker>wrote</marker> that.");
    let marker = marker.expect("marker range on the sub-rule example");
    let pipeline = EnRefreshPipeline::shared();
    let spans = group_match_spans(&compiled_rules(), &pattern_engine(), &pipeline, &clean);
    assert!(
        spans
            .iter()
            .any(|(start, end)| *start <= marker.0 && *end >= marker.1),
        "HAVE_PART_AGREEMENT must keep firing its own incorrect example '{clean}' with the \
         marker {marker:?} covered; got {spans:?}"
    );
}
