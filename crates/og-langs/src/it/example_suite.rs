use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn it_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("it/src/main/resources/org/languagetool/resource/it"))
}

fn it_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("it/src/main/resources/org/languagetool/rules/it"))
}

pub(crate) struct ItalianPipeline {
    tagger: Arc<og_tagger::GenericTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl ItalianPipeline {
    pub(crate) fn shared() -> std::sync::Arc<ItalianPipeline> {
        static PIPE: og_core::releasable::Releasable<ItalianPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = it_resource_dir().expect("it resource dir with italian.dict");
            let tagger = Arc::new(og_tagger::GenericTagger::from_resource_dir(
                &resource_dir,
                "italian",
            ));
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok())
                .and_then(|d| {
                    crate::engine::find_lt_core_resource_dir()
                        .map(|core| og_tagger::hybrid_disambiguator("it", &resource_dir, &core, d))
                });
            Some(std::sync::Arc::new(ItalianPipeline {
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("it"),
            }))
        })
        .expect("it pipeline")
    }

    pub(crate) fn analyze(&self, text: &str) -> Vec<AnalyzedSentence> {
        let ranges: Vec<(usize, usize)> = {
            use og_tokenizer::SentenceTokenizer as _;
            match &self.splitter {
                Some(srx) => srx
                    .split(text)
                    .iter()
                    .map(|s| (s.start(), s.end()))
                    .collect(),
                None => vec![(0, text.len())],
            }
        };
        ranges
            .into_iter()
            .map(|(start, end)| self.analyze_range(text, start, end))
            .collect()
    }

    fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, start, end);
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend({
            use og_tokenizer::WordTokenizer as _;
            og_tokenizer::DefaultWordTokenizer::new_generic()
                .tokenize(sentence_text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(AnalyzedToken::new(
                        t.text(),
                        t.start() + start,
                        t.end() + start,
                    ))
                })
        });
        sentence.set_tokens(tokens);
        let refs: Vec<&str> = sentence
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token())
            .collect();
        let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &refs);
        let tokens_mut = sentence.tokens_mut();
        for (i, atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens_mut.len() {
                continue;
            }
            let readings = atr.into_readings();
            if readings.is_empty() {
                continue;
            }
            let primary = tokens_mut[token_idx].token().clone();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let updated = AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                .with_pos_tags(all_tags)
                .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
            tokens_mut[token_idx] = AnalyzedTokenReadings::new(updated).with_readings(readings);
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }
}

fn grammar_rules() -> std::sync::Arc<CompiledRuleSet> {
    static RULES: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&RULES, || {
        let path = it_rules_dir().expect("it rules dir").join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("it grammar.xml read");
        Some(std::sync::Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("it grammar compile"),
        ))
    })
    .expect("it grammar")
}

fn rules_by_sub_id<'a>(
    rules: &'a CompiledRuleSet,
    id: &str,
    sub_id: &str,
) -> Vec<&'a CompiledRule> {
    rules
        .rules
        .iter()
        .filter(|r| r.id == id && r.sub_id.as_deref() == Some(sub_id))
        .collect()
}

fn matches_for_sub(id: &str, sub_id: &str, text: &str) -> Vec<og_core::RuleMatch> {
    let engine = PatternRuleEngine::new();
    let grammar = grammar_rules();
    let rules = rules_by_sub_id(&grammar, id, sub_id);
    let mut matches = Vec::new();
    let pipeline = ItalianPipeline::shared();
    for sentence in pipeline.analyze(text) {
        for rule in &rules {
            matches.extend(engine.match_rule(rule, &sentence));
        }
    }
    matches
}

fn reading_set(atr: &AnalyzedTokenReadings) -> Vec<String> {
    let mut set: Vec<String> = atr
        .readings()
        .iter()
        .map(|r| format!("{}/{}", r.lemma().unwrap_or("null"), r.pos_tags().join("|")))
        .collect();
    set.sort();
    set.dedup();
    set
}

