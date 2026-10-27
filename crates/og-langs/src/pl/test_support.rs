use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tokenizer::WordTokenizer as _;
use std::sync::Arc;

pub(crate) struct PolishPipeline {
    tokenizer: Arc<super::word_tokenizer::PolishWordTokenizer>,
    tagger: Arc<super::tagger::PolishTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl PolishPipeline {
    pub fn shared() -> std::sync::Arc<PolishPipeline> {
        static PIPE: og_core::releasable::Releasable<PolishPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                super::tagger::pl_resource_dir().expect("pl resource dir with polish.dict");
            let tagger =
                super::tagger::PolishTagger::shared_arc().expect("pl tagger with polish.dict");
            Some(std::sync::Arc::new(PolishPipeline {
                tokenizer: super::word_tokenizer::PolishWordTokenizer::shared(),
                tagger,
                disambiguator: super::build_pl_disambiguator(&resource_dir),
                splitter: og_tokenizer::srx::srx_splitter("pl"),
            }))
        })
        .expect("pl pipeline")
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

    pub(crate) fn build_sentence(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
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

pub(crate) fn analyze_undisambiguated(text: &str, start: usize, end: usize) -> AnalyzedSentence {
    PolishPipeline::shared().build_sentence(text, start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srx_languagemap_resolves_the_polish_ruleset_and_abbreviations_hold() {
        let splitter = og_tokenizer::srx::srx_splitter("pl").expect("pl SRX rules must resolve");
        use og_tokenizer::SentenceTokenizer as _;
        let sentences =
            splitter.split("Wczoraj kupiliśmy chleb, masło itp. Dziś sprzedaliśmy wszystko.");
        assert_eq!(
            sentences.len(),
            2,
            "itp. abbreviation must not break the sentence"
        );
        let sentences = splitter.split("Np. Kowalski napisał dwa donosy.");
        assert_eq!(sentences.len(), 1);
        let sentences = splitter.split("Dzień dobry. Nazywam się Marcin.");
        assert_eq!(sentences.len(), 2);
    }

    #[test]
    fn srx_abbreviated_month_and_ordinal_do_not_split() {
        let splitter = og_tokenizer::srx::srx_splitter("pl").expect("pl SRX rules must resolve");
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split("Było to w dniu 11 listopada 1918 r.");
        assert_eq!(sentences.len(), 1, "r. ordinal abbreviation holds");
        let sentences = splitter.split("Poznał ją w 1918 r., a potem wyjechał.");
        assert_eq!(sentences.len(), 1);
    }

    #[test]
    fn interp_comma_readings_stay_on_punctuation_only() {
        let text = "To jest co\u{15b}, co wiedzia\u{142}em od dawna.";
        let sentence = PolishPipeline::shared().analyze_range(text, 0, text.len());
        for t in sentence.non_whitespace_tokens() {
            let carries = t
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|x| x == "interp:comma"));
            let surface = t.token().token();
            assert_eq!(
                carries,
                surface == ",",
                "{surface:?} must carry interp:comma only on the comma: {:?}",
                t.readings()
                    .iter()
                    .map(|r| r.pos_tags().to_vec())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn normalization_rewrites_the_bare_dash_char_class_for_the_rust_regex_crate() {
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .expect("disambiguation.xml");
        let normalized = crate::pl::normalize_disambiguation_xml(&xml);
        assert!(
            normalized != xml,
            "the unescaped dash class must be rewritten"
        );
        assert!(
            normalized.contains(
                "[!-/:-@\\[-`{-~\\-\\u{2212}"
                    .replace("\\u{2212}", "\u{2212}")
                    .as_str()
            ),
            "the Java ASCII punct class with the escaped dash form must be present"
        );
        assert_eq!(
            normalized.matches("PUNCT_NO_DOT").count(),
            xml.matches("PUNCT_NO_DOT").count()
        );
    }

    #[test]
    fn pipeline_tags_through_the_polish_tagger_and_disambiguator() {
        let sentences = PolishPipeline::shared().analyze_text("Były to rozmowy z kobietami.");
        assert_eq!(sentences.len(), 1);
        let first = &sentences[0];
        let byly = first
            .tokens()
            .iter()
            .find(|t| t.token().token() == "Były")
            .expect("Były token");
        assert_eq!(
            byly.readings().len(),
            1,
            "the pl disambiguation.xml resolves Były before a verb complement: {:?}",
            byly.readings()
        );
        assert!(
            byly.readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t.starts_with("verb:"))),
            "{:?}",
            byly.readings()
        );
        let undisambiguated = crate::pl::tagger::PolishTagger::shared_arc().expect("pl tagger");
        let raw = undisambiguated.analyzed_tokens_at("Były", 0, 0);
        assert!(
            raw.len() > 1,
            "the tagger alone still offers the fused readings: {raw:?}"
        );
    }
}
