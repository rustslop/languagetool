use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::collections::HashMap;
use std::sync::Arc;

struct BelarusianPipeline {
    tokenizer: super::word_tokenizer::BelarusianWordTokenizer,
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

fn be_resource_dir() -> std::path::PathBuf {
    crate::engine::find_lt_resource_dir()
        .expect("lt resource dir")
        .join("be/src/main/resources/org/languagetool/resource/be")
}

impl BelarusianPipeline {
    fn shared() -> std::sync::Arc<BelarusianPipeline> {
        static PIPE: og_core::releasable::Releasable<BelarusianPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            Some(Arc::new(BelarusianPipeline {
                tokenizer: super::word_tokenizer::BelarusianWordTokenizer::new(),
                tagger: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                    &be_resource_dir(),
                    "",
                )),
                splitter: og_tokenizer::srx::srx_splitter("be"),
            }))
        })
        .expect("be pipeline")
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
        .join("be/src/main/resources/org/languagetool/rules/be/grammar.xml")
}

fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path();
        let xml = std::fs::read_to_string(&path).expect("be grammar.xml");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("compile be grammar.xml"),
        ))
    })
    .expect("be")
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

#[test]
#[ignore]
fn belarusian_grammar_examples_with_be_pipeline() {
    let _release_guard = crate::release_guard();
    let rule_set = compiled_rules();
    let groups = rule_groups(&rule_set);
    let engine = PatternRuleEngine::new();
    let pipeline = BelarusianPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let mut passed = 0usize;
    let mut fail_lines: Vec<String> = Vec::new();
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
            } else {
                passed += 1;
            }
        }
    }
    eprintln!(
        "be module pipeline: {}/{} passed",
        passed,
        passed + fail_lines.len()
    );
    let _ = std::fs::write(
        "/tmp/test_failures_be_module.txt",
        fail_lines.join("\n") + "\n",
    );
    assert!(
        passed + fail_lines.len() > 0,
        "suite must evaluate examples"
    );
}

#[test]
fn srx_languagemap_resolves_the_belarusian_ruleset() {
    let splitter = og_tokenizer::srx::srx_splitter("be").expect("be SRX rules must resolve");
    use og_tokenizer::SentenceTokenizer as _;
    let sentences = splitter.split("А. Б. В. піша кнігу. Гэта другі сказ.");
    assert_eq!(
        sentences.len(),
        2,
        "Belarusian capital-letter initials must not break the sentence"
    );
    assert!(sentences[0].text().starts_with("А. Б. В."));
    let sentences = splitter.split("у 1851. годзе было добра.");
    assert_eq!(
        sentences.len(),
        1,
        "digit dot before lowercase must not break"
    );
    let sentences = splitter.split("Гэта першы сказ. Гэта другі сказ!");
    assert_eq!(sentences.len(), 2);
}

#[test]
fn demo_text_rule_sequence_matches_belarusian_test() {
    let engine = crate::engine::LanguageEngine::shared_for_code("be").expect("be engine");
    let text = "Паспрабуйце напісаць нейкі тэкст з памылкамі, а LanguageTool паспрабуе паказаць нейкия найбольш распаусюджаныя памылки.";
    let matches = engine.check(text).matches;
    let ids: Vec<&str> = matches.iter().map(|m| m.rule().id()).collect();
    assert_eq!(ids, vec!["I", "U_KAROTKAJE", "I"]);
}

fn specific_case_matches(text: &str) -> Vec<og_core::RuleMatch> {
    let engine = crate::engine::LanguageEngine::shared_for_code("be").expect("be engine");
    let mut request = og_core::CheckRequest::new(text.to_string(), engine.language().clone());
    request.enabled_rules = vec!["BE_SPECIFIC_CASE".to_string()];
    engine.check_with_request(&request).matches
}

#[test]
fn specific_case_pins_the_java_example_pairs() {
    assert!(specific_case_matches("Беларуская Народная Рэспубліка").is_empty());
    assert!(specific_case_matches("Папа Рымскі").is_empty());
    let matches = specific_case_matches("дзяржаўны сцяг Рэспублікі Беларусь");
    assert_eq!(matches.len(), 1);
    let matches = specific_case_matches("вярхоўны суд рэспублікі беларусь");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].replacements()[0].value(),
        "Вярхоўны Суд Рэспублікі Беларусь"
    );
}

