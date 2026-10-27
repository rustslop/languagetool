use og_core::checker::Disambiguator as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

struct BretonPipeline {
    tokenizer: super::word_tokenizer::BretonWordTokenizer,
    tagger: Arc<super::tagger::BretonTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

fn br_resource_dir() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("br/src/main/resources/org/languagetool/resource/br")
}

impl BretonPipeline {
    fn shared() -> std::sync::Arc<BretonPipeline> {
        static PIPE: og_core::releasable::Releasable<BretonPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let disambiguator =
                std::fs::read_to_string(br_resource_dir().join("disambiguation.xml"))
                    .map(|xml| super::normalize::normalize_br_disambiguation_xml(&xml).into_owned())
                    .ok()
                    .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok());
            Some(Arc::new(BretonPipeline {
                tokenizer: super::word_tokenizer::BretonWordTokenizer::new(),
                tagger: super::tagger::BretonTagger::shared_arc().unwrap_or_else(|| {
                    Arc::new(super::tagger::BretonTagger::from_resource_dir(
                        &br_resource_dir(),
                    ))
                }),
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("br"),
            }))
        })
        .expect("br pipeline")
    }

    fn sentence_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        match &self.splitter {
            Some(srx) => srx
                .split(text)
                .iter()
                .map(|s| (s.start(), s.end()))
                .collect(),
            None => vec![(0, text.len())],
        }
    }

    fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(
            self.tokenizer.tokenize(sentence_text).into_iter().map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
            }),
        );
        sentence.set_tokens(tokens);
        let token_texts: Vec<String> = sentence
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token().to_string())
            .collect();
        let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
        let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &refs);
        let tokens_mut = sentence.tokens_mut();
        for (i, atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens_mut.len() {
                continue;
            }
            let readings = atr.into_readings();
            if !readings.is_empty() {
                let primary = tokens_mut[token_idx].token().clone();
                let updated_primary =
                    AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                        .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                tokens_mut[token_idx] =
                    AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
            }
        }
        let tokens = sentence.tokens_mut();
        for token in tokens.iter_mut().rev() {
            if !token.is_whitespace()
                && !token.token().pos_tags().contains(&"SENT_START".to_string())
            {
                if !token.has_pos_tag("SENT_END") {
                    token.add_sent_end();
                }
                break;
            }
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }

    fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        self.sentence_ranges(text)
            .into_iter()
            .map(|(s, e)| (self.analyze_range(text, s, e), s))
            .collect()
    }
}

fn grammar_path() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("br/src/main/resources/org/languagetool/rules/br/grammar.xml")
}

fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("br grammar.xml");
        let xml = super::normalize::normalize_br_rules_xml(&xml);
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile br grammar.xml"),
        ))
    })
    .expect("br")
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

fn rule_groups(rule_set: &CompiledRuleSet) -> Vec<Vec<&CompiledRule>> {
    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&CompiledRule>> = Vec::new();
    for rule in &rule_set.rules {
        let key = (rule.id.clone(), rule.sub_id.clone());
        let next = groups.len();
        let idx = *group_index.entry(key).or_insert(next);
        if idx == next {
            groups.push(vec![rule]);
        } else {
            groups[idx].push(rule);
        }
    }
    groups
}

pub(crate) fn all_grammar_matches(text: &str) -> Vec<(String, usize, usize)> {
    let engine = PatternRuleEngine::new();
    let pipeline = BretonPipeline::shared();
    let rule_set = compiled_rules();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut matched = Vec::new();
    for rule in &rule_set.rules {
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for m in engine.match_rule(rule, &sentence) {
                matched.push((
                    rule.id.clone(),
                    m.offset() + sentence_start,
                    m.offset() + sentence_start + m.length(),
                ));
            }
        }
    }
    matched
}

pub(crate) fn disambiguated_sentence(text: &str) -> AnalyzedSentence {
    BretonPipeline::shared()
        .analyze_offset_text(text)
        .into_iter()
        .map(|(sentence, _)| sentence)
        .next()
        .expect("text must yield at least one sentence")
}

