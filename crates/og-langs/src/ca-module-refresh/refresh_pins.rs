use std::collections::HashMap;

use og_core::rule::Rule as _;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::types::XmlExampleType;

use super::example_suite::RefreshPipeline;
use super::example_suite::ca_resource_dir;
use super::example_suite::ca_rules_dir;
use super::example_suite::compiled_rules;
use super::example_suite::strip_marker;

const NEW_STANDALONE_RULES: [&str; 13] = [
    "ANS_ENS",
    "CRISTALLINA",
    "D_E_DE",
    "JA_MINUSCULA",
    "MAJORA",
    "MAJORES",
    "MAJOR_A",
    "PERDUDA_PERDUA",
    "PETO",
    "S_OBRE_SOBRE",
    "TAT_TANT",
    "TE_LA_COPIA",
    "VEN_BEN",
];

const NEW_RULEGROUPS: [&str; 2] = ["D_ENCA", "MEREIXER_LA_PENA"];

const SURFACE_ONLY_NEW_RULES: [&str; 8] = [
    "CRISTALLINA",
    "D_E_DE",
    "MAJORA",
    "MAJORES",
    "MAJOR_A",
    "TAT_TANT",
    "TE_LA_COPIA",
    "VEN_BEN",
];

const TAG_DEPENDENT_NEW_RULES: [&str; 5] = [
    "ANS_ENS",
    "JA_MINUSCULA",
    "PERDUDA_PERDUA",
    "PETO",
    "S_OBRE_SOBRE",
];

fn rules_by_id<'a>(
    rule_set: &'a og_xml::compiler::CompiledRuleSet,
) -> HashMap<&'a str, Vec<&'a og_xml::compiler::CompiledRule>> {
    let mut by_id: HashMap<&'a str, Vec<&'a og_xml::compiler::CompiledRule>> = HashMap::new();
    for rule in &rule_set.rules {
        by_id.entry(rule.id.as_str()).or_default().push(rule);
    }
    by_id
}

#[test]
fn grammar_totals_pin_the_refreshed_file() {
    let rule_set = compiled_rules();
    assert_eq!(
        rule_set.rules.len(),
        9651,
        "the 4cff7008 grammar.xml must compile exactly its 9651 live <rule> elements"
    );
    let total_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    assert_eq!(
        total_examples, 25732,
        "26833 non-comment <example> occurrences minus the 1101 inside <antipattern> elements"
    );
}

#[test]
fn new_rule_cohort_and_removed_rulegroup_are_reflected() {
    let rule_set = compiled_rules();
    let by_id = rules_by_id(&rule_set);
    for id in NEW_STANDALONE_RULES {
        let group = by_id
            .get(id)
            .unwrap_or_else(|| panic!("rule {id} added by the refresh must compile"));
        assert_eq!(
            group.len(),
            1,
            "{id} appears exactly once (several of the new rules live inside the RARE_WORDS \
             rulegroup and carry its sub ids)"
        );
        assert!(
            group.iter().all(|r| !r.examples.is_empty()),
            "{id} must ship examples"
        );
    }
    for id in NEW_RULEGROUPS {
        let group = by_id
            .get(id)
            .unwrap_or_else(|| panic!("rulegroup {id} must compile"));
        assert_eq!(
            group.len(),
            2,
            "{id} became a two-member rulegroup in the refresh"
        );
        assert!(
            group.iter().all(|r| r.sub_id.is_some()),
            "{id} group members carry sub ids"
        );
    }
    assert!(
        !by_id.contains_key("PHRASE_REPETITION"),
        "the PHRASE_REPETITION rulegroup was removed; CATALAN_PHRASE_REPEAT_RULE replaces it"
    );
    let entra_entre = by_id
        .get("ENTRA_ENTRE")
        .expect("ENTRA_ENTRE stays compiled");
    assert!(
        entra_entre.iter().all(|r| r.default_on),
        "the refresh dropped ENTRA_ENTRE's default=\"temp_off\" and re-enabled the rule"
    );
}

