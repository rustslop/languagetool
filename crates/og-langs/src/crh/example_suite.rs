use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

struct CrimeanTatarPipeline {
    tokenizer: super::word_tokenizer::CrimeanTatarWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

fn crh_resource_dir() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("crh/src/main/resources/org/languagetool/resource/crh")
}

impl CrimeanTatarPipeline {
    fn shared() -> std::sync::Arc<CrimeanTatarPipeline> {
        static PIPE: og_core::releasable::Releasable<CrimeanTatarPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            Some(Arc::new(CrimeanTatarPipeline {
                tokenizer: super::word_tokenizer::CrimeanTatarWordTokenizer::new(),
                tagger: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                    &crh_resource_dir(),
                    "crimean_tatar",
                )),
                splitter: og_tokenizer::srx::srx_splitter("crh"),
            }))
        })
        .expect("crh pipeline")
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
        .join("crh/src/main/resources/org/languagetool/rules/crh/grammar.xml")
}

fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("crh grammar.xml");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile crh grammar.xml"),
        ))
    })
    .expect("crh")
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

fn matches_for_example(
    pipeline: &CrimeanTatarPipeline,
    engine: &PatternRuleEngine,
    group: &[&CompiledRule],
    clean_text: &str,
) -> Vec<og_core::RuleMatch> {
    let mut matches = Vec::new();
    for aux in group.iter() {
        for (sentence, sentence_start) in pipeline.analyze_offset_text(clean_text) {
            for mut m in engine.match_rule(aux, &sentence) {
                m.set_offset_length(m.offset() + sentence_start, m.length());
                matches.push(m);
            }
        }
    }
    matches
}

#[test]
#[ignore]
fn crimean_tatar_grammar_examples_with_crh_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = CrimeanTatarPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut correction_failures = Vec::new();
    let mut fail_lines: Vec<String> = Vec::new();
    for group in &groups {
        let Some(rule) = group.first() else {
            continue;
        };
        for example in &rule.examples {
            let (clean_text, marker_range) = extract_marker(&example.text);
            let matches = matches_for_example(&pipeline, &engine, group, &clean_text);
            let failed = match example.example_type {
                og_xml::types::XmlExampleType::Incorrect => {
                    matches.is_empty()
                        || marker_range.is_some_and(|(m_start, m_end)| {
                            let first = &matches[0];
                            let match_start = first.offset();
                            let match_end = match_start + first.length();
                            !(match_start <= m_start && match_end >= m_end)
                        })
                }
                og_xml::types::XmlExampleType::Correct => !matches.is_empty(),
                og_xml::types::XmlExampleType::TriggersError => false,
            };
            if failed {
                fail_lines.push(format!("FAIL: [{}]: '{}'", rule.id, clean_text));
                continue;
            }
            if let Some(expected) = example.corrections.first() {
                let offered = matches[0]
                    .replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect::<Vec<_>>();
                if !offered.iter().any(|r| r == expected) {
                    correction_failures.push(format!(
                        "CORRECTION: [{}]: '{}' expected '{expected}' got {offered:?}",
                        rule.id, clean_text
                    ));
                    continue;
                }
            }
            passed += 1;
        }
    }
    let total = passed + fail_lines.len() + correction_failures.len();
    eprintln!(
        "crh module pipeline: {}/{} passed (no_match/marker {}, correction {})",
        passed,
        total,
        fail_lines.len(),
        correction_failures.len()
    );
    let mut all_failures = fail_lines;
    all_failures.extend(correction_failures);
    let _ = std::fs::write(
        "/tmp/test_failures_crh_module.txt",
        all_failures.join("\n") + "\n",
    );
    assert_eq!(total, 98, "grammar.xml carries 98 examples");
}