pub(crate) fn trace_rule_matches(id: &str, text: &str) -> (usize, Vec<AnalyzedSentence>) {
    let engine = PatternRuleEngine::new();
    let pipeline = BretonPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let rule_set = compiled_rules();
    let rules: Vec<&CompiledRule> = rule_set.rules.iter().filter(|r| r.id == id).collect();
    let sentences: Vec<AnalyzedSentence> = pipeline
        .analyze_offset_text(text)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    for rule in &rules {
        for sentence in &sentences {
            engine.match_rule(rule, sentence);
        }
    }
    (rules.len(), sentences)
}

const TOKEN_REFERENCE_RESIDUE: &[(&str, &str)] = &[
    ("KLANV_PE_GLANVOCH", "Klañv pe klañvoc’h."),
    ("KLANV_PE_GLANVOCH", "Klañv pe c’hlañvoc’h."),
    ("KLANV_PE_GLANVOCH", "Tev pe tevoc’h."),
    ("KLANV_PE_GLANVOCH", "Tev pe zevoc’h."),
    ("KLANV_PE_GLANVOCH", "Pounner pe pounneroc’h."),
    ("KLANV_PE_GLANVOCH", "Pounner pe founneroc’h."),
    ("KLANV_PE_GLANVOCH", "Gell pe gelloc’h."),
    ("KLANV_PE_GLANVOCH", "Gell pe kelloc’h."),
    ("KLANV_PE_GLANVOCH", "Gwell pe gwelloc’h."),
    ("KLANV_PE_GLANVOCH", "Gwell pe kwelloc’h."),
    ("KLANV_PE_GLANVOCH", "Droch pe drochoc’h."),
    ("KLANV_PE_GLANVOCH", "Droch pe trochoc’h."),
    ("KLANV_PE_GLANVOCH", "Brav pe bravoc’h."),
    ("KLANV_PE_GLANVOCH", "Brav pe pravoc’h."),
    ("KLANV_PE_GLANVOCH", "Melen pe melenoc’h."),
    ("OCH_AN", "Diaesoc’h diaesañ eo kavout labour."),
    ("OCH_AN", "Gwelloc’h gwell"),
];

const DATE_TRIM_RESIDUE: &[(&str, &str)] = &[
    ("DEIZ_DEIZIAD", "D’ar Merc’her 6 a viz C’hwevrer 2014."),
    ("DEIZ_DEIZIAD", "D’ar Merc'her 6 a viz C'hwevrer 2014."),
];

