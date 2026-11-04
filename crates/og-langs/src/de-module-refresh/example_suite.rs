use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use og_core::checker::Chunker as _;
use og_core::checker::Disambiguator as _;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_tagger::Tagger as _;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;
use og_xml::types::XmlExampleType;

pub(crate) fn de_resource_dir() -> PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("languagetool resource dir")
        .join("de/src/main/resources/org/languagetool/resource/de")
}

pub(crate) fn compiled_de_rule_set() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = crate::engine::find_lt_resource_dir()?
            .join("de/src/main/resources/org/languagetool/rules/de/grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("de grammar.xml readable");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("de grammar.xml compiles"),
        ))
    })
    .expect("de module-refresh rule set")
}

pub(crate) struct DeRefreshPipeline {
    tokenizer: og_tokenizer::GermanWordTokenizer,
    tagger: Arc<og_tagger::GermanTagger>,
    disambiguator: Option<Arc<og_tagger::XmlDisambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl DeRefreshPipeline {
    pub(crate) fn shared() -> Arc<Self> {
        static PIPE: og_core::releasable::Releasable<DeRefreshPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = de_resource_dir();
            let tagger = og_tagger::GermanTagger::from_resource_dir(&resource_dir).map(Arc::new)?;
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml_with_english(&xml, None).ok())
                .map(Arc::new);
            Some(Arc::new(Self {
                tokenizer: og_tokenizer::GermanWordTokenizer::new(),
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("de"),
            }))
        })
        .expect("de module-refresh pipeline")
    }

    fn tokenize_sentence(&self, text: &str) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens: Vec<AnalyzedTokenReadings> = self
            .tokenizer
            .tokenize(text)
            .into_iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end())))
            .collect();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        let token_texts: Vec<&str> = tokens.iter().skip(1).map(|t| t.token().token()).collect();
        let tagged = self.tagger.tag(&token_texts);
        for (i, tagged_atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens.len() {
                continue;
            }
            let start = tokens[token_idx].token().start();
            let end = tokens[token_idx].token().end();
            if tagged_atr.readings().is_empty() {
                let primary = AnalyzedToken::new(tagged_atr.token().token(), start, end);
                let mut next = AnalyzedTokenReadings::new(primary);
                if !tagged_atr.chunks().is_empty() {
                    next.set_chunks(tagged_atr.chunks().to_vec());
                }
                tokens[token_idx] = next;
                continue;
            }
            let tagged_chunks = tagged_atr.chunks().to_vec();
            let surface = tagged_atr.token().token().to_string();
            let readings = tagged_atr.into_readings();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings[0]
                .lemma()
                .map(str::to_string)
                .unwrap_or_else(|| surface.clone());
            let updated_primary = AnalyzedToken::new(surface, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut atr = AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
            if !tagged_chunks.is_empty() {
                atr.set_chunks(tagged_chunks);
            }
            tokens[token_idx] = atr;
        }
        sentence.set_tokens(tokens);
        let tokens = sentence.tokens_mut();
        for i in (0..tokens.len()).rev() {
            if !tokens[i].is_whitespace()
                && !tokens[i]
                    .token()
                    .pos_tags()
                    .contains(&"SENT_START".to_string())
            {
                if !tokens[i].has_pos_tag("SENT_END") {
                    tokens[i].add_sent_end();
                }
                break;
            }
        }
        if self.disambiguator.is_some() {
            sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        og_tagger::german_chunker::shared_german_chunker().add_chunk_tags(&mut sentence);
        sentence
    }

    fn analyze_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        match &self.splitter {
            Some(splitter) => splitter
                .split(text)
                .into_iter()
                .map(|s| (self.tokenize_sentence(s.text()), s.start()))
                .collect(),
            None => Vec::from([(self.tokenize_sentence(text), 0)]),
        }
    }
}

