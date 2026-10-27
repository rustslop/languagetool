use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use std::sync::Arc;

pub(crate) enum PtTokenizer {
    Portuguese(Arc<super::word_tokenizer::PortugueseWordTokenizer>),
    Generic(og_tokenizer::DefaultWordTokenizer),
}

pub(crate) struct PortuguesePipeline {
    tokenizer: PtTokenizer,
    tagger: Arc<super::tagger::PortugueseTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl PortuguesePipeline {
    pub fn shared() -> std::sync::Arc<PortuguesePipeline> {
        static PIPE: og_core::releasable::Releasable<PortuguesePipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                super::tagger::pt_resource_dir().expect("pt resource dir with portuguese.dict");
            let tagger = super::tagger::PortugueseTagger::shared_arc().unwrap_or_else(|| {
                Arc::new(super::tagger::PortugueseTagger::from_resource_dir(
                    &resource_dir,
                ))
            });
            let tokenizer = match super::word_tokenizer::shared_portuguese_word_tokenizer() {
                Some(tz) => PtTokenizer::Portuguese(tz),
                None => PtTokenizer::Generic(og_tokenizer::DefaultWordTokenizer::new_generic()),
            };
            let disambiguator = super::build_pt_disambiguator(&resource_dir);
            Some(std::sync::Arc::new(PortuguesePipeline {
                tokenizer,
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("pt"),
            }))
        })
        .expect("pt pipeline")
    }

    pub fn tagger_ref(&self) -> &super::tagger::PortugueseTagger {
        &self.tagger
    }

    pub fn sentence_ranges(&self, text: &str) -> Vec<(usize, usize)> {
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

    pub fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(match &self.tokenizer {
            PtTokenizer::Portuguese(tz) => {
                use og_core::checker::WordTokenizer as _;
                tz.tokenize(sentence_text, 0)
            }
            PtTokenizer::Generic(tz) => {
                use og_tokenizer::WordTokenizer as _;
                tz.tokenize(sentence_text)
                    .into_iter()
                    .map(|t| {
                        AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
                    })
                    .collect()
            }
        });
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
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }

    pub fn analyze_text(&self, text: &str) -> Vec<AnalyzedSentence> {
        self.sentence_ranges(text)
            .into_iter()
            .map(|(s, e)| self.analyze_range(text, s, e))
            .collect()
    }

    pub fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        self.sentence_ranges(text)
            .into_iter()
            .map(|(s, e)| (self.analyze_range(text, s, e), s))
            .collect()
    }
}

pub(crate) fn analyze_portuguese(text: &str) -> Vec<AnalyzedSentence> {
    PortuguesePipeline::shared().analyze_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_disambiguation_adds_is_url_readings() {
        let sentences = analyze_portuguese("google.com.br é um excelente recurso.");
        let url_tokens: Vec<(&str, bool)> = sentences[0]
            .tokens()
            .iter()
            .filter(|t| matches!(t.token().token(), "google" | "com" | "br"))
            .map(|t| {
                (
                    t.token().token(),
                    t.readings()
                        .iter()
                        .any(|r| r.pos_tags().contains(&"_IS_URL".to_string())),
                )
            })
            .collect();
        assert_eq!(url_tokens.len(), 3, "{url_tokens:?}");
        assert!(
            url_tokens.iter().all(|(_, marked)| *marked),
            "disambiguation-global.xml GLOBAL_URL rules must add _IS_URL to every URL token: {url_tokens:?}"
        );
    }

    #[test]
    fn pt_disambiguation_entities_resolve_det_verbonome() {
        let sentences =
            analyze_portuguese("Ele confundiu o rapaz com um borrego, seu tira moncos.");
        let tira = sentences[0]
            .tokens()
            .iter()
            .find(|t| t.token().token() == "tira")
            .expect("tira token");
        let readings: Vec<String> = tira
            .readings()
            .iter()
            .map(|r| r.pos_tags().join("|"))
            .collect();
        assert_eq!(
            readings,
            vec!["NCFS000".to_string()],
            "DET_VERBONOME (disambiguation.xml with external entities resolved) must drop the V.[ISM].+ readings of 'tira' after 'seu': {readings:?}"
        );
    }

    #[test]
    fn srx_rules_resolve_for_portuguese() {
        let splitter = og_tokenizer::srx::srx_splitter("pt");
        assert!(splitter.is_some(), "pt SRX rules must resolve");
        use og_tokenizer::SentenceTokenizer as _;
        let splitter = splitter.unwrap();
        let sentences = splitter.split("Isso é bom. Muito bom.");
        assert_eq!(sentences.len(), 2);
        let sentences = splitter.split("Na Av. Paulista há flores.");
        assert_eq!(
            sentences.len(),
            1,
            "pt abbreviation rule keeps 'Av.' from ending a sentence"
        );
        let sentences = splitter.split("Fica na R. Augusta, n. 5, em Lisboa.");
        assert_eq!(
            sentences.len(),
            1,
            "pt abbreviation rule keeps 'n.' from ending a sentence"
        );
    }

    #[test]
    fn pipeline_tags_ordinal_and_mente_through_the_portuguese_tagger() {
        let sentences = analyze_portuguese("A 10ª reunião decorreu destacadamente bem.");
        let tags: Vec<(String, Vec<String>)> = sentences[0]
            .tokens()
            .iter()
            .filter(|t| !t.is_whitespace())
            .map(|t| {
                (
                    t.token().token().to_string(),
                    t.readings()
                        .iter()
                        .flat_map(|r| r.pos_tags().to_vec())
                        .collect(),
                )
            })
            .collect();
        let ordinal = tags
            .iter()
            .find(|(surface, _)| surface == "10ª")
            .expect("ordinal token");
        assert!(
            ordinal.1.iter().any(|t| t == "AO0FS0"),
            "10ª must carry the AO0FS0 ordinal reading: {:?}",
            ordinal.1
        );
        let adverb = tags
            .iter()
            .find(|(surface, _)| surface == "destacadamente")
            .expect("mente adverb token");
        assert!(
            adverb.1.iter().any(|t| t == "RM"),
            "destacadamente is dict-missing, tagMenteAdverbs tags it RG and the disambiguator's WORKAROUND_MISSING_ADVERBS_POS rewrites it to RM: {:?}",
            adverb.1
        );
    }

    #[test]
    fn pipeline_sentence_splits_and_disambiguates() {
        let sentences = analyze_portuguese("Ele chegou. Ele partiu.");
        assert_eq!(sentences.len(), 2);
        let readings: Vec<String> = sentences[0]
            .tokens()
            .iter()
            .filter(|t| !t.is_whitespace())
            .map(|t| {
                format!(
                    "{}={}",
                    t.token().token(),
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().first().cloned().unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect();
        assert!(
            readings.iter().any(|r| r.starts_with("chegou=")),
            "{readings:?}"
        );
    }
}