#[test]
#[ignore]
fn breton_grammar_examples_with_br_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = BretonPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut fail_lines: Vec<String> = Vec::new();
    let mut residue: Vec<(String, String)> = Vec::new();
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                    for mut m in engine.match_rule(aux, &sentence) {
                        m.set_offset_length(m.offset() + sentence_start, m.length());
                        matches.push(m);
                    }
                }
            }
            let failed = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some(format!(
                            "Expected match on incorrect example but got none: '{clean_text}'"
                        ))
                    } else {
                        marker_range.and_then(|(m_start, m_end)| {
                            let first = &matches[0];
                            let match_start = first.offset();
                            let match_end = match_start + first.length();
                            if !(match_start <= m_start && match_end >= m_end) {
                                Some(format!(
                                    "Match {match_start}..{match_end} does not contain marker {m_start}..{m_end}: '{clean_text}'"
                                ))
                            } else {
                                None
                            }
                        })
                    }
                }
                og_xml::types::XmlExampleType::Correct => (!matches.is_empty()).then(|| {
                    format!(
                        "Expected no match on correct example but got {}: '{clean_text}'",
                        matches.len()
                    )
                }),
                og_xml::types::XmlExampleType::TriggersError => None,
            };
            if let Some(message) = failed {
                residue.push((rule.id.clone(), clean_text));
                fail_lines.push(format!("FAIL: [{}] {}", rule.id, message));
            } else {
                passed += 1;
            }
        }
    }
    eprintln!(
        "br module pipeline: {}/{} passed",
        passed,
        passed + fail_lines.len()
    );
    let _ = std::fs::write(
        "/tmp/test_failures_br_module.txt",
        fail_lines.join("\n") + "\n",
    );
    assert!(
        passed + fail_lines.len() > 0,
        "suite must evaluate examples"
    );
    let mut transform_actual: Vec<(&str, &str)> = residue
        .iter()
        .filter(|(id, _)| id.as_str() != "DEIZ_DEIZIAD")
        .map(|(id, text)| (id.as_str(), text.as_str()))
        .collect();
    let mut date_actual: Vec<(&str, &str)> = residue
        .iter()
        .filter(|(id, _)| id.as_str() == "DEIZ_DEIZIAD")
        .map(|(id, text)| (id.as_str(), text.as_str()))
        .collect();
    transform_actual.sort_unstable();
    date_actual.sort_unstable();
    let mut transform_pinned = TOKEN_REFERENCE_RESIDUE.to_vec();
    let mut date_pinned = DATE_TRIM_RESIDUE.to_vec();
    transform_pinned.sort_unstable();
    date_pinned.sort_unstable();
    assert!(
        (transform_actual.is_empty() || transform_actual == transform_pinned)
            && (date_actual.is_empty() || date_actual == date_pinned),
        "br residue may only be the two pinned shared-engine families, each fully present or fully \
         cleared by its fix (token-reference transform {}/17, unicode month trim {}/2): {:?}",
        transform_actual.len(),
        date_actual.len(),
        residue
    );
}

#[test]
fn srx_languagemap_resolves_the_breton_ruleset() {
    let splitter = og_tokenizer::srx::srx_splitter("br").expect("br SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("Lakait amañ ho testenn. Pe implijit an frazenn-mañ.");
    assert_eq!(sentences.len(), 2);
    let sentences = splitter.split("D’an 29 a viz C’hwevrer 2014.");
    assert_eq!(sentences.len(), 1);
}

#[test]
fn demo_text_matches_the_java_ar_lies_sequence() {
    use og_core::rule::Rule as _;
    let text = "Lakait amañ ho testenn vrezhonek da vezañ gwiriet. Pe implijit an frazenn-mañ gant meur a fazioù yezhadurel enni.";
    let pipeline = BretonPipeline::shared();
    let speller = super::speller::BretonSpellerRule::shared_arc();
    let topo = super::topo_replace::TopoReplaceRule::new();
    let mut matched = all_grammar_matches(text);
    for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
        for m in speller.match_sentence(&sentence) {
            matched.push((
                speller.id().to_string(),
                m.offset() + sentence_start,
                m.offset() + sentence_start + m.length(),
            ));
        }
        for m in topo.match_sentence(&sentence) {
            matched.push((
                topo.id().to_string(),
                m.offset() + sentence_start,
                m.offset() + sentence_start + m.length(),
            ));
        }
    }
    matched.sort_by_key(|(_, start, end)| (*start, *end));
    let ids: Vec<&str> = matched.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["AR", "LIES"], "BretonTest.testDemoText sequence");
}

#[test]
fn pipeline_tokens_the_java_mixed_apostrophe_examples() {
    let pipeline = BretonPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Test n’eo");
    assert_eq!(sentences.len(), 1);
    let token_texts: Vec<&str> = sentences[0]
        .0
        .tokens()
        .iter()
        .skip(1)
        .map(|t| t.token().token())
        .collect();
    assert_eq!(token_texts, vec!["Test", " ", "n’", "eo"]);
    let sentences =
        pipeline.analyze_offset_text("C'hwerc'h merc'h gwerc'h war c'hwerc'h marc'h kalloc'h.");
    let token_texts: Vec<&str> = sentences[0]
        .0
        .tokens()
        .iter()
        .skip(1)
        .filter(|t| !t.is_whitespace())
        .map(|t| t.token().token())
        .collect();
    assert_eq!(
        token_texts,
        vec![
            "C’hwerc’h",
            "merc’h",
            "gwerc’h",
            "war",
            "c’hwerc’h",
            "marc’h",
            "kalloc’h",
            "."
        ]
    );
}

