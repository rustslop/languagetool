use crate::pt::example_suite::{apply_suite_filter, suite_engine, suite_synthesizer};
use crate::pt::filters::{is_pt_filter, pt_rules_dir};
use crate::pt::tagger::PortugueseTagger;
use crate::pt::test_support::PortuguesePipeline;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_xml::compiler::{CompiledRule, XmlCompiler};
use std::path::{Path, PathBuf};

fn rules_dir() -> PathBuf {
    pt_rules_dir().expect("pt rules dir")
}

fn compile_rules_file(path: &Path) -> Vec<CompiledRule> {
    let xml = std::fs::read_to_string(path).expect("rules xml");
    XmlCompiler::new()
        .compile_file_with_base(&xml, path.parent())
        .expect("compile rules xml")
        .rules
}

fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

fn check_rule_examples(path: &Path, rule_id: &str) -> Vec<String> {
    let rules = compile_rules_file(path);
    let targets: Vec<&CompiledRule> = rules.iter().filter(|r| r.id == rule_id).collect();
    assert!(
        !targets.is_empty(),
        "{rule_id} must be compiled from {}",
        path.display()
    );
    let engine = suite_engine();
    let synthesizer = suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut failures = Vec::new();
    for rule in &targets {
        let pt_filter = rule.filter.as_ref().filter(|f| is_pt_filter(&f.class));
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in &targets {
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                    let tokens: Vec<_> = sentence.non_whitespace_tokens();
                    for m in engine.match_rule(aux, &sentence) {
                        let Some(mut m) = (match pt_filter {
                            Some(filter) => apply_suite_filter(
                                aux,
                                filter,
                                &sentence,
                                &tokens,
                                synthesizer.as_ref(),
                                m,
                            ),
                            None => Some(m),
                        }) else {
                            continue;
                        };
                        m.set_offset_length(m.offset() + sentence_start, m.length());
                        matches.push(m);
                    }
                }
            }
            use og_xml::types::XmlExampleType;
            let failure: Option<String> = match example.example_type {
                XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some(format!("no match on incorrect example '{clean_text}'"))
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_end = first.offset() + first.length();
                        if first.offset() <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some(format!(
                                "match {}..{match_end} misses marker {m_start}..{m_end} on '{clean_text}'",
                                first.offset()
                            ))
                        }
                    } else {
                        None
                    }
                }
                XmlExampleType::Correct => {
                    if matches.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "expected no match on correct example but got {} on '{clean_text}'",
                            matches.len()
                        ))
                    }
                }
                XmlExampleType::TriggersError => None,
            };
            if let Some(message) = failure {
                failures.push(format!("[{rule_id}] {message}"));
            }
        }
    }
    failures
}

fn synthetic_sentence(tokens: &[(&str, &str, &[&str])]) -> AnalyzedSentence {
    let mut offset = 0usize;
    let mut analyzed = Vec::new();
    for &(text, lemma, postags) in tokens {
        let start = offset;
        let end = offset + text.len();
        let base = AnalyzedToken::new(text, start, end).with_lemma(lemma);
        let readings: Vec<AnalyzedToken> = postags
            .iter()
            .map(|pos| {
                AnalyzedToken::new(text, start, end)
                    .with_pos_tags(vec![pos.to_string()])
                    .with_lemma(lemma)
            })
            .collect();
        analyzed.push(AnalyzedTokenReadings::new(base).with_readings(readings));
        offset = end + 1;
    }
    let mut sentence = AnalyzedSentence::new("synthetic", 0, offset.saturating_sub(1));
    sentence.set_tokens(analyzed);
    sentence
}

fn postags_of(sentence: &AnalyzedSentence, surface: &str) -> Vec<String> {
    sentence
        .tokens()
        .iter()
        .filter(|t| t.token().token() == surface)
        .flat_map(|t| t.readings().iter().map(|r| r.pos_tags().join("|")))
        .collect()
}

fn shared_disambiguator() -> og_tagger::XmlDisambiguator {
    let resource_dir = crate::pt::tagger::pt_resource_dir().expect("pt resource dir");
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&xml, Some(&resource_dir));
    og_tagger::XmlDisambiguator::from_xml(&expanded).expect("compile disambiguation.xml")
}