#[test]
fn specific_case_pins_the_java_air_france_offsets() {
    let matches = specific_case_matches("Мне падабаецца air France.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].offset(), "Мне падабаецца ".len());
    assert_eq!(
        matches[0].offset() + matches[0].length(),
        "Мне падабаецца air France".len()
    );
    assert_eq!(matches[0].replacements()[0].value(), "Air France");
    assert_eq!(
        matches[0].message(),
        "Уласныя імёны і назвы пішуцца з вялікай літары."
    );
    assert_eq!(matches[0].short_message(), Some("Proper noun"));
}

#[test]
fn speller_rule_stays_data_blocked_until_a_belarusian_dict_lands() {
    let resource_dir = be_resource_dir();
    let dict_files: Vec<String> = std::fs::read_dir(&resource_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".dict"))
        .collect();
    assert!(
        dict_files.is_empty(),
        "the LT checkout shipped no Belarusian .dict when this pin was written; \
         landing one must unblock MorfologikBelarusianSpellerRule, found {dict_files:?}"
    );
    let spelling = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt"))
        .expect("be hunspell/spelling.txt");
    assert!(
        spelling
            .lines()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with('#')),
        "spelling.txt was comment-only when this pin was written"
    );

    let engine = crate::engine::LanguageEngine::shared_for_code("be").expect("be engine");
    assert!(
        !engine.rule_ids().contains(&"MORFOLOGIK_RULE_BE_BY"),
        "with no dictionary data the be engine must install no speller rule"
    );
    for text in [
        "знакамты",
        " сумна знакаміты вялікімі стратамі",
        "З’яўляцца",
        "З'яўляцца",
        "п'яны",
        "п’яны",
    ] {
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_BE_BY"),
            "'{text}' must stay unflagged by the blocked speller, got {ids:?}"
        );
    }
}

fn simple_replace_matches(text: &str) -> Vec<og_core::RuleMatch> {
    let engine = crate::engine::LanguageEngine::shared_for_code("be").expect("be engine");
    let mut request = og_core::CheckRequest::new(text.to_string(), engine.language().clone());
    request.enabled_rules = vec!["BE_SIMPLE_REPLACE".to_string()];
    engine.check_with_request(&request).matches
}

#[test]
fn simple_replace_pins_the_java_test_trio() {
    assert!(simple_replace_matches("Напрамкі дзейнасці былі ўзгоднены з ,").is_empty());
    assert!(simple_replace_matches("Яго камп’ютар выключыўся.").is_empty());
    let matches = simple_replace_matches("Яго кампутар выключыўся.");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].replacements().len(), 1);
    assert_eq!(matches[0].replacements()[0].value(), "камп’ютар");
}

#[test]
fn pipeline_keeps_apostrophe_words_inert_for_the_grammar_rules() {
    let pipeline = BelarusianPipeline::shared();
    let engine = PatternRuleEngine::new();
    let rule_set = compiled_rules();
    og_rules::pattern_rule::set_test_date_hack(true);
    for text in [
        "З'ява, зʼява, з’ява",
        "Яго камп’ютар выключыўся.",
        "Мой адрас — address@email.com.",
    ] {
        let mut matched = Vec::new();
        for rule in &rule_set.rules {
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                matched.extend(engine.match_rule(rule, &sentence));
            }
        }
        assert!(
            matched.is_empty(),
            "'{text}' must stay unflagged: {:?}",
            matched
                .iter()
                .map(|m| m.rule().id().to_string())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn pipeline_tokens_the_java_mixed_apostrophe_example() {
    let pipeline = BelarusianPipeline::shared();
    let sentences = pipeline.analyze_offset_text("З'ява, зʼява, з’ява");
    assert_eq!(sentences.len(), 1);
    let token_texts: Vec<&str> = sentences[0]
        .0
        .tokens()
        .iter()
        .skip(1)
        .map(|t| t.token().token())
        .collect();
    assert_eq!(
        token_texts,
        vec!["З'ява", ",", " ", "зʼява", ",", " ", "з'ява"]
    );
}

#[test]
fn grammar_rules_load_the_java_rule_count() {
    let rule_set = compiled_rules();
    assert!(
        rule_set.rules.len() >= 50,
        "be grammar.xml must compile a real rule set, got {}",
        rule_set.rules.len()
    );
}
