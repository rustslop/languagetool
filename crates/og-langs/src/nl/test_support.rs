use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tokenizer::WordTokenizer as _;
use std::sync::Arc;

pub(crate) struct DutchPipeline {
    tokenizer: og_tokenizer::DefaultWordTokenizer,
    tagger: Arc<super::tagger::DutchTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl DutchPipeline {
    pub fn shared() -> std::sync::Arc<DutchPipeline> {
        static PIPE: og_core::releasable::Releasable<DutchPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                super::tagger::nl_resource_dir().expect("nl resource dir with dutch.dict");
            let tagger = super::tagger::DutchTagger::shared_arc().unwrap_or_else(|| {
                Arc::new(super::tagger::DutchTagger::from_resource_dir(&resource_dir))
            });
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml_with_english(&xml, None).ok())
                .and_then(|d| {
                    crate::engine::find_lt_core_resource_dir()
                        .map(|core| og_tagger::hybrid_disambiguator("nl", &resource_dir, &core, d))
                });
            Some(std::sync::Arc::new(DutchPipeline {
                tokenizer: og_tokenizer::DefaultWordTokenizer::new_dutch(),
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("nl"),
            }))
        })
        .expect("nl pipeline")
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
            let ignored_by_speller = atr.is_ignored_by_speller();
            let readings = atr.into_readings();
            if !readings.is_empty() {
                let primary = tokens_mut[token_idx].token().clone();
                let updated_primary =
                    AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                        .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                let mut merged =
                    AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
                if ignored_by_speller {
                    merged.ignore_spelling();
                }
                tokens_mut[token_idx] = merged;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srx_rules_resolve_for_dutch_and_split_java_examples() {
        let splitter = og_tokenizer::srx::srx_splitter("nl").expect("nl SRX rules must resolve");
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split("Dit is een zin. Nog een.");
        assert_eq!(sentences.len(), 2);
        let sentences = splitter.split("Dit is een zin. Nog een.");
        assert_eq!(sentences[0].end(), 16);
        let sentences = splitter.split("Januari jl. is dat vastgelegd.");
        assert_eq!(sentences.len(), 1, "abbreviation keeps 'jl.' internal");
        let sentences = splitter.split("Vandaag is het 13-12-2004.");
        assert_eq!(sentences.len(), 1, "date keeps hyphens internal");
        let sentences = splitter.split("Om 17:00 begint het.");
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn pipeline_tags_through_the_dutch_tagger_and_disambiguator() {
        let sentences = DutchPipeline::shared().analyze_text("Aardappels koken. Zwijnden blijven.");
        assert_eq!(sentences.len(), 2);
        let first = &sentences[0];
        let aardappels = first
            .tokens()
            .iter()
            .find(|t| t.token().token() == "Aardappels")
            .expect("Aardappels token");
        assert!(
            aardappels
                .readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t == "ZNW:MRV:DE_")),
            "{:?}",
            aardappels.readings()
        );
        let second = &sentences[1];
        let zwijnden = second
            .tokens()
            .iter()
            .find(|t| t.token().token() == "Zwijnden")
            .expect("Zwijnden token");
        assert!(
            zwijnden
                .readings()
                .iter()
                .any(|r| r.lemma().is_some_and(|l| l == "zwijnen")),
            "{:?}",
            zwijnden.readings()
        );
    }

    #[test]
    fn multiword_chunker_flags_known_phrases_for_the_speller() {
        let text = "Het was a hell of a job.";
        let sentence = DutchPipeline::shared().analyze_range(text, 0, text.len());
        let flagged: Vec<&str> = sentence
            .tokens()
            .iter()
            .filter(|t| t.is_ignored_by_speller())
            .map(|t| t.token().token())
            .collect();
        assert!(
            flagged.contains(&"a") && flagged.contains(&"hell") && flagged.contains(&"job"),
            "the multiwords chunker with tagForNotAddingTags must mark the phrase words: {flagged:?}"
        );
    }
}