fn apply_through_rule(
    disambig: &og_tagger::XmlDisambiguator,
    rule_id: &str,
    tokens: &[(&str, &str, &[&str])],
) -> (Vec<String>, Vec<String>) {
    let idx = disambig
        .rule_ids()
        .iter()
        .position(|id| *id == rule_id)
        .unwrap_or_else(|| panic!("{rule_id} must be compiled from disambiguation.xml"));
    let mut before = synthetic_sentence(tokens);
    disambig.apply_rules_through(&mut before, idx);
    let mut after = synthetic_sentence(tokens);
    disambig.apply_rules_through(&mut after, idx + 1);
    (postags_of(&before, "target"), postags_of(&after, "target"))
}

#[test]
fn refreshed_example_totals_pin_the_java_delta() {
    for (file, expected) in [
        ("grammar.xml", 8123usize),
        ("style.xml", 3425),
        ("pt-PT/grammar.xml", 1088),
    ] {
        let xml = std::fs::read_to_string(rules_dir().join(file)).expect("rules xml");
        assert_eq!(
            xml.matches("<example").count(),
            expected,
            "{file} example total must match the refreshed Java refs (grammar 8101->8123, style 3355->3425, pt-PT +13)"
        );
    }
}

#[test]
fn new_disambiguation_rules_are_compiled() {
    let disambig = shared_disambiguator();
    let ids = disambig.rule_ids();
    for id in [
        "SE_VMIP3S0_VMM02S0_20261021",
        "SURGIR_INDICATIVO_3PPC_COM_CLITICO_20261002",
        "PI_NC_V_PPC_20260928",
        "PI_NC_V_SPS00PP_20260928",
    ] {
        assert!(
            ids.contains(&id),
            "{id} must be compiled from the refreshed disambiguation.xml"
        );
    }
}

#[test]
fn se_vmip3s0_vmm02s0_removes_the_imperative_after_clitic_se() {
    let disambig = shared_disambiguator();
    let (before, after) = apply_through_rule(
        &disambig,
        "SE_VMIP3S0_VMM02S0_20261021",
        &[
            ("ela", "ele", &["PP3FS000"][..]),
            ("se", "se", &["PP3CS000"][..]),
            ("target", "vestir", &["VMIP3S0", "VMM02S0"][..]),
        ],
    );
    assert_eq!(before, vec!["VMIP3S0", "VMM02S0"]);
    assert_eq!(
        after,
        vec!["VMIP3S0"],
        "the rule removes VMM02S0 from a token that keeps both VMIP3S0 and VMM02S0 after 'se'"
    );
    let sentences = PortuguesePipeline::shared()
        .analyze_text("Apesar de ser rica, ela se veste com bastante simplicidade.");
    let veste: Vec<String> = sentences
        .iter()
        .flat_map(|s| postags_of(s, "veste"))
        .collect();
    assert!(
        veste.iter().any(|t| t == "VMIP3S0"),
        "'veste' keeps VMIP3S0 through the full pipeline: {veste:?}"
    );
    assert!(
        !veste.iter().any(|t| t == "VMM02S0"),
        "the full pipeline drops the VMM02S0 reading of 'veste' after clitic 'se': {veste:?}"
    );
}

#[test]
fn surgir_rule_is_dict_excluded_but_mechanically_sound() {
    let tagger = PortugueseTagger::shared_arc().expect("tagger");
    let surge: Vec<String> = tagger
        .lookup("surge")
        .iter()
        .map(|(_, tag)| tag.to_string())
        .collect();
    assert!(
        surge.iter().any(|t| t == "VMIP3S0" || t == "VMM02S0"),
        "'surge' carries bare verb readings: {surge:?}"
    );
    assert!(
        !surge.iter().any(|t| t.contains(':')),
        "the pinned dict has no ':'-chained clitic tags for 'surge', so the rule's VMIP3S0:PP.C.+ pattern cannot match it (r26 dict enclitic family): {surge:?}"
    );
    let disambig = shared_disambiguator();
    let (before, after) = apply_through_rule(
        &disambig,
        "SURGIR_INDICATIVO_3PPC_COM_CLITICO_20261002",
        &[(
            "target",
            "surgir",
            &["VMIP3S0:PP3CSA00", "VMM02S0:PP3CSA00"],
        )],
    );
    assert_eq!(before, vec!["VMIP3S0:PP3CSA00", "VMM02S0:PP3CSA00"]);
    assert_eq!(
        after,
        vec!["VMIP3S0:PP3CSA00"],
        "with chained clitic tags the and-group matches and the rule strips VMM02S.*"
    );
}

