use super::entity_resolution::expand_lt_entities;
use crate::pt::example_suite::{
    apply_suite_filter, collect_style_antipattern_examples, strip_xml_comments_pub, suite_engine,
    suite_synthesizer,
};
use crate::pt::filters::{is_pt_filter, pt_rules_dir};
use crate::pt::test_support::PortuguesePipeline;
use og_core::AnalyzedTokenReadings;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, XmlCompiler};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

fn stage_predelta(commit: &str, relative: &str, dest: &Path) {
    let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
    let output = std::process::Command::new("git")
        .current_dir(&lt_root)
        .args(["show", &format!("{commit}:{relative}")])
        .output()
        .unwrap_or_else(|e| panic!("git show {commit}:{relative}: {e}"));
    assert!(
        output.status.success(),
        "git show {commit}:{relative} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(dest, output.stdout).expect("stage pre-delta xml");
}

fn predelta_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("pt-refresh-predelta");
    std::fs::create_dir_all(&dir).expect("predelta staging dir");
    dir
}

fn compile_expanded(path: &Path) -> Vec<CompiledRule> {
    let xml = std::fs::read_to_string(path).expect("rules xml");
    let expanded = expand_lt_entities(&xml);
    let decl = regex::Regex::new(r#"<!ENTITY\s+%\s+(\S+)\s+SYSTEM"#).unwrap();
    for cap in decl.captures_iter(&xml) {
        assert!(
            !expanded.contains(&format!("%{};", &cap[1])),
            "entity %{}; must expand for {}",
            &cap[1],
            path.display()
        );
    }
    XmlCompiler::new()
        .compile_file_with_base(&expanded, path.parent())
        .unwrap_or_else(|e| panic!("compile {}: {e}", path.display()))
        .rules
}

fn group_rules(rules: &[CompiledRule]) -> Vec<Vec<&CompiledRule>> {
    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&CompiledRule>> = Vec::new();
    for rule in rules {
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

fn main_example_failures(
    rules: &[CompiledRule],
    engine: &PatternRuleEngine,
    synthesizer: Option<&og_rules::postponed_adjective::LangSynthesizer>,
    pipeline: &PortuguesePipeline,
) -> BTreeSet<String> {
    let mut failures = BTreeSet::new();
    for group in group_rules(rules) {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                let pt_filter = aux.filter.as_ref().filter(|f| is_pt_filter(&f.class));
                for (sentence, sentence_start) in pipeline.analyze_offset_text(&clean_text) {
                    let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
                    for m in engine.match_rule(aux, &sentence) {
                        let Some(mut m) = (match pt_filter {
                            Some(filter) => {
                                apply_suite_filter(aux, filter, &sentence, &tokens, synthesizer, m)
                            }
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
            let failure: Option<&str> = match example.example_type {
                XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some("no match")
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_end = first.offset() + first.length();
                        if first.offset() <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some("marker miss")
                        }
                    } else {
                        None
                    }
                }
                XmlExampleType::Correct => {
                    if matches.is_empty() {
                        None
                    } else {
                        Some("false positive")
                    }
                }
                XmlExampleType::TriggersError => None,
            };
            if let Some(kind) = failure {
                failures.insert(format!("[{}] {} '{}'", rule.id, kind, clean_text));
            }
        }
    }
    failures
}

fn antipattern_failures(
    path: &Path,
    rules: &[CompiledRule],
    engine: &PatternRuleEngine,
    pipeline: &PortuguesePipeline,
) -> BTreeSet<String> {
    let xml = std::fs::read_to_string(path).expect("rules xml");
    let antipattern = collect_style_antipattern_examples(&strip_xml_comments_pub(&xml));
    let mut failures = BTreeSet::new();
    for ((id, sub_id), texts) in &antipattern.by_key {
        let targets: Vec<&CompiledRule> = rules
            .iter()
            .filter(|r| &r.id == id && &r.sub_id == sub_id)
            .collect();
        if targets.is_empty() {
            continue;
        }
        for text in texts {
            let mut matches = 0usize;
            for aux in &targets {
                for (sentence, _) in pipeline.analyze_offset_text(text) {
                    matches += engine.match_rule(aux, &sentence).len();
                }
            }
            if matches > 0 {
                failures.insert(format!(
                    "[{}/{} ANTIPATTERN] '{}'",
                    id,
                    sub_id.as_deref().unwrap_or("-"),
                    text
                ));
            }
        }
    }
    failures
}

fn evaluate_file(path: &Path, with_antipatterns: bool) -> BTreeSet<String> {
    let rules = compile_expanded(path);
    let engine = suite_engine();
    let synthesizer = suite_synthesizer();
    let pipeline = PortuguesePipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut failures = main_example_failures(&rules, &engine, synthesizer.as_ref(), &pipeline);
    if with_antipatterns {
        failures.extend(antipattern_failures(path, &rules, &engine, &pipeline));
    }
    failures
}

fn report_delta(label: &str, current: &BTreeSet<String>, predelta: &BTreeSet<String>) {
    let fresh: Vec<&String> = current.difference(predelta).collect();
    let resolved: Vec<&String> = predelta.difference(current).collect();
    eprintln!(
        "{label}: current {} failures, pre-delta {}",
        current.len(),
        predelta.len()
    );
    eprintln!(
        "{label}: {} new failures introduced by the refreshed xml:",
        fresh.len()
    );
    for line in &fresh {
        eprintln!("  NEW {line}");
    }
    eprintln!(
        "{label}: {} pre-delta failures resolved by the refreshed xml:",
        resolved.len()
    );
    for line in &resolved {
        eprintln!("  GONE {line}");
    }
}

#[test]
#[ignore]
fn predelta_failure_movement_is_attributed() {
    let _release_guard = crate::release_guard();
    let rules_dir = pt_rules_dir().expect("pt rules dir");
    let staging = predelta_dir();
    let cases: &[(&str, &str, &str, bool)] = &[
        (
            "grammar.xml",
            "b650fda3881",
            "languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/grammar.xml",
            false,
        ),
        (
            "style.xml",
            "1eefa2dc9c0",
            "languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/style.xml",
            true,
        ),
    ];
    let mut resolved_total = 0usize;
    for (file, commit, relative, with_antipatterns) in cases {
        let staged = staging.join(file);
        stage_predelta(commit, relative, &staged);
        let current = evaluate_file(&rules_dir.join(file), *with_antipatterns);
        let predelta = evaluate_file(&staged, *with_antipatterns);
        report_delta(file, &current, &predelta);
        let fresh: Vec<&String> = current.difference(&predelta).collect();
        assert!(
            fresh.is_empty(),
            "{file}: the refreshed xml may not introduce new suite failures: {fresh:?}"
        );
        let resolved: Vec<String> = predelta
            .difference(&current)
            .map(std::string::ToString::to_string)
            .collect();
        let expected: &[&str] = match *file {
            "grammar.xml" => &["[PT_MULTITOKEN_SPELLING_TWO] no match 'Michael Jodan.'"],
            _ => &[],
        };
        let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            resolved, expected,
            "{file}: pre-delta failures resolved by the refreshed xml must equal the pin"
        );
        resolved_total += resolved.len();
    }
    assert_eq!(resolved_total, 1);
}

const STYLE_BASELINE_COMMIT: &str = "1eefa2dc9c0";
const PT_RESOURCE_PREFIX: &str =
    "languagetool-language-modules/pt/src/main/resources/org/languagetool/resource/pt";

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("mkdir");
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let to = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy");
        }
    }
}

