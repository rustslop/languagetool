use crate::Tagger;
use crate::morfologik::MorfologikDictionary;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type Readings = Vec<(&'static str, &'static str)>;

const FRENCH_AMBIGUOUS_TOKENS: [&str; 11] = [
    "-Le", "-Les", "-La", "-Elle", "-Elles", "-On", "-Tu", "-Vous", "-Il", "-Ils", "-Ce",
];

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_alphabetic() || c.is_lowercase()),
        _ => false,
    }
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn is_java_space_char(c: char) -> bool {
    matches!(
        c,
        '\u{20}' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

fn title_case_iterating_chars(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut convert_next = true;
    for ch in word.chars() {
        if is_java_space_char(ch) || ch == '-' {
            out.push(ch);
            convert_next = true;
        } else if convert_next {
            out.extend(ch.to_uppercase());
            convert_next = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

fn is_emoji(word: &str) -> bool {
    let utf16_len: usize = word.chars().map(char::len_utf16).sum();
    utf16_len > 1 && word.chars().count() != utf16_len && !is_word_for_speller(word)
}

fn is_word_for_speller(word: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[\p{L}0-9\p{P}\p{Zs}]+$").unwrap())
        .is_match(word)
}

enum PrefixStage {
    ForVerbs,
    ForNounAdjInflected,
    NounAdj,
}

fn french_prefix_match(stage: PrefixStage, word: &str) -> Option<(String, String)> {
    use std::sync::OnceLock;
    static FOR_VERBS: OnceLock<regex::Regex> = OnceLock::new();
    static FOR_NOUN_ADJ_INFLECTED: OnceLock<regex::Regex> = OnceLock::new();
    static NOUN_ADJ: OnceLock<regex::Regex> = OnceLock::new();
    let re = match stage {
        PrefixStage::ForVerbs => FOR_VERBS.get_or_init(|| {
            regex::Regex::new(
                r"(?i)^(auto|auto-|re-|sur-)([^-].*[aeiouêàéèíòóïü].+[aeiouêàéèíòóïü].*)$",
            )
            .unwrap()
        }),
        PrefixStage::ForNounAdjInflected => FOR_NOUN_ADJ_INFLECTED.get_or_init(|| {
            regex::Regex::new(
                r"(?i)^(mini|méga)([^-].*[aeiouêàéèíòóïü].+[aeiouêàéèíòóïü].*)$",
            )
            .unwrap()
        }),
        PrefixStage::NounAdj => NOUN_ADJ.get_or_init(|| {
            regex::Regex::new(
                r"(?i)^(post-|sur-|mini-|méga-|demi-|péri-|anti-|géo-|nord-|sud-|néo-|méga-|ultra-|pro-|inter-|micro-|macro-|sous-|haut-|auto-|ré-|pré-|super-|vice-|hyper-|proto-|grand-|pseudo-)(.+)$",
            )
            .unwrap()
        }),
    };
    let caps = re.captures(word)?;
    let prefix = caps.get(1)?.as_str().to_lowercase();
    let stem = caps.get(2)?.as_str().to_lowercase();
    Some((prefix, stem))
}

fn french_tag_is_verb(tag: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^V .+$").unwrap())
        .is_match(tag)
}

fn french_tag_is_noun_adj(tag: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(?:[NJ] .+|V ppa.*)$").unwrap())
        .is_match(tag)
}

pub struct GenericTagger {
    dictionary: Option<Arc<MorfologikDictionary>>,
    manual: HashMap<Box<str>, Readings>,
    removals: HashMap<Box<str>, Readings>,
    french_normalization: bool,
    russian_normalization: bool,
}

const ACUTE_STRESSED: [&str; 9] = [
    "\u{435}\u{301}",
    "\u{43E}\u{301}",
    "\u{430}\u{301}",
    "\u{443}\u{301}",
    "\u{438}\u{301}",
    "\u{44E}\u{301}",
    "\u{44B}\u{301}",
    "\u{44D}\u{301}",
    "\u{44F}\u{301}",
];

fn russian_may_missing_yo_candidate(word: &str) -> bool {
    word.encode_utf16().count() > 1
        && !word.contains('ё')
        && !word.contains('Ё')
        && (word.contains('е') || word.contains('Е'))
        && !ACUTE_STRESSED.iter().any(|s| word.contains(s))
}

fn russian_strip_stress(word: &str) -> String {
    word.replace("\u{43E}\u{301}", "\u{43E}")
        .replace("\u{430}\u{301}", "\u{430}")
        .replace("\u{435}\u{301}", "\u{435}")
        .replace("\u{443}\u{301}", "\u{443}")
        .replace("\u{438}\u{301}", "\u{438}")
        .replace("\u{44B}\u{301}", "\u{44B}")
        .replace("\u{44D}\u{301}", "\u{44D}")
        .replace("\u{44E}\u{301}", "\u{44E}")
        .replace("\u{44F}\u{301}", "\u{44F}")
        .replace("\u{43E}\u{300}", "\u{43E}")
        .replace("\u{430}\u{300}", "\u{430}")
        .replace("\u{435}\u{300}", "\u{435}")
        .replace("\u{443}\u{300}", "\u{443}")
        .replace("\u{45D}", "\u{438}")
        .replace("\u{44B}\u{300}", "\u{44B}")
        .replace("\u{44D}\u{300}", "\u{44D}")
        .replace("\u{44E}\u{300}", "\u{44E}")
        .replace("\u{44F}\u{300}", "\u{44F}")
        .replace("\u{2BC}", "\u{44A}")
}

impl GenericTagger {
    pub fn french(resource_dir: &Path) -> Self {
        let mut tagger = Self::from_resource_dir(resource_dir, "french");
        tagger.french_normalization = true;
        tagger
    }

    pub fn russian(resource_dir: &Path) -> Self {
        let mut tagger = Self::from_resource_dir(resource_dir, "russian");
        tagger.russian_normalization = true;
        tagger
    }

    pub fn from_resource_dir(resource_dir: &Path, dict_stem: &str) -> Self {
        let dictionary = shared_dictionary(resource_dir, dict_stem);
        let read = |name: &str| std::fs::read_to_string(resource_dir.join(name)).ok();
        let added = ["added.txt", "added_custom.txt"]
            .iter()
            .filter_map(|f| read(f))
            .collect::<Vec<_>>()
            .join("\n");
        let removed = ["removed.txt", "removed_custom.txt"]
            .iter()
            .filter_map(|f| std::fs::read_to_string(resource_dir.join(f)).ok())
            .collect::<Vec<_>>()
            .join("\n");
        let mut tagger = Self {
            dictionary,
            manual: HashMap::new(),
            removals: HashMap::new(),
            french_normalization: false,
            russian_normalization: false,
        };
        tagger.load_manual_data(&added, &removed);
        tagger
    }

    pub fn has_dictionary(&self) -> bool {
        self.dictionary.is_some()
    }

    fn load_manual_data(&mut self, added: &str, removed: &str) {
        for (target, data) in [(&mut self.manual, added), (&mut self.removals, removed)] {
            for line in data.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.splitn(3, '\t');
                let (Some(form), Some(lemma), Some(tag)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let form_leaked: &'static str = Box::leak(form.to_string().into_boxed_str());
                let lemma_leaked: &'static str = Box::leak(lemma.to_string().into_boxed_str());
                let tag_leaked: &'static str = Box::leak(tag.trim().to_string().into_boxed_str());
                target
                    .entry(form_leaked.into())
                    .or_default()
                    .push((lemma_leaked, tag_leaked));
            }
        }
    }

    pub(crate) fn lookup(&self, word: &str) -> Vec<(&'static str, &'static str)> {
        let mut readings: Readings = self
            .manual
            .get(word)
            .map(|v| v.to_vec())
            .unwrap_or_default();
        if let Some(dictionary) = &self.dictionary {
            readings.extend(dictionary.lookup(word));
        }
        if let Some(removals) = self.removals.get(word) {
            readings.retain(|(lemma, tag)| {
                !removals
                    .iter()
                    .any(|(r_lemma, r_tag)| r_lemma == lemma && r_tag == tag)
            });
        }
        readings
    }

    fn readings_for(
        &self,
        word: &str,
        start: usize,
        end: usize,
        display: &str,
    ) -> Vec<AnalyzedToken> {
        self.lookup(word)
            .into_iter()
            .map(|(lemma, tag)| {
                AnalyzedToken::new(display, start, end)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![tag.to_string()])
            })
            .collect()
    }

    fn tag_word_inner(
        &self,
        surface: &str,
        start: usize,
        end: usize,
        display: &str,
    ) -> Vec<AnalyzedToken> {
        let lower = surface.to_lowercase();
        let is_lowercase = surface == lower;
        let mut readings = self.readings_for(surface, start, end, display);
        if !is_lowercase {
            readings.extend(self.readings_for(&lower, start, end, display));
        }
        readings
    }

    fn russian_tag_word(
        &self,
        surface: &str,
        start: usize,
        end: usize,
    ) -> (Vec<AnalyzedToken>, bool) {
        let candidate = russian_may_missing_yo_candidate(surface);
        let normalized = russian_strip_stress(surface);
        let readings = self.tag_word_inner(&normalized, start, end, surface);
        let may_missing_yo = candidate
            && !self
                .lookup(&normalized.to_lowercase().replace('\u{435}', "\u{451}"))
                .is_empty();
        (readings, may_missing_yo)
    }

    fn french_tag_word(
        &self,
        word: &str,
        original: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let lower = word.to_lowercase();
        let is_start_upper = is_capitalized_word(word);
        let is_all_upper = is_all_uppercase(word);
        let is_hyphenated_title_case = !FRENCH_AMBIGUOUS_TOKENS.contains(&original)
            && original.contains('-')
            && *original == title_case_iterating_chars(&lower);
        let mut readings = self.readings_for(word, start, end, original);
        if is_all_upper || is_start_upper || is_hyphenated_title_case {
            readings.extend(self.readings_for(&lower, start, end, original));
        }
        if readings.is_empty() && is_all_upper {
            let first_upper = title_case_iterating_chars(&lower);
            readings.extend(self.readings_for(&first_upper, start, end, original));
        }
        if readings.is_empty() {
            readings.extend(self.french_additional_tags(word, start, end, original));
        }
        readings
    }

    fn french_additional_tags(
        &self,
        word: &str,
        start: usize,
        end: usize,
        original: &str,
    ) -> Vec<AnalyzedToken> {
        let mut out = Vec::new();
        if let Some((prefix, stem)) = french_prefix_match(PrefixStage::ForVerbs, word) {
            for (lemma, tag) in self.lookup(&stem) {
                if french_tag_is_verb(tag) {
                    out.push(
                        AnalyzedToken::new(original, start, end)
                            .with_lemma(format!("{prefix}{lemma}"))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        if let Some((prefix, stem)) = french_prefix_match(PrefixStage::ForNounAdjInflected, word) {
            for (lemma, tag) in self.lookup(&stem) {
                if french_tag_is_noun_adj(tag) {
                    out.push(
                        AnalyzedToken::new(original, start, end)
                            .with_lemma(format!("{prefix}{lemma}"))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        if let Some((prefix, stem)) = french_prefix_match(PrefixStage::NounAdj, word) {
            for (lemma, tag) in self.lookup(&stem) {
                if french_tag_is_noun_adj(tag) {
                    out.push(
                        AnalyzedToken::new(original, start, end)
                            .with_lemma(format!("{prefix}{lemma}"))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
        }
        out
    }

    pub fn tag_word(&self, surface: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        if self.french_normalization {
            let normalized = if surface.chars().count() > 1 {
                surface.replace('\u{2019}', "'")
            } else {
                surface.to_string()
            };
            let mut readings = self.french_tag_word(&normalized, surface, start, end);
            if readings.is_empty() && normalized.to_lowercase().contains("oe") {
                let oe = normalized.replace("oe", "\u{153}").replace("OE", "\u{152}");
                readings = self.french_tag_word(&oe, surface, start, end);
            }
            if readings.is_empty() && is_emoji(&normalized) {
                readings.push(
                    AnalyzedToken::new(surface, start, end)
                        .with_lemma("_emoji_".to_string())
                        .with_pos_tags(vec!["_emoji_".to_string()]),
                );
            }
            return readings;
        }
        self.tag_word_inner(surface, start, end, surface)
    }

    fn apostrophe_chunk(&self, surface: &str) -> Option<&'static str> {
        if !self.french_normalization || surface.chars().count() <= 1 {
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

    pub fn is_word_tagged(&self, word: &str) -> bool {
        if self.french_normalization {
            let normalized = if word.chars().count() > 1 {
                word.replace('\u{2019}', "'")
            } else {
                word.to_string()
            };
            let mut readings = self.french_tag_word(&normalized, &normalized, 0, 0);
            if readings.is_empty() && normalized.to_lowercase().contains("oe") {
                let oe = normalized.replace("oe", "\u{153}").replace("OE", "\u{152}");
                readings = self.french_tag_word(&oe, &oe, 0, 0);
            }
            return !readings.is_empty();
        }
        !self.lookup(word).is_empty()
    }
}

pub fn shared_dictionary(
    resource_dir: &Path,
    dict_stem: &str,
) -> Option<Arc<MorfologikDictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<HashMap<PathBuf, std::sync::Weak<MorfologikDictionary>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = resource_dir.join(format!("{dict_stem}.dict"));
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(&key).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let dictionary = Arc::new(MorfologikDictionary::load(
        &key,
        &resource_dir.join(format!("{dict_stem}.info")),
    )?);
    guard.insert(key, std::sync::Arc::downgrade(&dictionary));
    Some(dictionary)
}

impl Tagger for GenericTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for token_text in tokens {
            let start = byte_offset;
            let end = start + token_text.len();
            let (readings, may_missing_yo) = if self.russian_normalization {
                self.russian_tag_word(token_text, start, end)
            } else {
                (self.tag_word(token_text, start, end), false)
            };
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| token_text.to_string());
            let primary = AnalyzedToken::new(*token_text, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut atr = AnalyzedTokenReadings::new(primary).with_readings(readings);
            if let Some(chunk) = self.apostrophe_chunk(token_text) {
                atr.set_chunks(vec![chunk.to_string()]);
            } else if may_missing_yo {
                atr.set_chunks(vec!["MayMissingYO".to_string()]);
            }
            results.push(atr);
            byte_offset = end;
        }
        results
    }
}

impl og_core::checker::Tagger for GenericTagger {
    fn tag(&self, tokens: &mut [og_core::AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let (readings, may_missing_yo) = if self.russian_normalization {
                self.russian_tag_word(&surface, start, end)
            } else {
                (self.tag_word(&surface, start, end), false)
            };
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| surface.clone());
            let apostrophe_chunk = self.apostrophe_chunk(&surface);
            let updated = AnalyzedToken::new(surface, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut next = AnalyzedTokenReadings::new(updated).with_readings(readings);
            if let Some(chunk) = apostrophe_chunk {
                next.set_chunks(vec![chunk.to_string()]);
            } else if may_missing_yo {
                next.set_chunks(vec!["MayMissingYO".to_string()]);
            }
            *atr = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn french_prefix_stages_tag_verbs_and_nouns() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);

        let verb = tagger.tag_word("remaquiller", 0, 11);
        assert_eq!(
            verb.iter()
                .map(|r| r.pos_tags().join(","))
                .collect::<Vec<_>>(),
            vec!["V inf".to_string()]
        );
        assert_eq!(verb[0].lemma(), Some("remaquiller"));

        for (word, tag) in [
            ("minicrevasse", "N f s"),
            ("mégaorchestre", "N m s"),
            ("micro-cahier", "N m s"),
            ("anti-garage", "N m s"),
            ("post-baguette", "N f s"),
            ("pseudo-orchestre", "N m s"),
        ] {
            let readings = tagger.tag_word(word, 0, word.len());
            assert!(
                readings
                    .iter()
                    .any(|r| r.pos_tags().contains(&tag.to_string())),
                "{word}: {:?}",
                readings
                    .iter()
                    .map(|r| r.pos_tags().to_vec())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn french_oe_ligature_falls_back_to_the_dict_form() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);
        let coeur = tagger.tag_word("coeur", 0, 5);
        assert!(coeur.iter().any(|r| r.lemma() == Some("cœur")));
        assert!(
            coeur
                .iter()
                .any(|r| r.pos_tags().contains(&"N m s".to_string()))
        );
        assert!(tagger.is_word_tagged("coeur"));
        assert!(tagger.is_word_tagged("maison"));
        assert!(!tagger.is_word_tagged("qwxz"));
    }

    #[test]
    fn french_checker_tagging_marks_apostrophes_by_kind() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);
        let mut tokens = vec![
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("<S>", 0, 0)),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("l’été", 0, 7)),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("l'été", 8, 13)),
        ];
        og_core::checker::Tagger::tag(&tagger, &mut tokens);
        assert_eq!(
            tokens[1].chunks(),
            &["containsTypographicApostrophe".to_string()]
        );
        assert_eq!(
            tokens[2].chunks(),
            &["containsTypewriterApostrophe".to_string()]
        );
        assert_eq!(tokens[0].chunks(), &[] as &[String]);
    }

    #[test]
    fn shared_dictionary_caches_by_path() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let first = shared_dictionary(&dir, "french").expect("shared french dict");
        let second = shared_dictionary(&dir, "french").expect("shared french dict again");
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }

    fn resource_dir(lang: &str) -> Option<std::path::PathBuf> {
        [
            format!("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"),
            format!("languagetool/languagetool-language-modules/{lang}/src/main/resources/org/languagetool/resource/{lang}"),
        ]
        .into_iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
    }

    #[test]
    fn russian_tags_via_morfologik_dict() {
        let Some(dir) = resource_dir("ru") else {
            return;
        };
        let tagger = GenericTagger::from_resource_dir(&dir, "russian");
        assert!(tagger.has_dictionary());
        let readings = tagger.tag_word("дом", 0, 6);
        assert!(!readings.is_empty(), "expected tags for 'дом'");
    }

    #[test]
    fn russian_normalization_sets_may_missing_yo() {
        let Some(dir) = resource_dir("ru") else {
            return;
        };
        let tagger = GenericTagger::russian(&dir);
        let tagged = Tagger::tag(&tagger, &["слез", "дом"]);
        let yo_words: Vec<bool> = tagged
            .iter()
            .map(|atr| atr.chunks().iter().any(|c| c == "MayMissingYO"))
            .collect();
        assert_eq!(yo_words, vec![true, false], "{yo_words:?}");

        let tagged = Tagger::tag(&tagger, &["слезу"]);
        assert!(
            !tagged[0].chunks().iter().any(|c| c == "MayMissingYO"),
            "no ё variant in dict means no MayMissingYO"
        );
    }

    #[test]
    fn russian_stress_marks_are_stripped_for_lookup() {
        let Some(dir) = resource_dir("ru") else {
            return;
        };
        let plain = GenericTagger::from_resource_dir(&dir, "russian");
        let stressed = GenericTagger::russian(&dir);
        let plain_readings = plain.tag_word("дом", 0, 6);
        let stressed_readings = {
            let (readings, _) = stressed.russian_tag_word("до\u{301}м", 0, 6);
            readings
        };
        assert_eq!(
            plain_readings.len(),
            stressed_readings.len(),
            "stressed variant tags like the plain form"
        );
    }

    #[test]
    fn french_emoji_and_prefix_tags() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);
        let emoji = tagger.tag_word("\u{1F600}", 0, 4);
        assert!(
            emoji
                .iter()
                .any(|r| r.pos_tags().contains(&"_emoji_".to_string())),
            "emoji word gets the _emoji_ tag"
        );
        let readings = tagger.tag_word("miniatures", 0, 11);
        assert!(!readings.is_empty());
    }

    #[test]
    fn french_tag_path_sets_apostrophe_chunks() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);
        let tagged = Tagger::tag(
            &tagger,
            &["eau", "aujourd'hui", "d\u{2019}aujourd\u{2019}hui", "'"],
        );
        let chunks: Vec<Vec<&str>> = tagged
            .iter()
            .map(|atr| atr.chunks().iter().map(String::as_str).collect())
            .collect();
        assert_eq!(chunks[0], Vec::<&str>::new(), "no apostrophe, no chunk");
        assert_eq!(
            chunks[1],
            vec!["containsTypewriterApostrophe"],
            "Java FrenchTagger sets the typewriter chunk"
        );
        assert_eq!(
            chunks[2],
            vec!["containsTypographicApostrophe"],
            "typographic wins when both apostrophes could apply"
        );
        assert_eq!(chunks[3], Vec::<&str>::new(), "single-char token gets none");

        let plain = GenericTagger::from_resource_dir(&dir, "french");
        let tagged = Tagger::tag(&plain, &["aujourd'hui"]);
        assert!(
            tagged[0].chunks().is_empty(),
            "chunk tags are French-only (french_normalization gate)"
        );
    }

    #[test]
    fn french_all_uppercase_falls_back_to_title_case_lookup() {
        let Some(dir) = resource_dir("fr") else {
            return;
        };
        let tagger = GenericTagger::french(&dir);
        let readings = tagger.tag_word("FRANCE", 0, 6);
        assert!(
            !readings.is_empty(),
            "all-uppercase word finds title-case dictionary entry"
        );
    }

    #[test]
    fn dutch_tags_via_morfologik_dict() {
        let Some(dir) = resource_dir("nl") else {
            return;
        };
        let tagger = GenericTagger::from_resource_dir(&dir, "dutch");
        assert!(tagger.has_dictionary());
    }
}

#[cfg(test)]
mod ar_lookup_probe {
    #[test]
    fn probe() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        );
        let t = crate::GenericTagger::from_resource_dir(dir, "arabic");
        for w in [
            "\u{645}\u{644}\u{64a}\u{648}\u{646}\u{64a}\u{646}",
            "\u{648}\u{645}\u{626}\u{62a}\u{627}\u{646}",
        ] {
            let r = t.lookup(w);
            eprintln!("{w} -> {}", r.len());
            for (lemma, tag) in r {
                eprintln!("  {lemma} {tag}");
            }
        }
    }
}