#[test]
fn pi_nc_v_ppc_removes_verb_readings_and_honors_its_antipattern() {
    let disambig = shared_disambiguator();
    let tokens: &[(&str, &str, &[&str])] = &[
        ("alguém", "alguém", &["PI0FS000"]),
        ("target", "gosto", &["VMIP1S0", "NCMS000"]),
        ("eu", "eu", &["PP1CS000"]),
    ];
    let (before, after) = apply_through_rule(&disambig, "PI_NC_V_PPC_20260928", tokens);
    assert_eq!(before, vec!["VMIP1S0", "NCMS000"]);
    assert_eq!(
        after,
        vec!["NCMS000"],
        "PI + and(VMIP1S0|VMS.+, NC.+|AQ.+) + personal pronoun strips V.* from the middle token"
    );
    let blocked: &[(&str, &str, &[&str])] = &[
        (",", ",", &["_PUNCT"]),
        ("Quantos", "quanto", &["PI0MP000"]),
        ("como", "como", &["VMIP1S0", "NCMS000"]),
        ("eu", "eu", &["PP1CS000"]),
    ];
    let idx = disambig
        .rule_ids()
        .iter()
        .position(|id| *id == "PI_NC_V_PPC_20260928")
        .expect("rule idx");
    let mut before = synthetic_sentence(blocked);
    disambig.apply_rules_through(&mut before, idx);
    let mut after = synthetic_sentence(blocked);
    disambig.apply_rules_through(&mut after, idx + 1);
    assert_eq!(
        postags_of(&before, "como"),
        vec!["VMIP1S0", "NCMS000"],
        "earlier rules must leave the dual readings in place"
    );
    assert_eq!(
        postags_of(&after, "como"),
        vec!["VMIP1S0", "NCMS000"],
        "the SENT_START|_PUNCT + quant[ao]s? + como + eu antipattern must veto the overlapping match"
    );
}

fn dna_rulegroup_xml() -> String {
    let resource_dir = crate::pt::tagger::pt_resource_dir().expect("pt resource dir");
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let start = xml
        .find("<rulegroup id=\"DNA\"")
        .expect("DNA rulegroup start");
    let end = xml[start..]
        .find("</rulegroup>")
        .map(|e| start + e + "</rulegroup>".len())
        .expect("DNA rulegroup end");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<rules lang=\"pt\">{}</rules>\n",
        &xml[start..end]
    )
}

#[test]
fn dna_todos_ele_antipattern_gates_overlapping_matches() {
    let disambig = og_tagger::XmlDisambiguator::from_xml(&dna_rulegroup_xml())
        .expect("compile the extracted DNA rulegroup");
    let fire: &[(&str, &str, &[&str])] = &[
        ("todos", "todo", &["DI0MP0"]),
        ("target", "convidado", &["NCMP000"]),
        ("pacíficos", "pacífico", &["AQ0MP0"]),
    ];
    let mut firing = synthetic_sentence(fire);
    disambig.apply_rules_through(&mut firing, 2);
    let unified = postags_of(&firing, "target");
    let blocked: &[(&str, &str, &[&str])] = &[
        ("todos", "todo", &["DI0MP0"]),
        ("target", "convidado", &["PP3MP000", "NCMP000"]),
        ("pacíficos", "pacífico", &["AQ0MP0"]),
    ];
    let mut blocking = synthetic_sentence(blocked);
    disambig.apply_rules_through(&mut blocking, 2);
    let gated = postags_of(&blocking, "target");
    assert_eq!(
        unified,
        vec!["NCMP000"],
        "without a tod[ao]s? + PP.+ window the DNA unify applies"
    );
    assert_eq!(
        gated,
        vec!["PP3MP000", "NCMP000"],
        "the new todos-ele antipattern vetoes the DNA match when the middle token exposes a PP reading"
    );
}

