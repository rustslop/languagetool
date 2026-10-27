use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tokenizer::WordTokenizer as _;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn fr_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("fr/src/main/resources/org/languagetool/resource/fr"))
}

pub(crate) fn fr_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("fr/src/main/resources/org/languagetool/rules/fr"))
}

pub(crate) fn apostrophe_chunk(surface: &str) -> Option<&'static str> {
    if surface.chars().count() <= 1 {
        return None;
    }
    if surface.contains('\u{2019}') {
        Some("containsTypographicApostrophe")
    } else if surface.contains('\'') {
        Some("containsTypewriterApostrophe")
    } else {
        None
    }
}

pub(crate) struct FrenchPipeline {
    tokenizer: og_tokenizer::FrenchWordTokenizer,
    tagger: Arc<crate::fr::tagger::FrenchTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl FrenchPipeline {
    pub fn shared() -> std::sync::Arc<FrenchPipeline> {
        static PIPE: og_core::releasable::Releasable<FrenchPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = fr_resource_dir().expect("fr resource dir with french.dict");
            let tagger = Arc::new(crate::fr::tagger::FrenchTagger::from_resource_dir(
                &resource_dir,
            ));
            let tokenizer = og_tokenizer::FrenchWordTokenizer::new(Arc::new({
                let tagger = tagger.clone();
                move |w: &str| tagger.inner_is_word_tagged(w)
            }));
            let disambiguator = crate::fr::build_fr_disambiguator(&resource_dir);
            Some(std::sync::Arc::new(FrenchPipeline {
                tokenizer,
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("fr"),
            }))
        })
        .expect("fr pipeline")
    }

    pub fn tagger_ref(&self) -> &crate::fr::tagger::FrenchTagger {
        &self.tagger
    }

    pub fn tokenizer_ref(&self) -> &og_tokenizer::FrenchWordTokenizer {
        &self.tokenizer
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
        let mut tokens = Vec::new();
        let start_token = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        tokens.push(start_token);
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
            let start = tokens_mut[token_idx].token().start();
            let end = tokens_mut[token_idx].token().end();
            if atr.readings().is_empty() {
                let primary = AnalyzedToken::new(atr.token().token(), start, end);
                let mut next = AnalyzedTokenReadings::new(primary);
                if !atr.chunks().is_empty() {
                    next.set_chunks(atr.chunks().to_vec());
                }
                tokens_mut[token_idx] = next;
                continue;
            }
            let chunks = atr.chunks().to_vec();
            let text = atr.token().token().to_string();
            let readings = atr.into_readings();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings[0].lemma().unwrap_or(text.as_str()).to_string();
            let mut merged = AnalyzedTokenReadings::new(
                AnalyzedToken::new(text, start, end)
                    .with_pos_tags(all_tags)
                    .with_lemma(lemma),
            )
            .with_readings(readings);
            if !chunks.is_empty() {
                merged.set_chunks(chunks);
            }
            tokens_mut[token_idx] = merged;
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

pub(crate) fn analyze_french(text: &str) -> Vec<AnalyzedSentence> {
    FrenchPipeline::shared().analyze_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings_join(text: &str) -> String {
        let sentences = analyze_french(text);
        let parts: Vec<String> = sentences
            .iter()
            .flat_map(|s| s.tokens().iter())
            .filter(|t| {
                !t.is_whitespace() && !t.token().token().is_empty() && t.token().token() != "<S>"
            })
            .map(|t| {
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .filter(|r| !r.pos_tags().is_empty() && r.pos_tags()[0] != "SENT_END")
                    .map(|r| {
                        format!(
                            "{}/[{}]{}",
                            r.lemma().unwrap_or("null"),
                            r.pos_tags().join("|"),
                            ""
                        )
                    })
                    .collect();
                format!("{}={}", t.token().token(), readings.join("|"))
            })
            .collect();
        parts.join(" ; ")
    }

    fn sorted_readings(text: &str) -> String {
        readings_join(text)
            .split(" ; ")
            .map(|part| match part.split_once('=') {
                Some((surface, readings)) => {
                    let mut items: Vec<&str> = readings.split('|').collect();
                    items.sort_unstable();
                    format!("{}={}", surface, items.join("|"))
                }
                None => part.to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ; ")
    }

    #[test]
    fn global_disambiguation_marks_user_mentions_ignored_by_speller() {
        let sentence = &analyze_french("Merci @utilisateur pour ton aide.")[0];
        let mention = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == "@utilisateur")
            .expect("@utilisateur token");
        assert!(
            mention.is_ignored_by_speller(),
            "disambiguation-global.xml GLOBAL_IGNORE_USER_MENTION must ignore-spell @utilisateur"
        );
    }

    #[test]
    fn java_frenchrule_disambiguator_test_pins() {
        assert_eq!(
            readings_join("Quand j'étais petit, je jouais au football."),
            "Quand=quand/[A inte]|quand/[C sub] ; j'=je/[R pers suj 1 s] ; étais=être/[V etre ind impa 1 s] \
             ; petit=petit/[J m s] ; ,=,/[M nonfin]|,/[M nonfin] ; je=je/[R pers suj 1 s] ; jouais=jouer/[V ind impa 1 s] \
             ; au=à+le/[P+D m s] ; football=football/[N m s] ; .=./[M fin]|./[M fin]"
        );
        assert_eq!(
            readings_join("Je suis d'abord allé au laboratoire."),
            "Je=je/[R pers suj 1 s] ; suis=être/[V etre ind pres 1 s] ; d'=d'abord/[A] \
             ; abord=d'abord/[A] ; allé=aller/[V ppa m s] ; au=à+le/[P+D m s] \
             ; laboratoire=laboratoire/[N m s] ; .=./[M fin]|./[M fin]"
        );
    }

    #[test]
    fn java_frenchtagger_test_pins() {
        assert_eq!(
            sorted_readings("passagère"),
            "passagère=passager/[J f s]|passager/[N f s]"
        );
        assert_eq!(readings_join("blablabla"), "blablabla=blablabla/[N m s]");
        assert_eq!(
            readings_join("entr'ouvrions"),
            "entr'ouvrions=entr'ouvrir/[V ind impa 1 p]|entr'ouvrir/[V sub pres 1 p]"
        );
        assert_eq!(
            readings_join("auto-mutilées"),
            "auto-mutilées=auto-mutiler/[V ppa f p]"
        );
        assert_eq!(
            readings_join("Grand-Chambre"),
            "Grand-Chambre=grand-chambre/[N f s]"
        );
        assert_eq!(readings_join("soeur"), "soeur=sœur/[N f s]");
    }

    fn tagger_only(tokens: &[&str]) -> String {
        let tagged = og_tagger::Tagger::tag(FrenchPipeline::shared().tagger_ref(), tokens);
        tagged
            .iter()
            .map(|atr| {
                let mut readings: Vec<String> = atr
                    .readings()
                    .iter()
                    .map(|r| {
                        format!(
                            "{}/[{}]{}",
                            atr.token().token(),
                            r.lemma().unwrap_or("null"),
                            if r.pos_tags().is_empty() {
                                "null".to_string()
                            } else {
                                r.pos_tags().join("|")
                            }
                        )
                    })
                    .collect();
                readings.sort_unstable();
                readings.join("|")
            })
            .collect::<Vec<_>>()
            .join(" -- ")
    }

    #[test]
    fn java_frenchtagger_test_tagger_only_pins() {
        assert_eq!(
            tagger_only(&["C'", "est", "la", "vie"]),
            "C'/[ce]R dem m s -- est/[est]N m s|est/[être]V etre ind pres 3 s \
             -- la/[la]N m sp|la/[le]D f s|la/[le]R pers obj 3 f s -- vie/[vie]N f s"
        );
        assert_eq!(
            tagger_only(&["Je", "ne", "parle", "pas", "français"]),
            "Je/[je]R pers suj 1 s -- ne/[ne]A \
             -- parle/[parler]V imp pres 2 s|parle/[parler]V ind pres 1 s|parle/[parler]V ind pres 3 s|parle/[parler]V sub pres 1 s|parle/[parler]V sub pres 3 s \
             -- pas/[pas]A|pas/[pas]N m sp -- français/[français]J m sp|français/[français]N m sp"
        );
        assert_eq!(
            tagger_only(&["d'", "aujourd'hui"]),
            "d'/[de]D e sp|d'/[de]P -- aujourd'hui/[aujourd'hui]A"
        );
        assert_eq!(
            tagger_only(&["d’", "aujourd’hui"]),
            "d'/[de]D e sp|d'/[de]P -- aujourd'hui/[aujourd'hui]A"
        );
        assert_eq!(
            tagger_only(&["Fontaine-l'Évêque"]),
            "Fontaine-l'Évêque/[Fontaine-l'Évêque]Z e sp"
        );
        assert_eq!(
            tagger_only(&["Fontaine-l’Évêque"]),
            "Fontaine-l'Évêque/[Fontaine-l'Évêque]Z e sp"
        );
        assert_eq!(
            tagger_only(&["Strauss-Kahn"]),
            "Strauss-Kahn/[Strauss-Kahn]Z e sp"
        );
        assert_eq!(
            tagger_only(&["Ç'", "avait"]),
            "Ç'/[cela]R dem m s -- avait/[avoir]V avoir ind impa 3 s"
        );
        assert_eq!(
            tagger_only(&["Penses", "-tu"]),
            "Penses/[penser]V ind pres 2 s|Penses/[penser]V sub pres 2 s -- -tu/[tu]R pers suj 2 s"
        );
        assert_eq!(
            tagger_only(&["va", "-t", "'en"]),
            "va/[aller]V imp pres 2 s|va/[aller]V ind pres 3 s -- -t/[te]R pers obj 2 s -- 'en/[en]R pers obj 3 sp"
        );
        assert_eq!(tagger_only(&["Al-Qaïda"]), "Al-Qaïda/[Al-Qaïda]Z f sp");
        assert_eq!(
            tagger_only(&["micro-plastiques"]),
            "micro-plastiques/[micro-plastique]J e p|micro-plastiques/[micro-plastique]N e p"
        );
        assert_eq!(
            tagger_only(&["sous-espace"]),
            "sous-espace/[sous-espace]N m s"
        );
        assert_eq!(
            tagger_only(&["sous-corps"]),
            "sous-corps/[sous-corps]N m sp"
        );
        assert_eq!(
            tagger_only(&["minitélévision"]),
            "minitélévision/[minitélévision]N f s"
        );
        assert_eq!(
            tagger_only(&["oeils-de-boeuf"]),
            "oeils-de-boeuf/[œils-de-bœuf]N m p"
        );
        assert_eq!(tagger_only(&["peut-être"]), "peut-être/[peut-être]A");
        assert_eq!(tagger_only(&["aéro-club"]), "aéro-club/[aéro-club]N m s");
        assert_eq!(tagger_only(&["Aéro-club"]), "Aéro-club/[aéro-club]N m s");
        assert_eq!(tagger_only(&["Aéro-Club"]), "Aéro-Club/[aéro-club]N m s");
        assert_eq!(tagger_only(&["AÉRO-CLUB"]), "AÉRO-CLUB/[aéro-club]N m s");
    }

    #[test]
    fn typewriter_apostrophe_chunk_tags_follow_frenchtagger() {
        let sentences = analyze_french("Ils ont beaucoup d'enfants. Va-t'en d’ici.");
        let chunks: Vec<Vec<String>> = sentences
            .iter()
            .flat_map(|s| s.tokens().iter())
            .map(|t| t.chunks().to_vec())
            .collect();
        let flat: Vec<String> = chunks.into_iter().flatten().collect();
        assert!(flat.contains(&"containsTypewriterApostrophe".to_string()));
        assert!(flat.contains(&"containsTypographicApostrophe".to_string()));
    }

    #[test]
    fn single_apostrophe_token_carries_no_chunk() {
        assert_eq!(apostrophe_chunk("'"), None);
        assert_eq!(
            apostrophe_chunk("d’aujourd’hui"),
            Some("containsTypographicApostrophe")
        );
        assert_eq!(
            apostrophe_chunk("aujourd'hui"),
            Some("containsTypewriterApostrophe")
        );
    }

    #[test]
    fn srx_french_sentence_splitting() {
        let ranges = FrenchPipeline::shared().sentence_ranges("M. Dupont arrive. Il part.");
        assert_eq!(ranges.len(), 2, "{ranges:?}");
    }
}