#[test]
fn new_rules_split_cleanly_into_surface_and_dict_dependent() {
    let rule_set = compiled_rules();
    let by_id = rules_by_id(&rule_set);
    for id in SURFACE_ONLY_NEW_RULES {
        let group = by_id.get(id).expect(id);
        assert!(
            group
                .iter()
                .all(|r| !crate::ca::trace::rule_needs_readings(r)),
            "{id} must be matchable without dictionary readings"
        );
    }
    for id in TAG_DEPENDENT_NEW_RULES {
        let group = by_id.get(id).expect(id);
        assert!(
            group
                .iter()
                .all(|r| crate::ca::trace::rule_needs_readings(r)),
            "{id} needs postag/inflected readings and stays under the missing-dict umbrella"
        );
    }
}

#[test]
fn surface_only_new_rules_fire_their_examples_through_the_pipeline() {
    let engine = PatternRuleEngine::new();
    let pipeline = RefreshPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let rule_set = compiled_rules();
    let by_id = rules_by_id(&rule_set);
    for id in SURFACE_ONLY_NEW_RULES {
        let group = by_id.get(id).expect(id);
        let incorrect: Vec<_> = group
            .iter()
            .flat_map(|r| r.examples.iter())
            .filter(|e| e.example_type == XmlExampleType::Incorrect)
            .collect();
        assert!(
            !incorrect.is_empty(),
            "{id} must carry at least one incorrect example"
        );
        for example in incorrect {
            let (clean, marker) = strip_marker(&example.text);
            let mut matched = false;
            let mut covered = marker.is_none();
            for rule in group {
                for (sentence, _) in pipeline.analyze_offset_text(&clean) {
                    for m in engine.match_rule(rule, &sentence) {
                        matched = true;
                        if let Some((from, to)) = marker {
                            let start = m.offset();
                            let end = start + m.length();
                            covered |= start <= from && end >= to;
                        }
                    }
                }
            }
            assert!(
                matched && covered,
                "{id} must fire on its incorrect example '{clean}'"
            );
        }
        for example in group
            .iter()
            .flat_map(|r| r.examples.iter())
            .filter(|e| e.example_type == XmlExampleType::Correct)
        {
            let (clean, _) = strip_marker(&example.text);
            let mut fired = 0usize;
            for rule in group {
                for (sentence, _) in pipeline.analyze_offset_text(&clean) {
                    fired += engine.match_rule(rule, &sentence).len();
                }
            }
            assert_eq!(
                fired, 0,
                "{id} must stay silent on its correct example '{clean}'"
            );
        }
    }
}

#[test]
fn disambiguation_refresh_loads_with_its_new_rules() {
    let resource_dir = ca_resource_dir().expect("ca resource dir");
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("disambig");
    let disambiguator = og_tagger::XmlDisambiguator::from_xml(&xml).expect("parse disambig");
    let ids = disambiguator.rule_ids();
    for id in ["davant", "deure", "IMMUNIZE_ELLIPSIS"] {
        assert!(
            ids.contains(&id),
            "rule '{id}' added by the refresh must parse, got {} ids",
            ids.len()
        );
    }
    assert!(
        ids.contains(&"gens"),
        "the 'gens + adj' rule was restructured into a rulegroup, not removed"
    );
    let untouched: usize = (0..disambiguator.rule_count())
        .map(|i| disambiguator.rule_examples(i).0.len())
        .sum();
    assert_eq!(
        untouched, 72,
        "the XML carries 76 untouched examples (75 before the refresh; the +1 is          IMMUNIZE_ELLIPSIS) and the og-xml parser stably surfaces 72 of them (it dropped the          same 4 in the pre-refresh file)"
    );
}

#[test]
fn added_txt_refresh_reaches_the_tagger() {
    let pipeline = RefreshPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Artaxerxes era un rei persa.");
    let token = sentences[0]
        .0
        .tokens()
        .iter()
        .find(|t| t.token().token() == "Artaxerxes")
        .expect("Artaxerxes token");
    assert!(
        token.has_pos_tag("NPMSSP0"),
        "the added.txt line 'Artaxerxes\tArtaxerxes\tNPMSSP0' must reach the readings: {:?}",
        token.readings()
    );
}

#[test]
fn check_case_refresh_covers_the_new_entries() {
    let rule = crate::ca::native_rules::CheckCaseRule::new();
    let pipeline = RefreshPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Ahir va venir el ni\u{f1}o i avui marxa.");
    let matches = rule.match_sentence(&sentences[0].0);
    assert_eq!(
        matches.len(),
        1,
        "'el niño' must fire CA_CHECKCASE against the new 'el Niño' entry"
    );
    assert!(
        matches[0]
            .replacements()
            .iter()
            .any(|r| r.value() == "el Ni\u{f1}o"),
        "suggestion must be the canonical 'el Niño': {:?}",
        matches[0].replacements()
    );
}