fn de_spelling_support(
    resource_dir: &Path,
    core_resource_dir: &Path,
) -> Option<og_rules::spelling_support::SpellingSupport> {
    let dictionary = og_tagger::german_tagger::shared_german_dictionary(resource_dir).map(|dict| {
        let mut d = og_spell::Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(dict),
        ));
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in words.lines() {
                for word in og_rules::spelling_support::prepare_line_for_speller(
                    og_rules::spelling_support::SpellerVariant::German,
                    line,
                ) {
                    d.add_word(&word);
                }
            }
        }
        d
    });
    let dictionary = dictionary.map(Arc::new);
    dictionary.map(|dictionary| {
        let mut support = og_rules::spelling_support::SpellingSupport::new_for_language(
            dictionary.clone(),
            core_resource_dir,
            resource_dir,
            "de",
        );
        if let Some(checker) = og_spell::hunspell::HunspellChecker::load(
            &resource_dir.join("hunspell/de_DE.aff"),
            &resource_dir.join("hunspell/de_DE.dic"),
        ) {
            let base = og_spell::GermanSpeller::new(
                dictionary,
                resource_dir,
                Some(core_resource_dir),
                None,
                "de-DE",
            );
            support = support
                .with_german_rule_speller(Arc::new(base.with_hunspell_core(Arc::new(checker))));
        }
        support
    })
}

pub(crate) fn de_pattern_engine() -> Arc<PatternRuleEngine> {
    static ENGINE: og_core::releasable::Releasable<PatternRuleEngine> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&ENGINE, || {
        let resource_dir = de_resource_dir();
        let core_resource_dir = crate::engine::find_lt_core_resource_dir()?;
        let support = de_spelling_support(&resource_dir, &core_resource_dir);
        let mut engine = match support {
            Some(support) => {
                let mut engine = PatternRuleEngine::new().with_spelling_support(Arc::new(support));
                if let Some(synth) = og_rules::postponed_adjective::LangSynthesizer::for_language(
                    &resource_dir,
                    "de",
                ) {
                    engine = engine.with_synthesizer(Arc::new(synth));
                }
                engine
            }
            None => {
                match og_rules::postponed_adjective::LangSynthesizer::for_language(
                    &resource_dir,
                    "de",
                ) {
                    Some(synth) => PatternRuleEngine::new().with_synthesizer(Arc::new(synth)),
                    None => PatternRuleEngine::new(),
                }
            }
        };
        if let Some(synth) =
            og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "de")
        {
            engine = engine.with_suggestion_synthesizer(Arc::new(synth));
        }
        Some(Arc::new(engine))
    })
    .expect("de module-refresh pattern engine")
}

fn strip_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
enum Outcome {
    NoMatch,
    FalsePositive,
    Marker,
}

type ExampleRecord = (String, String, Option<Outcome>);

fn evaluate_all() -> Vec<ExampleRecord> {
    use rayon::prelude::*;
    let rule_set = compiled_de_rule_set();
    let engine = de_pattern_engine();
    let pipeline = DeRefreshPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
    let mut groups: Vec<Vec<&og_xml::compiler::CompiledRule>> = Vec::new();
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

    let pairs: Vec<(
        &Vec<&og_xml::compiler::CompiledRule>,
        &og_xml::types::XmlExample,
    )> = groups
        .iter()
        .filter_map(|group| group.first().map(|rule| (group, rule)))
        .flat_map(|(group, rule)| rule.examples.iter().map(move |e| (group, e)))
        .collect();

    pairs
        .par_iter()
        .map(|(group, example)| {
            let rule = group[0];
            let (clean_text, marker_range) = strip_marker(&example.text);
            let mut matches = Vec::new();
            for aux in group.iter() {
                for (sentence, sentence_start) in pipeline.analyze_text(&clean_text) {
                    for mut m in engine.match_rule(aux, &sentence) {
                        m.set_offset_length(m.offset() + sentence_start, m.length());
                        matches.push(m);
                    }
                }
            }
            let failure = match example.example_type {
                XmlExampleType::Incorrect => {
                    if matches.is_empty() {
                        Some(Outcome::NoMatch)
                    } else if let Some((m_start, m_end)) = marker_range {
                        let first = &matches[0];
                        let match_start = first.offset();
                        let match_end = match_start + first.length();
                        if match_start <= m_start && match_end >= m_end {
                            None
                        } else {
                            Some(Outcome::Marker)
                        }
                    } else {
                        None
                    }
                }
                XmlExampleType::Correct => {
                    if matches.is_empty() {
                        None
                    } else {
                        Some(Outcome::FalsePositive)
                    }
                }
                XmlExampleType::TriggersError => None,
            };
            (rule.id.clone(), clean_text, failure)
        })
        .collect()
}

const PINNED_EVALUATED: usize = 10_500;
const PINNED_PASSED: usize = 10_500;
const PINNED_RESIDUES: &[(&str, &str)] = &[];