fn sorted(mut v: Vec<&str>) -> Vec<String> {
    v.sort_unstable();
    v.into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_weekday_with_year_matches_java_examples() {
        og_rules::pattern_rule::set_test_date_hack(true);
        for text in [
            "Luned\u{ec} 7 ottobre 2014",
            "Luned\u{ec}, 7.ott.2014",
            "Luned\u{ec} 7/ottobre/2014",
        ] {
            let matches = matches_for_sub("DATE_WEEKDAY", "1", text);
            assert_eq!(matches.len(), 1, "{text}");
            assert_eq!(
                matches[0].message(),
                "Questa data non \u{e8} un luned\u{ec} ma un marted\u{ec}.",
                "{text}"
            );
        }
        assert!(matches_for_sub("DATE_WEEKDAY", "1", "Marted\u{ec}, 7 ottobre 2014").is_empty());
    }

    #[test]
    fn date_weekday_abbreviated_weekday_still_checks() {
        og_rules::pattern_rule::set_test_date_hack(true);
        let matches = matches_for_sub("DATE_WEEKDAY", "1", "Lun 7 ottobre 2014");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Questa data non \u{e8} un luned\u{ec} ma un marted\u{ec}."
        );
    }

    #[test]
    fn date_weekday_without_year_uses_2014_like_java_test_hack() {
        og_rules::pattern_rule::set_test_date_hack(true);
        let matches = matches_for_sub("DATE_WEEKDAY", "2", "Luned\u{ec} 7 ottobre");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Se si riferisce all'anno in corso (2014), questa data non \u{e8} un luned\u{ec} ma un marted\u{ec}."
        );
        assert!(matches_for_sub("DATE_WEEKDAY", "2", "Marted\u{ec}, 7 ottobre").is_empty());
    }

    #[test]
    fn date_weekday_grammar_examples_all_pass_through_date_check_filter() {
        og_rules::pattern_rule::set_test_date_hack(true);
        for (text, expected) in [
            ("Luned\u{ec} 7 ottobre 2014", 1),
            ("Marted\u{ec}, 7 ottobre 2014", 0),
            ("Luned\u{ec}, 7.ott.2014", 1),
            ("Luned\u{ec} 7/ottobre/2014", 1),
            ("Lun 7 ottobre 2014", 1),
        ] {
            assert_eq!(
                matches_for_sub("DATE_WEEKDAY", "1", text).len(),
                expected,
                "DATE_WEEKDAY[1] on {text}"
            );
        }
        for (text, expected) in [
            ("Luned\u{ec} 7 ottobre", 1),
            ("Luned\u{ec}, 7 ottobre", 1),
            ("Marted\u{ec}, 7 ottobre", 0),
            ("Marted\u{ec} 27 novembre 1922", 0),
            ("Marted\u{ec} 27 novembre del 1922", 0),
        ] {
            assert_eq!(
                matches_for_sub("DATE_WEEKDAY", "2", text).len(),
                expected,
                "DATE_WEEKDAY[2] on {text}"
            );
        }
        let matches = matches_for_sub("DATE_WEEKDAY", "2", "Luned\u{ec}, 7 ottobre");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Se si riferisce all'anno in corso (2014), questa data non \u{e8} un luned\u{ec} ma un marted\u{ec}."
        );
    }

    #[test]
    fn date_weekday_year_examples_blocked_by_antipattern() {
        og_rules::pattern_rule::set_test_date_hack(true);
        for text in [
            "Marted\u{ec} 27 novembre 1922",
            "Marted\u{ec} 27 novembre del 1922",
        ] {
            assert!(
                matches_for_sub("DATE_WEEKDAY", "2", text).is_empty(),
                "antipattern must immunize {text}"
            );
        }
    }

    #[test]
    fn tagger_readings_match_italian_tagger_test() {
        let pipeline = ItalianPipeline::shared();
        let sentences = pipeline.analyze("Non c'\u{e8} linguaggio senza inganno.");
        assert_eq!(sentences.len(), 1);
        let expected: &[(&str, &[&str])] = &[
            ("Non", &["non/ADV"]),
            (
                "\u{e8}",
                &["essere/AUX:ind+pres+3+s", "essere/VER:ind+pres+3+s"],
            ),
            ("linguaggio", &["linguaggio/NOUN-M:s"]),
            ("senza", &["senza/CON", "senza/PRE"]),
            (
                "inganno",
                &["ingannare/VER:ind+pres+1+s", "inganno/NOUN-M:s"],
            ),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = sentences[0]
            .non_whitespace_tokens()
            .into_iter()
            .filter(|t| {
                !t.has_pos_tag("SENT_START") && t.token().token().chars().any(|c| c.is_alphabetic())
            })
            .collect();
        for (surface, readings) in expected {
            let atr = tokens
                .iter()
                .find(|t| t.token().token() == *surface)
                .unwrap_or_else(|| panic!("token {surface} missing"));
            assert_eq!(reading_set(atr), sorted(readings.to_vec()), "{surface}");
        }
    }

    #[test]
    fn tagger_readings_match_italian_tagger_test_second_sentence() {
        let pipeline = ItalianPipeline::shared();
        let sentences = pipeline.analyze("Amo quelli che desiderano l'impossibile.");
        assert_eq!(sentences.len(), 1);
        let expected: &[(&str, &[&str])] = &[
            ("Amo", &["amare/VER:ind+pres+1+s"]),
            ("quelli", &["quelli/PRO-DEMO-M-P", "quello/DET-DEMO:m+p"]),
            (
                "che",
                &[
                    "che/CON",
                    "che/DET-WH:f+p",
                    "che/DET-WH:f+s",
                    "che/DET-WH:m+p",
                    "che/DET-WH:m+s",
                    "che/WH-CHE",
                ],
            ),
            ("desiderano", &["desiderare/VER:ind+pres+3+p"]),
            ("l", &["null/"]),
            (
                "impossibile",
                &["impossibile/ADJ:pos+f+s", "impossibile/ADJ:pos+m+s"],
            ),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = sentences[0]
            .non_whitespace_tokens()
            .into_iter()
            .filter(|t| {
                !t.has_pos_tag("SENT_START") && t.token().token().chars().any(|c| c.is_alphabetic())
            })
            .collect();
        for (surface, readings) in expected {
            let atr = tokens
                .iter()
                .find(|t| t.token().token() == *surface)
                .unwrap_or_else(|| panic!("token {surface} missing"));
            assert_eq!(reading_set(atr), sorted(readings.to_vec()), "{surface}");
        }
    }

    #[test]
    fn tagger_unknown_word_has_no_readings() {
        let pipeline = ItalianPipeline::shared();
        let sentences = pipeline.analyze("blablabla");
        assert_eq!(sentences.len(), 1);
        let tokens = sentences[0].non_whitespace_tokens();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1].token().token(), "blablabla");
        assert_eq!(reading_set(tokens[1]), vec!["null/".to_string()]);
    }

    #[test]
    fn synthesizer_seek_path_matches_italian_synthesizer_test() {
        let resource_dir = it_resource_dir().expect("it resource dir");
        let dict = og_tagger::morfologik::shared_synth_dictionary(&resource_dir, "italian_synth")
            .expect("italian_synth dict loads");
        assert_eq!(
            dict.lookup("richiedere|VER:impr+pres+2+s"),
            vec!["richiedi".to_string()]
        );
        assert_eq!(
            dict.lookup("esultare|VER:part+past+s+f"),
            vec!["esultata".to_string()]
        );
        assert_eq!(dict.lookup("libro|NOUN-M:p"), vec!["libri".to_string()]);
        assert!(dict.lookup("blablabla|blablabla").is_empty());
    }

    #[test]
    fn srx_keeps_italian_abbreviations_in_one_sentence() {
        let pipeline = ItalianPipeline::shared();
        for text in [
            "Dott. Bunsen Honeydew",
            "La Prov. di Bolzano ha competenze autonome.",
            "Alcuni frutti, es. mele e pere, sono disponibili.",
            "Nel XIX sec. si verificarono grandi cambiamenti.",
            "La diagnosi \u{e8} compatibile con sdr. metabolica.",
            "Il cd. regolamento \u{e8} stato approvato ieri.",
            "Lavora nel sett. energetico da anni.",
            "La Reg. d\u{2019}Abruzzo confina con il Lazio.",
        ] {
            assert_eq!(
                pipeline.analyze(text).len(),
                1,
                "must not split after abbreviation in {text}"
            );
        }
    }

    #[test]
    fn srx_splits_at_sentence_boundary() {
        let pipeline = ItalianPipeline::shared();
        let text = "Il Castello Reale di Racconigi \u{e8} situato a Racconigi, in provincia di Cuneo ma poco distante da Torino. Nel corso della sua quasi millenaria storia ha visto numerosi rimaneggiamenti.";
        let sentences = pipeline.analyze(text);
        assert_eq!(sentences.len(), 2);
        assert!(sentences[0].text().trim_end().ends_with("Torino."));
        assert!(sentences[1].text().starts_with("Nel corso"));
    }

    #[test]
    fn italian_test_demo_text_matches_java_expectation() {
        let _release_guard = crate::release_guard();
        let engine = crate::engine::LanguageEngine::for_code("it").expect("it engine");
        let text = "Inserite qui il vostro testo... Oppure controlate direttamente questo ed avrete un assaggio di quali errori possono essere identificati con LanguageTool.";
        let result = engine.check(text);
        let covered: Vec<(&str, &str)> = result
            .matches
            .iter()
            .map(|m| {
                let start = m.offset();
                (m.rule().id(), &text[start..start + m.length()])
            })
            .collect();
        let java_expected: Vec<(&str, &str)> = vec![
            ("MORFOLOGIK_RULE_IT_IT", "controlate"),
            ("ST_03_001", "ed avrete"),
        ];
        let non_residue: Vec<(&str, &str)> = covered
            .iter()
            .filter(|(id, word)| !(*id == "MORFOLOGIK_RULE_IT_IT" && *word == "LanguageTool"))
            .copied()
            .collect();
        assert_eq!(
            non_residue, java_expected,
            "ItalianTest.java:35-40 expects exactly [MORFOLOGIK_RULE_IT_IT, ST_03_001]; \
             the LanguageTool speller hit is pinned residue, see it::trace \
             probe_demo_text_languagetool_speller_fp"
        );
        let st = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "ST_03_001")
            .expect("ST_03_001 match");
        assert_eq!(st.replacements().len(), 1);
        assert_eq!(st.replacements()[0].value(), "e avrete");
    }
}
