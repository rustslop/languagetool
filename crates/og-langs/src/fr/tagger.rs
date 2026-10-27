use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use std::path::Path;
use std::sync::Arc;

pub struct FrenchTagger {
    inner: Arc<og_tagger::GenericTagger>,
}

fn apostrophe_replacement(surface: &str) -> (Option<String>, Option<&'static str>) {
    if surface.chars().count() <= 1 {
        return (None, None);
    }
    if surface.contains('\u{2019}') {
        (
            Some(surface.replace('\u{2019}', "'")),
            Some("containsTypographicApostrophe"),
        )
    } else if surface.contains('\'') {
        (None, Some("containsTypewriterApostrophe"))
    } else {
        (None, None)
    }
}

fn normalized_atr(atr: AnalyzedTokenReadings) -> AnalyzedTokenReadings {
    let primary = atr.token();
    let surface = primary.token();
    let (replacement, chunk) = apostrophe_replacement(surface);
    let Some(replacement) = replacement else {
        if let Some(chunk) = chunk
            && atr.chunks().is_empty()
        {
            let mut with_chunk = atr;
            with_chunk.set_chunks(vec![chunk.to_string()]);
            return with_chunk;
        }
        return atr;
    };
    let lemma = atr
        .readings()
        .first()
        .and_then(|r| r.lemma())
        .map(str::to_string)
        .unwrap_or_else(|| replacement.clone());
    let pos_tags = primary.pos_tags().to_vec();
    let updated = AnalyzedToken::new(replacement, primary.start(), primary.end())
        .with_pos_tags(pos_tags)
        .with_lemma(lemma);
    let mut next = AnalyzedTokenReadings::new(updated).with_readings(atr.into_readings());
    if let Some(chunk) = chunk {
        next.set_chunks(vec![chunk.to_string()]);
    }
    next
}

impl FrenchTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            inner: Arc::new(og_tagger::GenericTagger::french(resource_dir)),
        }
    }

    pub fn shared_arc() -> Option<Arc<FrenchTagger>> {
        crate::weak_shared(|| {
            crate::engine::find_lt_resource_dir().map(|root| {
                Arc::new(FrenchTagger::from_resource_dir(
                    &root.join("fr/src/main/resources/org/languagetool/resource/fr"),
                ))
            })
        })
    }

    pub fn inner_is_word_tagged(&self, word: &str) -> bool {
        if word.chars().count() > 1 && word.contains('\u{2019}') {
            return self.inner.is_word_tagged(&word.replace('\u{2019}', "'"));
        }
        self.inner.is_word_tagged(word)
    }
}

impl Tagger for FrenchTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        og_tagger::Tagger::tag(self.inner.as_ref(), tokens)
            .into_iter()
            .map(normalized_atr)
            .collect()
    }
}

