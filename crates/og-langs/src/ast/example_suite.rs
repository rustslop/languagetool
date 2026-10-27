use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::{CompiledRuleSet, XmlCompiler};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn ast_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ast/src/main/resources/org/languagetool/rules/ast"))
}

pub(crate) struct AsturianPipeline {
    tagger: Arc<og_tagger::GenericTagger>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl AsturianPipeline {
    pub(crate) fn shared() -> std::sync::Arc<AsturianPipeline> {
        static PIPE: og_core::releasable::Releasable<AsturianPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                super::speller::ast_resource_dir().expect("ast resource dir with asturian.dict");
            Some(std::sync::Arc::new(AsturianPipeline {
                tagger: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                    &resource_dir,
                    "asturian",
                )),
                splitter: og_tokenizer::srx::srx_splitter("ast"),
            }))
        })
        .expect("ast pipeline")
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
        let final_tokens = sentence.tokens_mut();
        for i in (0..final_tokens.len()).rev() {
            if !final_tokens[i].is_whitespace() && !final_tokens[i].has_pos_tag("SENT_START") {
                if !final_tokens[i].has_pos_tag("SENT_END") {
                    final_tokens[i].add_sent_end();
                }
                break;
            }
        }
        sentence
    }
}

fn grammar_rules() -> Arc<CompiledRuleSet> {
    static RULES: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&RULES, || {
        let path = ast_rules_dir().expect("ast rules dir").join("grammar.xml");
        let xml = std::fs::read_to_string(&path).expect("ast grammar.xml read");
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .expect("ast grammar compile"),
        ))
    })
    .expect("ast")
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::Rule as _;

    const JAVA_DEMO_TEXT: &str = "Apega testu equ\u{ed}. o revisa toes les pallabres de esti testu pa ver dalg\u{fa}n de los problemis que LanguageTool ye pa deteutar. \u{bf}Tas avec\u{e1}u con los correutores gramaticales? Has date cuenta de que ent\u{e1} nun son perfeutos.";

    #[test]
    fn srx_splits_match_asturian_srx_sentence_tokenizer_test() {
        use og_tokenizer::SentenceTokenizer as _;
        let pipeline = AsturianPipeline::shared();
        let splitter = pipeline.splitter.as_ref().expect("ast srx splitter");
        let texts: Vec<String> = splitter
            .split("De secute, los hackers de Minix aportaron idegues y c\u{f3}digu al n\u{fa}cleu Linux, y g\u{fc}ey recibiera contribuciones de miles de programadores. Torvalds sigue lliberando nueves versiones del n\u{fa}cleu, consolidando aportes d'otros programadores y faciendo cambios el mesmu.")
            .into_iter()
            .map(|s| s.text().to_string())
            .collect();
        assert_eq!(
            texts,
            vec![
                "De secute, los hackers de Minix aportaron idegues y c\u{f3}digu al n\u{fa}cleu Linux, y g\u{fc}ey recibiera contribuciones de miles de programadores. ",
                "Torvalds sigue lliberando nueves versiones del n\u{fa}cleu, consolidando aportes d'otros programadores y faciendo cambios el mesmu."
            ]
        );

        let two_breaks = |text: &str| -> Vec<String> {
            let splitter = og_tokenizer::srx::srx_splitter("ast").unwrap();
            splitter
                .split(text)
                .into_iter()
                .map(|s| s.text().to_string())
                .collect()
        };
        assert_eq!(
            two_breaks("De secute,\nlos hackers de Minix..."),
            vec!["De secute,\nlos hackers de Minix..."]
        );
        assert_eq!(
            two_breaks("De secute,\n\nlos hackers de Minix..."),
            vec!["De secute,\n\n", "los hackers de Minix..."]
        );
        let one_break = |text: &str| -> Vec<String> {
            let doc = og_tokenizer::srx::shared_srx_document().unwrap();
            let splitter =
                og_tokenizer::srx::SrxSentenceSplitter::new(std::sync::Arc::clone(doc), "ast_one");
            splitter
                .split(text)
                .into_iter()
                .map(|s| s.text().to_string())
                .collect()
        };
        assert_eq!(
            one_break("De secute,\nlos hackers de Minix..."),
            vec!["De secute,\n", "los hackers de Minix..."]
        );
        assert_eq!(
            one_break("De secute,\n\nlos hackers de Minix..."),
            vec!["De secute,\n", "\n", "los hackers de Minix..."]
        );
    }

    #[test]
    fn tagger_readings_back_grammar_postag_rules() {
        let sentences = AsturianPipeline::shared().analyze(
            "Xuan va a garrar l\u{2019}autob\u{fa}s. Nos marchamos pal pueblu. Tien les llaves de l\u{2019}apartamentu. Tienes que lo dicir a l\u{2019}entamu.",
        );
        let flat: Vec<&AnalyzedTokenReadings> = sentences
            .iter()
            .flat_map(|s| s.non_whitespace_tokens())
            .filter(|t| !t.has_pos_tag("SENT_START"))
            .collect();
        let reading_of = |surface: &str| -> String {
            let atr = flat
                .iter()
                .find(|t| t.token().token() == surface)
                .unwrap_or_else(|| panic!("token {surface} missing"));
            format!(
                "{}/{}",
                atr.readings()[0].lemma().unwrap_or("null"),
                atr.readings()[0].pos_tags().join("|")
            )
        };
        assert_eq!(reading_of("va"), "dir/VMIP3S00");
        assert_eq!(reading_of("garrar"), "garrar/VMN00000");
        assert_eq!(reading_of("entamu"), "entamu/NCMSV000");
        assert_eq!(reading_of("marchamos"), "marchar/VMIP1P00");
        assert_eq!(reading_of("apartamentu"), "apartamentu/NCMSV000");
    }

    #[test]
    fn contraction_rules_match_on_apostrophe_split_tokens() {
        let engine = PatternRuleEngine::new();
        let mut hits = 0;
        for rule in &grammar_rules().rules {
            if rule.id != "A_CONTR" {
                continue;
            }
            for sentence in
                AsturianPipeline::shared().analyze("Tienes que lo dicir a l\u{2019}entamu.")
            {
                hits += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            hits, 1,
            "A_CONTR A_L' subrule must match 'a l\u{2019}entamu'"
        );
    }

    const MEASURED_DEMO_SEQUENCE: &[&str] = &[
        "UPPERCASE_SENTENCE_START",
        "TOU_CONTR",
        "PREPOS_APOSTR",
        "MORFOLOGIK_RULE_AST",
        "MORFOLOGIK_RULE_AST",
        "MORFOLOGIK_RULE_AST",
        "CON_CONTR",
        "QUE_APOSTR",
    ];

    fn demo_match_sequence(speller: &dyn og_core::rule::Rule) -> Vec<String> {
        let mut matches: Vec<og_core::RuleMatch> = Vec::new();
        for sentence in AsturianPipeline::shared().analyze(JAVA_DEMO_TEXT) {
            let engine = PatternRuleEngine::new();
            for rule in &grammar_rules().rules {
                matches.extend(engine.match_rule(rule, &sentence));
            }
            matches.extend(
                og_rules::native_rules::UppercaseSentenceStartRule::new().match_sentence(&sentence),
            );
            matches.extend(speller.match_sentence(&sentence));
        }
        matches.sort_by_key(|m| m.offset());
        matches.iter().map(|m| m.rule().id().to_string()).collect()
    }

    #[test]
    fn demo_text_rule_sequence_matches_asturian_test() {
        let ids = demo_match_sequence(&super::super::speller::AsturianSpellerRule::new());
        assert_eq!(
            ids, MEASURED_DEMO_SEQUENCE,
            "Java AsturianTest expects this sequence prefixed by one extra MORFOLOGIK_RULE_AST on sentence-start 'Apega'; see module report"
        );
    }
}