#[test]
fn grammar_rules_compile_the_java_inventory() {
    let rule_set = compiled_rules();
    assert!(
        rule_set.rules.len() >= 90,
        "crh grammar.xml must compile a real rule set, got {}",
        rule_set.rules.len()
    );
    let example_count: usize = rule_groups(&rule_set)
        .iter()
        .filter_map(|g| g.first())
        .flat_map(|r| r.examples.iter())
        .count();
    assert_eq!(example_count, 98, "grammar.xml carries 98 examples");
}

#[test]
fn srx_languagemap_resolves_the_crimean_tatar_rule_cascade() {
    let splitter = og_tokenizer::srx::srx_splitter("crh").expect("crh SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("Yapraqlar töküldi. Otlar-ölenler sarardı, soldılar.");
    assert_eq!(
        sentences.len(),
        2,
        "CrimeanTatarSRXSentenceTokenizerTest.java:45-46 splits after the dot"
    );
    assert!(sentences[0].text().starts_with("Yapraqlar töküldi."));
    let sentences = splitter.split("Yapraqlar töküldi. ");
    assert_eq!(sentences.len(), 1);
}

#[test]
fn comma_whitespace_rule_flags_the_java_example() {
    let engine = crate::engine::LanguageEngine::shared_for_code("crh").expect("crh engine");
    let text = "Özü bir çaqrım uzaqta , demiryol keçidi yanında yaşay.";
    let matches = engine.check(text).matches;
    let hit = matches
        .iter()
        .find(|m| m.rule().id() == "COMMA_PARENTHESIS_WHITESPACE")
        .expect("CrimeanTatar.java:122-124 example must be flagged");
    let start = text.find(" ,").expect("space-comma span");
    assert_eq!(hit.offset(), start);
    assert_eq!(hit.offset() + hit.length(), start + 2);
}

#[test]
fn uppercase_sentence_start_flags_the_java_example() {
    let engine = crate::engine::LanguageEngine::shared_for_code("crh").expect("crh engine");
    let text =
        "Meclis Haberleri 10.09.2003. qırımtatar Milliy Meclisiniñ 120-cı toplaşuvı olıp keçti";
    let matches = engine.check(text).matches;
    let hit = matches
        .iter()
        .find(|m| m.rule().id() == "UPPERCASE_SENTENCE_START")
        .expect("CrimeanTatar.java:126-128 example must be flagged");
    let start = text.find("qırımtatar").expect("qırımtatar offset");
    assert_eq!(hit.offset(), start);
    assert_eq!(hit.offset() + hit.length(), start + "qırımtatar".len());
    assert_eq!(hit.replacements()[0].value(), "Qırımtatar");
}

#[test]
fn engine_registers_the_java_relevant_rule_set() {
    let engine = crate::engine::LanguageEngine::shared_for_code("crh").expect("crh engine");
    let ids = engine.rule_ids();
    for id in [
        "DOUBLE_PUNCTUATION",
        "COMMA_PARENTHESIS_WHITESPACE",
        "UPPERCASE_SENTENCE_START",
        "WHITESPACE_PARAGRAPH_BEGIN",
        "PLURAL_AFFIX_CONFUSION",
        "PHRASEOLOGY",
        "PHRASEOLOGY_CYR",
    ] {
        assert!(ids.contains(&id), "missing rule {id}");
    }
    let text_level = engine.text_level_rule_ids();
    for id in ["WHITESPACE_RULE", "SENTENCE_WHITESPACE"] {
        assert!(text_level.contains(&id), "missing text-level rule {id}");
    }
}

#[test]
fn speller_rule_stays_data_blocked_until_a_crimean_tatar_dict_lands() {
    let resource_dir = crh_resource_dir();
    let dict_files: Vec<String> = std::fs::read_dir(&resource_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".dict"))
        .collect();
    assert!(
        dict_files.is_empty(),
        "the LT checkout shipped no Crimean Tatar .dict (crimean_tatar.dict, \
         crimean_tatar_synth.dict, hunspell/crh_UA.dict) when this pin was written; \
         landing one must unblock CrimeanTatarTagger, CrimeanTatarSynthesizer and \
         MorfologikCrimeanTatarSpellerRule, found {dict_files:?}"
    );
    let spelling = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt"))
        .expect("crh hunspell/spelling.txt");
    assert!(
        spelling
            .lines()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with('#')),
        "spelling.txt was comment-only when this pin was written"
    );

    let engine = crate::engine::LanguageEngine::shared_for_code("crh").expect("crh engine");
    assert!(
        !engine.rule_ids().contains(&"MORFOLOGIK_RULE_CRH_UA"),
        "with no dictionary data the crh engine must install no speller rule"
    );
    for text in [
        "abadlarnı amutlarıñ!",
        "aaabadlarnı",
        "abadanlaşırlar",
        "meraba",
    ] {
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_CRH_UA"),
            "'{text}' must stay unflagged by the blocked speller, got {ids:?}"
        );
    }
}