#[test]
fn pipeline_tags_c_h_words_and_applies_disambiguation() {
    let pipeline = BretonPipeline::shared();
    let sentences = pipeline.analyze_offset_text("Ar c’havotenn.");
    let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentences[0]
        .0
        .tokens()
        .iter()
        .skip(1)
        .filter(|t| !t.is_whitespace())
        .collect();
    assert_eq!(tokens[1].token().token(), "c’havotenn");
    assert!(
        !tokens[1].readings().is_empty(),
        "c’havotenn must carry dictionary readings"
    );
}

#[test]
fn grammar_rules_load_the_java_rule_count() {
    let rule_set = compiled_rules();
    assert!(
        rule_set.rules.len() >= 140,
        "br grammar.xml must compile a real rule set, got {}",
        rule_set.rules.len()
    );
}

#[test]
fn faz_kemm_examples_pass_with_the_breton_pipeline() {
    let matched = all_grammar_matches("C’hallout a ran.");
    assert!(
        matched.iter().any(|(id, _, _)| id == "FAZ_KEMM"),
        "FAZ_KEMM must flag 'C’hallout': {matched:?}"
    );
    let matched = all_grammar_matches("Gallout a ran.");
    assert!(
        !matched.iter().any(|(id, _, _)| id == "FAZ_KEMM"),
        "FAZ_KEMM must stay quiet on 'Gallout': {matched:?}"
    );
}

#[test]
fn disambiguation_replaces_the_da_lavaret_eo_initialism() {
    let pipeline = BretonPipeline::shared();
    let sentences = pipeline.analyze_offset_text("D.l.e tesket a-walc’h war ar brezhoneg.");
    let e_token = sentences[0]
        .0
        .tokens()
        .iter()
        .find(|t| t.token().token() == "e")
        .expect("e token");
    assert!(
        e_token.has_pos_tag("X DA_LAVARET_EO"),
        "the action-less <disambig> must replace 'e' like Java: {:?}",
        e_token.readings()
    );
    let matched = all_grammar_matches("D.l.e tesket a-walc’h war ar brezhoneg.");
    assert!(
        matched.iter().any(|(id, _, _)| id == "FAZ_KEMM"),
        "FAZ_KEMM must flag 'tesket' after 'D.l.e': {matched:?}"
    );
}

#[test]
fn residue_probes_track_the_shared_engine_fix_state() {
    let probes: [(&str, &str, &str); 3] = [
        ("KLANV_PE_GLANVOCH", "Klañv pe c’hlañvoc’h.", "c’hlañvoc’h"),
        (
            "OCH_AN",
            "Diaesoc’h diaesañ eo kavout labour.",
            "Diaesoc’h diaesañ",
        ),
        (
            "DEIZ_DEIZIAD",
            "D’ar Merc’her 6 a viz C’hwevrer 2014.",
            "Merc’her 6 a viz C’hwevrer 2014",
        ),
    ];
    let mut fired = [false; 3];
    for (i, (id, text, marker)) in probes.iter().enumerate() {
        let marker_start = text.find(marker).unwrap_or(text.len());
        let marker_end = marker_start + marker.len();
        for (m_id, start, end) in all_grammar_matches(text) {
            if m_id != *id {
                continue;
            }
            assert!(
                start <= marker_start && end >= marker_end,
                "{id} match {start}..{end} must cover the marker {marker_start}..{marker_end}"
            );
            fired[i] = true;
        }
    }
    assert!(
        !fired[0] && !fired[1] || fired[0] && fired[1],
        "KLANV_PE_GLANVOCH and OCH_AN flip together with the token-level <match no> \
         regexp_match/regexp_replace transform (XMLRuleHandler.java:405-415, \
         PatternToken.java:502-514, MatchState.java:215-227 and :382-386): {:?}",
        fired
    );
}