#[test]
fn compounds_refresh_joins_trenca_closques_and_drops_baby_boom() {
    let resource_dir = ca_resource_dir().expect("ca resource dir");
    let data = std::fs::read_to_string(resource_dir.join("compounds.txt")).expect("compounds");
    assert!(
        data.lines()
            .any(|l| l.trim() == "trenca-closques+" || l.trim() == "suca-mulla+"),
        "the refresh added the trenca-closques+/suca-mulla+ entries"
    );
    assert!(
        data.lines().any(|l| l.trim() == "#baby-boom*"),
        "baby-boom* was commented out by the refresh"
    );
    let rule = crate::ca::compound_rule::CompoundRule::new();
    let pipeline = RefreshPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Era un joc trenca closques.");
    assert!(
        rule.match_sentence(&sentences[0].0).is_empty(),
        "Java CompoundRule only offers a merge the speller knows: 'trencaclosques' is not in \
         added.txt, so without ca-ES.dict the suggestion is filtered as misspelled and the \
         rule stays dormant (accepted-excluded under the dict umbrella)"
    );
    let sentences = pipeline.analyze_offset_text("El baby boom va ser llarg.");
    assert!(
        rule.match_sentence(&sentences[0].0).is_empty(),
        "commenting out baby-boom* must stop 'baby boom' from firing"
    );
    let sentences = pipeline.analyze_offset_text("El blau grana \u{e9}s bonic.");
    let matches = rule.match_sentence(&sentences[0].0);
    assert_eq!(
        matches.len(),
        1,
        "the rule itself still fires when the joined suggestion is tagged via added.txt"
    );
}

#[test]
fn replace_txt_refresh_maps_the_new_entries() {
    let rule = crate::ca::simple_replace::catalan_simple_replace_rule();
    let sentence = crate::ca::native_rules::sentence_with_readings(&[("alemana", &[]), (".", &[])]);
    let matches = rule.match_sentence(&sentence);
    assert_eq!(matches.len(), 1, "'alemana' is a new replace.txt key");
    assert_eq!(matches[0].replacements()[0].value(), "alemanya");
    let sentence = crate::ca::native_rules::sentence_with_readings(&[("postraumatic", &[])]);
    let matches = rule.match_sentence(&sentence);
    assert_eq!(
        matches[0]
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect::<Vec<_>>(),
        vec!["posttraum\u{e0}tic".to_string()],
        "the diacritic-preserving alternation 'postraumatic|postraumàtic=posttraumàtic'"
    );
}

#[test]
fn inert_data_files_pin_the_refreshed_sizes() {
    let resource_dir = ca_resource_dir().expect("ca resource dir");
    let pins: [(&str, usize); 6] = [
        ("added.txt", 5338),
        ("spelling.txt", 4238),
        ("removed.txt", 902),
        ("multiwords.txt", 15632),
        ("hyphenated_words.txt", 10715),
        ("compounds.txt", 1809),
    ];
    for (file, expected) in pins {
        let text = std::fs::read_to_string(resource_dir.join(file)).expect(file);
        let live = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .count();
        assert_eq!(live, expected, "{file} live (non-comment) line count");
    }
    let rules_dir = ca_rules_dir().expect("ca rules dir");
    let pins: [(&str, usize); 5] = [
        ("replace.txt", 3668),
        ("replace_multiwords.txt", 996),
        ("replace_verbs.txt", 458),
        ("replace_anglicism.txt", 511),
        ("check_case.txt", 2591),
    ];
    for (file, expected) in pins {
        let text = std::fs::read_to_string(rules_dir.join(file)).expect(file);
        let live = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .count();
        assert_eq!(live, expected, "{file} live (non-comment) line count");
    }
}

#[test]
fn multiwords_refresh_adds_the_new_proper_noun_readings() {
    let pipeline = RefreshPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Franz Xaver Richter va n\u{e9}ixer el 1709.");
    let tagged = sentences[0]
        .0
        .tokens()
        .iter()
        .any(|t| t.has_pos_tag("NPMSSP0"));
    assert!(
        tagged,
        "the new multiwords.txt line 'Franz Xaver Richter;NPMSSP0' must add its reading"
    );
}