impl CoreTagger for FrenchTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        og_core::checker::Tagger::tag(self.inner.as_ref(), tokens);
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token();
            let (replacement, chunk) = apostrophe_replacement(surface);
            let Some(replacement) = replacement else {
                if let Some(chunk) = chunk
                    && atr.chunks().is_empty()
                {
                    atr.set_chunks(vec![chunk.to_string()]);
                }
                continue;
            };
            let lemma = atr
                .readings()
                .first()
                .and_then(|r| r.lemma())
                .map(str::to_string)
                .unwrap_or_else(|| replacement.clone());
            let pos_tags = primary.pos_tags().to_vec();
            let updated = AnalyzedToken::new(replacement, primary.start(), primary.end())
                .with_pos_tags(pos_tags)
                .with_lemma(lemma);
            let readings = atr.readings().to_vec();
            let mut next = AnalyzedTokenReadings::new(updated).with_readings(readings);
            if let Some(chunk) = chunk {
                next.set_chunks(vec![chunk.to_string()]);
            }
            *atr = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> FrenchTagger {
        let resource_dir = crate::fr::test_support::fr_resource_dir().expect("fr resource dir");
        FrenchTagger::from_resource_dir(&resource_dir)
    }

    fn surfaces(tokens: &[&str]) -> Vec<String> {
        og_tagger::Tagger::tag(&tagger(), tokens)
            .iter()
            .map(|atr| atr.token().token().to_string())
            .collect()
    }

    fn joined_with(tagger: &FrenchTagger, tokens: &[&str]) -> String {
        og_tagger::Tagger::tag(tagger, tokens)
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

    fn joined(tokens: &[&str]) -> String {
        joined_with(&tagger(), tokens)
    }

    #[test]
    fn typographic_apostrophe_surfaces_become_typewriter() {
        assert_eq!(surfaces(&["d’", "aujourd’hui"]), vec!["d'", "aujourd'hui"]);
        assert_eq!(surfaces(&["Fontaine-l’Évêque"]), vec!["Fontaine-l'Évêque"]);
        assert_eq!(surfaces(&["entr’ouvrions"]), vec!["entr'ouvrions"]);
        assert_eq!(surfaces(&["Qu’", "est"]), vec!["Qu'", "est"]);
    }

    #[test]
    fn java_frenchtagger_test_apostrophe_pins() {
        assert_eq!(
            joined(&["d’", "aujourd’hui"]),
            "d'/[de]D e sp|d'/[de]P -- aujourd'hui/[aujourd'hui]A"
        );
        assert_eq!(
            joined(&["d'", "aujourd’hui"]),
            "d'/[de]D e sp|d'/[de]P -- aujourd'hui/[aujourd'hui]A"
        );
        assert_eq!(
            joined(&["d’", "aujourd'hui"]),
            "d'/[de]D e sp|d'/[de]P -- aujourd'hui/[aujourd'hui]A"
        );
        assert_eq!(
            joined(&["Fontaine-l’Évêque"]),
            "Fontaine-l'Évêque/[Fontaine-l'Évêque]Z e sp"
        );
        assert_eq!(
            joined(&["entr’ouvrions"]),
            "entr'ouvrions/[entr'ouvrir]V ind impa 1 p|entr'ouvrions/[entr'ouvrir]V sub pres 1 p"
        );
    }

    #[test]
    fn java_frenchtagger_test_battery() {
        let tagger = tagger();
        let battery: &[(&[&str], &str)] = &[
            (
                &["C'", "est", "la", "vie"],
                "C'/[ce]R dem m s -- est/[est]N m s|est/[être]V etre ind pres 3 s -- \
                 la/[la]N m sp|la/[le]D f s|la/[le]R pers obj 3 f s -- vie/[vie]N f s",
            ),
            (
                &["Je", "ne", "parle", "pas", "français"],
                "Je/[je]R pers suj 1 s -- ne/[ne]A -- \
                 parle/[parler]V imp pres 2 s|parle/[parler]V ind pres 1 s|parle/[parler]V ind pres 3 s|\
                 parle/[parler]V sub pres 1 s|parle/[parler]V sub pres 3 s -- \
                 pas/[pas]A|pas/[pas]N m sp -- français/[français]J m sp|français/[français]N m sp",
            ),
            (&["blablabla"], "blablabla/[blablabla]N m s"),
            (
                &["passagère"],
                "passagère/[passager]J f s|passagère/[passager]N f s",
            ),
            (&["nonexistingword"], ""),
            (
                &["auto-mutile"],
                "auto-mutile/[auto-mutiler]V imp pres 2 s|auto-mutile/[auto-mutiler]V ind pres 1 s|\
                 auto-mutile/[auto-mutiler]V ind pres 3 s|auto-mutile/[auto-mutiler]V sub pres 1 s|\
                 auto-mutile/[auto-mutiler]V sub pres 3 s",
            ),
            (&["auto-mutilées"], "auto-mutilées/[auto-mutiler]V ppa f p"),
            (
                &["micro-plastiques"],
                "micro-plastiques/[micro-plastique]J e p|micro-plastiques/[micro-plastique]N e p",
            ),
            (&["sous-espace"], "sous-espace/[sous-espace]N m s"),
            (&["sous-corps"], "sous-corps/[sous-corps]N m sp"),
            (
                &["auto-empoisonnement"],
                "auto-empoisonnement/[auto-empoisonnement]N m s",
            ),
            (
                &["auto-équilibrage"],
                "auto-équilibrage/[auto-équilibrage]N m s",
            ),
            (&["Grand-Chambre"], "Grand-Chambre/[grand-chambre]N f s"),
            (
                &["Fontaine-l'Évêque"],
                "Fontaine-l'Évêque/[Fontaine-l'Évêque]Z e sp",
            ),
            (
                &["entr'ouvrions"],
                "entr'ouvrions/[entr'ouvrir]V ind impa 1 p|entr'ouvrions/[entr'ouvrir]V sub pres 1 p",
            ),
            (
                &["Penses", "-tu"],
                "Penses/[penser]V ind pres 2 s|Penses/[penser]V sub pres 2 s -- -tu/[tu]R pers suj 2 s",
            ),
            (&["Strauss-Kahn"], "Strauss-Kahn/[Strauss-Kahn]Z e sp"),
            (
                &["va", "-t", "'en"],
                "va/[aller]V imp pres 2 s|va/[aller]V ind pres 3 s -- -t/[te]R pers obj 2 s -- \
                 'en/[en]R pers obj 3 sp",
            ),
            (&["Al-Qaïda"], "Al-Qaïda/[Al-Qaïda]Z f sp"),
            (&["minitélévision"], "minitélévision/[minitélévision]N f s"),
            (&["soeur"], "soeur/[sœur]N f s"),
            (&["oeils-de-boeuf"], "oeils-de-boeuf/[œils-de-bœuf]N m p"),
            (
                &["Ç'", "avait"],
                "Ç'/[cela]R dem m s -- avait/[avoir]V avoir ind impa 3 s",
            ),
            (&["aéro-club"], "aéro-club/[aéro-club]N m s"),
            (&["Aéro-club"], "Aéro-club/[aéro-club]N m s"),
            (&["Aéro-Club"], "Aéro-Club/[aéro-club]N m s"),
            (&["AÉRO-CLUB"], "AÉRO-CLUB/[aéro-club]N m s"),
            (
                &["ex", "Le", "Pen"],
                "ex/[ex]A|ex/[ex]N e sp -- Le/[le]D m s|Le/[le]R pers obj 3 m s -- ",
            ),
            (
                &["Saint-Côme", "Le", "Mesnil"],
                "Saint-Côme/[Saint-Côme]Z e sp -- Le/[le]D m s|Le/[le]R pers obj 3 m s -- ",
            ),
            (
                &["Dieppe", "Le", "Tréport"],
                "Dieppe/[Dieppe]Z e sp -- Le/[le]D m s|Le/[le]R pers obj 3 m s -- ",
            ),
            (
                &[
                    "Caricaturistes",
                    "Les",
                    "fantassins",
                    "de",
                    "la",
                    "démocratie",
                ],
                "Caricaturistes/[caricaturiste]N e p -- Les/[le]D e p|Les/[les]R pers obj 3 e p -- \
                 fantassins/[fantassin]N m p -- de/[de]D e sp|de/[de]P -- \
                 la/[la]N m sp|la/[le]D f s|la/[le]R pers obj 3 f s -- démocratie/[démocratie]N f s",
            ),
        ];
        for (tokens, expected) in battery {
            assert_eq!(joined_with(&tagger, tokens), *expected, "tokens {tokens:?}");
        }
    }

    #[test]
    fn unknown_words_have_zero_readings_java_null_reading_surrogate() {
        let tagger = tagger();
        for surface in ["nonexistingword", "Pen", "Mesnil", "Tréport"] {
            let atr = &og_tagger::Tagger::tag(&tagger, &[surface])[0];
            assert!(atr.readings().is_empty(), "{surface}");
            assert_eq!(atr.token().token(), surface);
        }
    }

    #[test]
    fn apostrophe_chunks_follow_java_precedence() {
        let atr = &og_tagger::Tagger::tag(&tagger(), &["aujourd’hui"])[0];
        assert_eq!(atr.chunks(), &["containsTypographicApostrophe".to_string()]);
        let atr = &og_tagger::Tagger::tag(&tagger(), &["aujourd'hui"])[0];
        assert_eq!(atr.chunks(), &["containsTypewriterApostrophe".to_string()]);
        let atr = &og_tagger::Tagger::tag(&tagger(), &["’"])[0];
        assert!(atr.chunks().is_empty());
    }

    #[test]
    fn inner_is_word_tagged_normalizes_the_typographic_apostrophe() {
        let tagger = tagger();
        assert!(
            tagger.inner_is_word_tagged("aujourd’hui"),
            "the lookup must rewrite ’ to ' before hitting the dictionary"
        );
        assert!(tagger.inner_is_word_tagged("aujourd'hui"));
        assert!(!tagger.inner_is_word_tagged("aujourd’huixyz"));
    }

    #[test]
    fn core_tagger_tag_rewrites_apostrophes_in_place() {
        let surfaces = ["<S>", "aujourd’hui", "aujourd'hui", "plainword"];
        let mut atrs: Vec<AnalyzedTokenReadings> = surfaces
            .iter()
            .map(|s| AnalyzedTokenReadings::new(AnalyzedToken::new(*s, 0, s.len())))
            .collect();
        CoreTagger::tag(&tagger(), &mut atrs);
        assert_eq!(atrs[1].token().token(), "aujourd'hui");
        assert_eq!(
            atrs[1].chunks(),
            &["containsTypographicApostrophe".to_string()]
        );
        assert_eq!(atrs[2].token().token(), "aujourd'hui");
        assert_eq!(atrs[3].token().token(), "plainword");
        assert!(atrs[3].chunks().is_empty());
        assert!(
            atrs[1]
                .readings()
                .iter()
                .any(|r| r.lemma() == Some("aujourd'hui")),
            "the dictionary reading survives the surface rewrite: {:?}",
            atrs[1].readings()
        );
    }
}