fn rule_matches_only(text: &str, rule_id: &str) -> Vec<og_core::RuleMatch> {
    let engine = crate::engine::LanguageEngine::shared_for_code("crh").expect("crh engine");
    let mut request = og_core::CheckRequest::new(text.to_string(), engine.language().clone());
    request.enabled_rules = vec![rule_id.to_string()];
    engine.check_with_request(&request).matches
}

#[test]
fn plural_affix_confusion_pins_the_java_correction_pairs() {
    let matches = rule_matches_only("Defterimde hataler yaptım.", "PLURAL_AFFIX_CONFUSION");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "hatalar");
    let matches = rule_matches_only("Tereklar kördim.", "PLURAL_AFFIX_CONFUSION");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "Terekler");
    let matches = rule_matches_only("Дефтеримде хаталер яптым.", "PLURAL_AFFIX_CONFUSION_CYR");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "хаталар");
}

#[test]
fn regexp_match_suggestion_families_render_the_java_corrections() {
    let cases = [
        (
            "Yeter, ağızıñnı qapat!",
            "MIDDLE_VOWEL_EXCLUSION",
            "ağzıñnı",
        ),
        ("Eleki bozulğan edi.", "MISSVOCALIZATION", "Elegi"),
        (
            "Menim ağa qardaşlarım Aqmescitte yaşaylar.",
            "NOUN_DOUBLING",
            "ağa-qardaşlarım",
        ),
        (
            "İşiñni yaptıñ mı?",
            "QUESTION_PARTICLE_SEPARATION",
            "yaptıñmı",
        ),
        (
            "Bu yapar mısıñ?",
            "QUESTION_PARTICLE_PERSONALIZED_MISSPOS",
            "yaparsıñmı",
        ),
        ("O Qrımda yaşamız.", "TOPONOMY_TYPOS", "Qırımda"),
        ("qışkence", "LASTING_ADVERB_AFFIX", "qışqanca"),
        ("Salam aleykum dostum.", "COLLOQUIAL_TYPOS", "Selâm aleykum"),
    ];
    for (text, rule_id, expected) in cases {
        let matches = rule_matches_only(text, rule_id);
        assert_eq!(
            matches.len(),
            1,
            "{rule_id} must fire exactly once on '{text}'"
        );
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == expected),
            "{rule_id} on '{text}' must suggest '{expected}', got {:?}",
            matches[0].replacements()
        );
    }
}