fn strip_xml_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => {
                rest = &rest[start + end + "-->".len()..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[test]
#[ignore]
fn german_refresh_example_suite_is_green_over_accepted_residues() {
    let _release_guard = crate::release_guard();
    let started = std::time::Instant::now();
    let rule_set = compiled_de_rule_set();
    let per_example = evaluate_all();

    let evaluated = per_example.len();
    let passed = per_example
        .iter()
        .filter(|(_, _, failure)| failure.is_none())
        .count();
    let mut outcome_counts: HashMap<Outcome, usize> = HashMap::new();
    let mut failures: Vec<(&str, &str)> = per_example
        .iter()
        .filter_map(|(rule, text, failure)| {
            failure.map(|outcome| {
                *outcome_counts.entry(outcome).or_default() += 1;
                (rule.as_str(), text.as_str())
            })
        })
        .collect();
    failures.sort_unstable();

    eprintln!(
        "de refresh suite: {passed}/{evaluated} passed ({:.1}s) - no_match: {}, fp: {}, marker: {}",
        started.elapsed().as_secs_f64(),
        outcome_counts.get(&Outcome::NoMatch).copied().unwrap_or(0),
        outcome_counts
            .get(&Outcome::FalsePositive)
            .copied()
            .unwrap_or(0),
        outcome_counts.get(&Outcome::Marker).copied().unwrap_or(0),
    );

    let grammar_path = crate::engine::find_lt_resource_dir()
        .expect("languagetool resource dir")
        .join("de/src/main/resources/org/languagetool/rules/de/grammar.xml");
    let xml = std::fs::read_to_string(&grammar_path).expect("de grammar.xml readable");
    let raw_example_count = xml.matches("<example").count();
    let live = strip_xml_comments(&xml);
    let commented_example_count = raw_example_count - live.matches("<example").count();
    let mut antipattern_depth = 0usize;
    let mut antipattern_example_count = 0usize;
    for tag in live
        .match_indices('<')
        .filter_map(|(i, _)| live[i..].find('>').map(|e| &live[i..=i + e]))
    {
        if tag.starts_with("<antipattern") {
            antipattern_depth += 1;
        } else if tag.starts_with("</antipattern") {
            antipattern_depth -= 1;
        } else if tag.starts_with("<example") && antipattern_depth > 0 {
            antipattern_example_count += 1;
        }
    }
    let all_compiled_examples: usize = rule_set.rules.iter().map(|r| r.examples.len()).sum();
    eprintln!(
        "accepted-excluded residue: {raw_example_count} <example> tags in the refreshed de \
         grammar.xml ({commented_example_count} inside XML comments, \
         {antipattern_example_count} inside antipatterns), {all_compiled_examples} on compiled \
         rules, {evaluated} evaluated as first-rule examples of their (id,sub_id) group",
    );

    let leerzeichen: Vec<&(String, String, Option<Outcome>)> = per_example
        .iter()
        .filter(|(rule, _, _)| rule == "LEERZEICHEN_VOR_KLAMMER")
        .collect();
    assert_eq!(
        leerzeichen.len(),
        3,
        "the rewritten rulegroup must surface its three examples: the general-case one plus \
         the job-ad incorrect/correct pair"
    );
    assert!(
        leerzeichen.iter().all(|(_, _, failure)| failure.is_none()),
        "every LEERZEICHEN_VOR_KLAMMER example must pass, including the two new (m/w/d) \
         job-ad ones"
    );
    let gruende: Vec<&(String, String, Option<Outcome>)> = per_example
        .iter()
        .filter(|(rule, _, _)| rule == "GRUENDE")
        .collect();
    assert!(
        gruende.iter().all(|(_, _, failure)| failure.is_none()),
        "the GRUENDE example must pass with the postag trailing-space fix live"
    );

    assert_eq!(
        evaluated, PINNED_EVALUATED,
        "the refreshed grammar.xml must evaluate its {evaluated} live first-rule examples \
         ({raw_example_count} raw <example> tags)"
    );
    assert_eq!(
        passed, PINNED_PASSED,
        "passed count pinned at the de module refresh"
    );
    assert_eq!(
        failures, PINNED_RESIDUES,
        "every example must pass: the former ZUSAMMENSCHREIBUNG_HINTER residue \
         ('Er hat ihr hinterher spioniert.') now matches because speller accept-list lines like \
         spelling.txt's 'hinterher_spionieren' expand to their synthesized conjugations \
         (hinterherspioniert, hinterherzuspionieren, ...), so RemoveUnknownCompoundsFilter no \
         longer drops the compound — pinned identically by og-test-runner's \
         test_real_german_grammar_examples"
    );
}