#[test]
fn pi_nc_v_sps00pp_removes_verb_readings_under_chained_prepositions() {
    let disambig = shared_disambiguator();
    let (before, after) = apply_through_rule(
        &disambig,
        "PI_NC_V_SPS00PP_20260928",
        &[
            ("ninguém", "ninguém", &["PI000000"][..]),
            ("target", "gosto", &["VMS1S0", "NCMS000"][..]),
            ("nele", "em+ele", &["SPS00:PP3MS00"][..]),
        ],
    );
    assert_eq!(before, vec!["VMS1S0", "NCMS000"]);
    assert_eq!(
        after,
        vec!["NCMS000"],
        "PI + and(VM[PS].+, NC.+|AQ.+) + SPS00:PP.+ strips V.* from the middle token"
    );
    let tagger = PortugueseTagger::shared_arc().expect("tagger");
    let nele: Vec<String> = tagger
        .lookup("nele")
        .iter()
        .map(|(lemma, tag)| format!("{lemma}/{tag}"))
        .collect();
    assert!(
        !nele.iter().any(|r| r.contains("SPS00:PP")),
        "the pinned dict only plus-joins the clitic chain on 'nele', so the rule is dict-excluded on live data (r26 contraction family): {nele:?}"
    );
}

#[test]
fn bem_dizer_hifenizar_examples_behave() {
    let failures = check_rule_examples(&rules_dir().join("grammar.xml"), "BEM_DIZER_HIFENIZAR");
    assert!(failures.is_empty(), "{failures:?}");
    let grammar = compile_rules_file(&rules_dir().join("grammar.xml"));
    let rule = grammar
        .iter()
        .find(|r| r.id == "BEM_DIZER_HIFENIZAR")
        .expect("rule");
    assert!(rule.picky, "BEM_DIZER_HIFENIZAR carries tags='picky'");
}

#[test]
fn brasileirismo_ter_existencial_aqui_examples_behave() {
    let failures = check_rule_examples(
        &rules_dir().join("pt-PT/grammar.xml"),
        "BRASILEIRISMO_TER_EXISTENCIAL_AQUI",
    );
    assert!(failures.is_empty(), "{failures:?}");
    let pt_pt = compile_rules_file(&rules_dir().join("pt-PT/grammar.xml"));
    let rule = pt_pt
        .iter()
        .find(|r| r.id == "BRASILEIRISMO_TER_EXISTENCIAL_AQUI")
        .expect("rule");
    assert!(
        rule.picky,
        "BRASILEIRISMO_TER_EXISTENCIAL_AQUI carries tags='picky'"
    );
}

#[test]
fn new_and_rewritten_style_rules_examples_behave() {
    for id in [
        "CUMPRIR_COM_TUDO_O_QUE",
        "SEM_MARGEM_PARA_DUVIDAS",
        "ENSINOS_SUPERIORES",
        "QUEM_GANHA_COM",
        "USAR_UTILIZAR_POUCAS_VEZES",
        "APRESENTAR_OS_MELHORES_CUMPRIMENTOS",
        "PERDER_A_VIDA",
    ] {
        let failures = check_rule_examples(&rules_dir().join("style.xml"), id);
        assert!(failures.is_empty(), "{id}: {failures:?}");
    }
}

#[test]
fn mais_reduzida_inferior_is_now_a_plain_rule() {
    let style = compile_rules_file(&rules_dir().join("style.xml"));
    let hits: Vec<&CompiledRule> = style
        .iter()
        .filter(|r| r.id == "MAIS_REDUZIDA_INFERIOR")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "the delta rewrote the MAIS_REDUZIDA_INFERIOR rulegroup into a single top-level rule"
    );
    assert_eq!(
        hits[0].sub_id, None,
        "the rewritten rule is not a rulegroup sub-rule"
    );
    let failures = check_rule_examples(&rules_dir().join("style.xml"), "MAIS_REDUZIDA_INFERIOR");
    assert!(failures.is_empty(), "{failures:?}");
}