fn stage_predelta_resource() -> PathBuf {
    let live = crate::pt::tagger::pt_resource_dir().expect("pt resource dir");
    let staged = std::env::temp_dir().join("pt-refresh-predelta-resource");
    let _ = std::fs::remove_dir_all(&staged);
    copy_dir(&live, &staged);
    for file in [
        "added.txt",
        "disambiguation.xml",
        "multiwords.txt",
        "spelling.txt",
        "post-reform-compounds-inactive.txt",
        "post-reform-compounds.txt",
        "pre-reform-compounds.txt",
        "entities/english.ent",
        "entities/misc.ent",
    ] {
        stage_predelta(
            STYLE_BASELINE_COMMIT,
            &format!("{PT_RESOURCE_PREFIX}/{file}"),
            &staged.join(file),
        );
    }
    staged
}

fn analyze_with_predelta_data(
    tokenizer: &crate::pt::word_tokenizer::PortugueseWordTokenizer,
    tagger: &crate::pt::tagger::PortugueseTagger,
    disambiguator: Option<&std::sync::Arc<dyn og_core::checker::Disambiguator>>,
    splitter: &og_tokenizer::srx::SrxSentenceSplitter,
    text: &str,
) -> Vec<og_core::AnalyzedSentence> {
    use og_core::checker::WordTokenizer as _;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::SentenceTokenizer as _;
    splitter
        .split(text)
        .iter()
        .map(|sent| {
            let (start, end) = (sent.start(), sent.end());
            let sentence_text = &text[start..end];
            let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
            let mut tokens = vec![AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            )];
            tokens.extend(tokenizer.tokenize(sentence_text, 0));
            sentence.set_tokens(tokens);
            let token_texts: Vec<String> = sentence
                .tokens()
                .iter()
                .skip(1)
                .map(|t| t.token().token().to_string())
                .collect();
            let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
            let tagged = og_tagger::Tagger::tag(tagger, &refs);
            let tokens_mut = sentence.tokens_mut();
            for (i, atr) in tagged.into_iter().enumerate() {
                let token_idx = i + 1;
                if token_idx >= tokens_mut.len() {
                    continue;
                }
                let tagged_chunks = atr.chunks().to_vec();
                let readings = atr.into_readings();
                if !readings.is_empty() {
                    let primary = tokens_mut[token_idx].token().clone();
                    let all_tags: Vec<String> = readings
                        .iter()
                        .flat_map(|r| r.pos_tags().to_vec())
                        .collect();
                    let updated_primary =
                        AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                            .with_pos_tags(all_tags)
                            .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                    let mut merged =
                        AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
                    if !tagged_chunks.is_empty() {
                        merged.set_chunks(tagged_chunks);
                    }
                    tokens_mut[token_idx] = merged;
                } else if !tagged_chunks.is_empty() {
                    tokens_mut[token_idx].set_chunks(tagged_chunks);
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
            if let Some(disambiguator) = disambiguator {
                disambiguator.disambiguate(&mut sentence);
            }
            sentence
        })
        .collect()
}

const ZAARPRESS_SENTENCE: &str = "Antes de uma cirurgia e anestesia, informe ao seu médico (ou dentista) que está em tratamento com Zaarpress HCT, pois pode ocorrer queda repentina da pressão arterial associada à anestesia.";

fn reading_sets(sentences: &[og_core::AnalyzedSentence]) -> Vec<(String, Vec<String>)> {
    sentences
        .iter()
        .flat_map(|s| s.tokens().iter())
        .filter(|t| !t.is_whitespace())
        .map(|t| {
            (
                t.token().token().to_string(),
                t.readings()
                    .iter()
                    .map(|r| r.pos_tags().join("|"))
                    .collect(),
            )
        })
        .collect()
}

#[test]
#[ignore]
fn zaarpress_style_antipattern_movement_is_engine_side() {
    let _release_guard = crate::release_guard();
    let staged = stage_predelta_resource();
    let old_tagger = std::sync::Arc::new(crate::pt::tagger::PortugueseTagger::from_resource_dir(
        &staged,
    ));
    let old_disamb = crate::pt::build_pt_disambiguator(&staged);
    let old_tokenizer =
        crate::pt::word_tokenizer::PortugueseWordTokenizer::new(std::sync::Arc::new({
            let tagger = old_tagger.clone();
            move |w: &str| tagger.is_word_tagged(w)
        }));
    let splitter = og_tokenizer::srx::srx_splitter("pt").expect("pt srx splitter");
    let old = analyze_with_predelta_data(
        &old_tokenizer,
        &old_tagger,
        old_disamb.as_ref(),
        &splitter,
        ZAARPRESS_SENTENCE,
    );
    let new = PortuguesePipeline::shared().analyze_text(ZAARPRESS_SENTENCE);
    let old_readings = reading_sets(&old);
    let new_readings = reading_sets(&new);
    let diffs: Vec<String> = old_readings
        .iter()
        .zip(new_readings.iter())
        .filter(|(o, n)| o != n)
        .map(|(o, n)| format!("{o:?} -> {n:?}"))
        .collect();
    assert_eq!(
        diffs,
        vec![r#"("uma", ["DI0FS0"]) -> ("uma", ["DI0FS0", "Z0FS0"])"#.to_string()],
        "the data delta must move exactly one reading in this sentence, far from the match window"
    );
    let style_path = pt_rules_dir().expect("pt rules dir").join("style.xml");
    let rules = compile_expanded(&style_path);
    let target = rules
        .iter()
        .find(|r| r.id == "QUE_ESTA_ESTAO_EM")
        .expect("QUE_ESTA_ESTAO_EM");
    let engine = suite_engine();
    let old_matches: usize = old.iter().map(|s| engine.match_rule(target, s).len()).sum();
    let new_matches: usize = new.iter().map(|s| engine.match_rule(target, s).len()).sum();
    assert_eq!(
        old_matches, new_matches,
        "the rule must match identically under pre-delta and current data"
    );
    assert_eq!(new_matches, 1);
    let e1_fingerprint = target.antipatterns.iter().any(|anti| {
        anti.tokens
            .iter()
            .any(|t| t.postag.as_deref() == Some("_PUNCT") && t.compiled_postag_regexp.is_some())
    });
    assert!(
        e1_fingerprint,
        "the veto's postag='_PUNCT' postag_regexp='no' element must stay the documented E1 \
         shared-xml fingerprint: the match surfaces only because this element is dead"
    );
}