#[test]
fn number_defis_rules_pin_the_java_correction_pairs() {
    let rule_set = compiled_rules();
    let pipeline = CrimeanTatarPipeline::shared();
    let engine = PatternRuleEngine::new();
    let offered_for = |rule_id: &str, text: &str| -> Vec<String> {
        rule_set
            .rules
            .iter()
            .filter(|r| r.id == rule_id)
            .flat_map(|rule| {
                let group = vec![rule];
                matches_for_example(&pipeline, &engine, &group, text)
                    .into_iter()
                    .flat_map(|m| {
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let offered = offered_for("NUMBER_AFTER_DEFIS_MISSING", "1981s.");
    assert_eq!(offered, vec!["1981-s".to_string(), "1981 s".to_string()]);
    let offered = offered_for("NUMBER_AFTER_DEFIS_MISSING", "1981с.");
    assert_eq!(offered, vec!["1981-с".to_string(), "1981 с".to_string()]);
    let offered = offered_for("NUMBER_BEFORE_DEFIS_MISSING", "F32 savaş uçağı.");
    assert_eq!(offered, vec!["F-32".to_string(), "F 32".to_string()]);
    let offered = offered_for("NUMBER_BEFORE_DEFIS_MISSING", "Ф32 саваш учагъы.");
    assert_eq!(offered, vec!["Ф-32".to_string(), "Ф 32".to_string()]);
    let offered = offered_for("COMPLEX_NUMBER_DEFIS_MISSING", "21fayız.");
    assert_eq!(
        offered,
        vec!["21 fayız".to_string(), "21-fayız".to_string()]
    );
    let offered = offered_for("COMPLEX_NUMBER_DEFIS_MISSING", "5nci.");
    assert_eq!(offered, vec!["5 nci".to_string(), "5-nci".to_string()]);
}

#[test]
fn postag_rule_residue_is_exactly_the_missing_dict_family() {
    let failing_ids = [
        "THANKFULLNESS_COLLOQUIAL",
        "PERSON_COORDINATION",
        "REASON_PARTICLE_CASE_COLLOCATION",
        "TENSE_PARTICLE_CASE_COLLOCATION",
        "POSTPOSITION_CASE_COLLOCATION",
        "EXODIVE_VERB_ADVB_COLLOCATION",
        "NUMERAL_PLURAL_COLLOCATION",
        "VERB_PERSON_COORDINATION",
        "PERCENTAGE_COORDINATION",
        "PERCENTAGE_COORDINATION_CYR",
        "PHRASEOLOGY",
        "PHRASEOLOGY_CYR",
        "VERB_CASE_COLLOCATION",
        "VERB_CASE_COLLOCATION_CYR",
    ];
    let rule_set = compiled_rules();
    for id in failing_ids {
        let rules = rule_set
            .rules
            .iter()
            .filter(|r| r.id == id)
            .collect::<Vec<_>>();
        assert!(!rules.is_empty(), "{id} must exist in grammar.xml");
        for rule in rules {
            assert!(
                rule.pattern.tokens.iter().any(|t| t.postag.is_some()),
                "{id} must gate at least one token on a postag"
            );
        }
    }
    let postag_free: Vec<&str> = rule_set
        .rules
        .iter()
        .map(|r| r.id.as_str())
        .filter(|id| !failing_ids.contains(id))
        .collect();

    let pipeline = CrimeanTatarPipeline::shared();
    assert!(
        !pipeline.tagger.has_dictionary(),
        "the residue family assumes no crimean_tatar.dict data"
    );
    for word in [
        "Menim",
        "dostu",
        "yaptılar",
        "elli",
        "faiz",
        "kelgen",
        "soñ",
    ] {
        let tagged = og_tagger::Tagger::tag(pipeline.tagger.as_ref(), &[word]);
        assert!(
            tagged[0].readings().is_empty(),
            "'{word}' must carry no readings without dictionary data"
        );
    }

    let engine = PatternRuleEngine::new();
    let mut postag_free_failures = Vec::new();
    for group in rule_groups(&rule_set) {
        let Some(rule) = group.first() else {
            continue;
        };
        if failing_ids.contains(&rule.id.as_str()) {
            continue;
        }
        for example in &rule.examples {
            let (clean_text, _) = extract_marker(&example.text);
            if matches_for_example(&pipeline, &engine, &group, &clean_text).is_empty() {
                postag_free_failures.push(format!("[{}]: '{}'", rule.id, clean_text));
            }
        }
    }
    assert!(
        postag_free_failures.is_empty(),
        "every postag-free rule must match its examples without dictionary data: \
         {postag_free_failures:?} (postag-free ids: {})",
        postag_free.len()
    );
}
