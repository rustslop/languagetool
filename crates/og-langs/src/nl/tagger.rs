use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

type Readings = Vec<(&'static str, &'static str)>;

pub(crate) fn nl_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("nl/src/main/resources/org/languagetool/resource/nl"))
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_alphabetic() || c.is_lowercase()),
        _ => false,
    }
}

fn is_not_all_lowercase(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && c.is_uppercase())
}

fn is_mixed_case(word: &str) -> bool {
    !is_all_uppercase(word) && !is_capitalized_word(word) && is_not_all_lowercase(word)
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

pub(crate) struct WordLookup {
    dict: Option<Arc<MorfologikDictionary>>,
    manual: HashMap<Box<str>, Readings>,
    removals: HashMap<Box<str>, Readings>,
}

impl WordLookup {
    fn from_resource_dir(resource_dir: &Path) -> Self {
        let dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "dutch");
        let read = |name: &str| std::fs::read_to_string(resource_dir.join(name)).ok();
        let added = ["added.txt", "added_custom.txt"]
            .iter()
            .filter_map(|f| read(f))
            .collect::<Vec<_>>()
            .join("\n");
        let removed = ["removed.txt", "removed_custom.txt"]
            .iter()
            .filter_map(|f| read(f))
            .collect::<Vec<_>>()
            .join("\n");
        let mut lookup = Self {
            dict,
            manual: HashMap::new(),
            removals: HashMap::new(),
        };
        lookup.load_manual_data(&added, false);
        lookup.load_manual_data(&removed, true);
        lookup
    }

    pub(crate) fn shared() -> Arc<WordLookup> {
        static CACHE: og_core::releasable::Releasable<WordLookup> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CACHE, || {
            let dir = nl_resource_dir().expect("nl resource dir with dutch.dict");
            Some(Arc::new(WordLookup::from_resource_dir(&dir)))
        })
        .expect("nl word lookup")
    }

    fn load_manual_data(&mut self, data: &str, into_removals: bool) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(form), Some(lemma), Some(tag)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let form_leaked: &'static str = og_tagger::morfologik::intern_string(form.to_string());
            let lemma_leaked: &'static str =
                og_tagger::morfologik::intern_string(lemma.to_string());
            let tag_leaked: &'static str =
                og_tagger::morfologik::intern_string(tag.trim().to_string());
            let target = if into_removals {
                &mut self.removals
            } else {
                &mut self.manual
            };
            target
                .entry(form_leaked.into())
                .or_default()
                .push((lemma_leaked, tag_leaked));
        }
    }

    pub(crate) fn lookup(&self, word: &str) -> Readings {
        let mut readings: Readings = self
            .manual
            .get(word)
            .map(|v| v.to_vec())
            .unwrap_or_default();
        if let Some(dict) = &self.dict {
            readings.extend(dict.lookup(word).iter().copied());
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

    fn is_word_tagged(&self, word: &str) -> bool {
        !self.lookup(word).is_empty()
    }

    fn readings_for(
        &self,
        word: &str,
        display: &str,
        start: usize,
        end: usize,
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
}

fn pattern1_replacements() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            [('á', 'a'), ('é', 'e'), ('í', 'i'), ('ó', 'o'), ('ú', 'u')]
                .into_iter()
                .map(|(accented, base)| {
                    let pattern = og_tagger::morfologik::intern_string(format!(
                        r"([^aeiouáéíóú])({accented})([^aeiouáéíóú])"
                    ));
                    let replacement: &'static str =
                        og_tagger::morfologik::intern_string(format!("${{1}}{base}${{3}}"));
                    (Regex::new(pattern).unwrap(), replacement)
                })
                .collect()
        })
        .as_slice()
}

fn char_pattern_replacements() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            [
                ("áá", "aa"),
                ("áé", "ae"),
                ("áí", "ai"),
                ("áú", "au"),
                ("éé", "ee"),
                ("éí", "ei"),
                ("éú", "eu"),
                ("íé", "ie"),
                ("óé", "oe"),
                ("óí", "oi"),
                ("óó", "oo"),
                ("óú", "ou"),
                ("úí", "ui"),
                ("úú", "uu"),
                ("íj", "ij"),
            ]
            .into_iter()
            .map(|(from, to)| {
                let replacement: &'static str =
                    og_tagger::morfologik::intern_string(to.to_string());
                (Regex::new(from).unwrap(), replacement)
            })
            .collect()
        })
        .as_slice()
}

fn pattern2_replacements() -> &'static [(Regex, &'static str)] {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            [('á', 'a'), ('é', 'e'), ('í', 'i'), ('ó', 'o'), ('ú', 'u')]
                .into_iter()
                .map(|(accented, base)| {
                    let pattern = og_tagger::morfologik::intern_string(format!(
                        r"(^|[^aeiou]){accented}([^aeiou]|$)"
                    ));
                    let replacement: &'static str =
                        og_tagger::morfologik::intern_string(format!("${{1}}{base}${{2}}"));
                    (Regex::new(pattern).unwrap(), replacement)
                })
                .collect()
        })
        .as_slice()
}

fn hyphen2_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"([a-z])-([a-z])").unwrap())
}

fn after_last_hyphen(word: &str) -> &str {
    match word.rfind('-') {
        Some(idx) => &word[idx + 1..],
        None => word,
    }
}

fn flatten_accents(word: &str) -> String {
    let mut out = word.to_string();
    for (re, replacement) in pattern1_replacements() {
        out = re.replace_all(&out, *replacement).into_owned();
    }
    for (re, replacement) in char_pattern_replacements() {
        out = out.replace(re.as_str(), replacement);
    }
    for (re, replacement) in pattern2_replacements() {
        out = re.replace_all(&out, *replacement).into_owned();
    }
    out
}

pub struct DutchTagger {
    lookup_tables: Arc<WordLookup>,
    acceptor: Option<Arc<super::compound_acceptor::CompoundAcceptor>>,
}

impl DutchTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            lookup_tables: Arc::new(WordLookup::from_resource_dir(resource_dir)),
            acceptor: None,
        }
    }

    pub fn shared_arc() -> Option<Arc<DutchTagger>> {
        crate::weak_shared(|| {
            let dir = nl_resource_dir()?;
            let mut tagger = DutchTagger::from_resource_dir(&dir);
            tagger.acceptor = Some(super::compound_acceptor::shared_compound_acceptor());
            Some(Arc::new(tagger))
        })
    }

    pub fn has_dictionary(&self) -> bool {
        self.lookup_tables.dict.is_some()
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        self.lookup_tables.is_word_tagged(word)
    }

    fn readings_for(
        &self,
        word: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        self.lookup_tables.readings_for(word, display, start, end)
    }

    pub fn get_postags(&self, word: &str) -> Readings {
        self.lookup_tables.lookup(word)
    }

    fn analyzed_tokens_at(
        &self,
        original: &str,
        start: usize,
        end: usize,
    ) -> (Vec<AnalyzedToken>, bool) {
        let word = original.replace(['`', '\u{2019}', '\u{2018}', '\u{00B4}'], "'");
        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let is_mixed = is_mixed_case(&word);
        let is_all_upper = is_all_uppercase(&word);

        let mut tokens: Vec<AnalyzedToken> = Vec::new();
        tokens.extend(self.readings_for(&word, original, start, end));
        if !is_lowercase && !is_mixed {
            tokens.extend(self.readings_for(&lower_word, original, start, end));
        }
        if tokens.is_empty() && is_all_upper {
            let first_upper = uppercase_first_char(&lower_word);
            tokens.extend(self.readings_for(&first_upper, original, start, end));
        }

        let mut ignore_spelling = false;
        if tokens.is_empty() {
            let mut word2 = flatten_accents(&word);
            if word2.contains('-') {
                let part2 = after_last_hyphen(&word2).to_string();
                let p2 = self.readings_for(&part2, original, start, end);
                if !p2.is_empty() {
                    word2 = hyphen2_re().replace_all(&word2, "${1}${2}").into_owned();
                }
            }
            if word2 != word {
                let l2 = self.readings_for(&word2, original, start, end);
                if !l2.is_empty() {
                    tokens.extend(l2);
                    ignore_spelling = true;
                }
            }
        }

        if tokens.is_empty() && word.chars().count() > 5 {
            let parts = self
                .acceptor
                .as_deref()
                .and_then(|acceptor| acceptor.get_parts(&word));
            if let Some((part1, part2)) = parts {
                let part1lc = part1.to_lowercase();
                let part2_readings = self.analyzed_tokens_at(&part2, 0, 0).0;
                for reading in &part2_readings {
                    let Some(tag) = reading.pos_tags().first() else {
                        continue;
                    };
                    if part1.ends_with('-') && tag.starts_with("ENM:LOC") {
                        tokens.push(
                            AnalyzedToken::new(original, start, end)
                                .with_lemma(part2.clone())
                                .with_pos_tags(vec![tag.clone()]),
                        );
                        break;
                    }
                    if tag.starts_with("ZNW") {
                        let tag = if super::compound_acceptor::always_needs_het(&part2) {
                            "ZNW:EKV:HET".to_string()
                        } else if super::compound_acceptor::always_needs_de(&part2) {
                            "ZNW:EKV:DE_".to_string()
                        } else if super::compound_acceptor::always_needs_mrv(&part2) {
                            "ZNW:MRV:DE_".to_string()
                        } else {
                            tag.clone()
                        };
                        let lemma = format!("{}{}", part1lc, reading.lemma().unwrap_or_default());
                        tokens.push(
                            AnalyzedToken::new(original, start, end)
                                .with_lemma(lemma)
                                .with_pos_tags(vec![tag]),
                        );
                        if super::compound_acceptor::compound_part2_is_pinned(&part2) {
                            break;
                        }
                    }
                }
            }
        }

        if tokens.is_empty() {
            tokens.push(AnalyzedToken::new(original, start, end));
            return (tokens, false);
        }

        let ignore_flag = if ignore_spelling {
            if is_lowercase {
                let first_upper = uppercase_first_char(original);
                self.readings_for(&first_upper, original, start, end)
                    .is_empty()
            } else {
                true
            }
        } else {
            false
        };
        (tokens, ignore_flag)
    }
}

impl Tagger for DutchTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        tokens
            .iter()
            .map(|word| {
                let (readings, ignore_spelling) = self.analyzed_tokens_at(word, 0, word.len());
                let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(*word, 0, word.len()))
                    .with_readings(readings);
                if ignore_spelling {
                    atr.ignore_spelling();
                }
                atr
            })
            .collect()
    }
}

impl CoreTagger for DutchTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let (readings, ignore_spelling) = self.analyzed_tokens_at(&surface, start, end);
            let mut next = AnalyzedTokenReadings::new(AnalyzedToken::new(&surface, start, end))
                .with_readings(readings);
            if ignore_spelling {
                next.ignore_spelling();
            }
            *atr = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> Arc<DutchTagger> {
        DutchTagger::shared_arc().expect("nl resource dir with dutch.dict")
    }

    fn readings_str(text: &str) -> String {
        let tagged = Tagger::tag(tagger().as_ref(), &[text]);
        let mut readings: Vec<String> = tagged[0]
            .readings()
            .iter()
            .map(|r| {
                format!(
                    "{}/[{}]{}",
                    tagged[0].token().token(),
                    r.lemma().unwrap_or("null"),
                    r.pos_tags().join("|")
                )
            })
            .collect();
        if readings.is_empty() {
            readings.push(format!("{}/[null]", tagged[0].token().token()));
        }
        readings.sort_unstable();
        readings.join("|")
    }

    #[test]
    fn java_pins_plain_words() {
        assert_eq!(
            readings_str("Aardappels"),
            "Aardappels/[aardappel]ZNW:MRV:DE_"
        );
        assert_eq!(readings_str("koken"), "koken/[koken]WKW:TGW:INF");
        assert_eq!(readings_str("zwijnden"), "zwijnden/[zwijnen]WKW:VLT:INF");
        assert_eq!(
            readings_str("havermout"),
            "havermout/[havermout]ZNW:EKV:DE_"
        );
    }

    #[test]
    fn java_pins_compound_words() {
        assert_eq!(
            readings_str("oorlogsbeker"),
            "oorlogsbeker/[oorlogsbeker]ZNW:EKV:DE_"
        );
        assert_eq!(
            readings_str("dorpswezen"),
            "dorpswezen/[dorpswees]ZNW:MRV:DE_|dorpswezen/[dorpswezen]ZNW:EKV:HET"
        );
        assert_eq!(
            readings_str("varkensweer"),
            "varkensweer/[varkensweer]ZNW:EKV:HET"
        );
        assert_eq!(
            readings_str("jongenstablet"),
            "jongenstablet/[jongenstablet]ZNW:EKV:DE_|jongenstablet/[jongenstablet]ZNW:EKV:HET"
        );
        assert_eq!(
            readings_str("passagierssituaties"),
            "passagierssituaties/[passagierssituatie]ZNW:MRV:DE_"
        );
    }

    #[test]
    fn java_pins_rejected_compounds_stay_untagged() {
        assert_eq!(
            readings_str("beoordelinggeschiedenis"),
            "beoordelinggeschiedenis/[null]"
        );
        assert_eq!(
            readings_str("gastkritiekgeschiedenis"),
            "gastkritiekgeschiedenis/[null]"
        );
        assert_eq!(
            readings_str("haarhalfbergnacht"),
            "haarhalfbergnacht/[null]"
        );
        assert_eq!(readings_str("vriendenvrienden"), "vriendenvrienden/[null]");
    }

    #[test]
    fn java_pins_accepted_compound_variants() {
        assert_eq!(
            readings_str("beoordelingsgeschiedenis"),
            "beoordelingsgeschiedenis/[beoordelingsgeschiedenis]ZNW:EKV:DE_"
        );
        assert_eq!(
            readings_str("beoordelingsgeschiedenisje"),
            "beoordelingsgeschiedenisje/[beoordelingsgeschiedenis]ZNW:EKV:VRK:HET"
        );
        assert_eq!(
            readings_str("Beoordelingsgeschiedenisjes"),
            "Beoordelingsgeschiedenisjes/[beoordelingsgeschiedenis]ZNW:MRV:VRK:DE_"
        );
    }

    #[test]
    fn java_pins_geographical_and_hyphen_compounds() {
        assert_eq!(
            readings_str("Zuidoost-Gouda"),
            "Zuidoost-Gouda/[Gouda]ENM:LOC:PTS"
        );
        assert_eq!(
            readings_str("West-Bergambacht"),
            "West-Bergambacht/[Bergambacht]ENM:LOC:PTS"
        );
        assert_eq!(
            readings_str("beroertegeschiedenisje"),
            "beroertegeschiedenisje/[beroertegeschiedenis]ZNW:EKV:VRK:HET"
        );
        assert_eq!(
            readings_str("aspirant-burgemeestertje"),
            "aspirant-burgemeestertje/[aspirant-burgemeester]ZNW:EKV:VRK:HET"
        );
    }

    #[test]
    fn java_pins_accent_flattening() {
        assert_eq!(readings_str("déúr"), "déúr/[deur]ZNW:EKV:DE_");
        assert_eq!(readings_str("kómen"), "kómen/[komen]WKW:TGW:INF");
        assert_eq!(
            readings_str("kán"),
            "kán/[kan]ZNW:EKV:DE_|kán/[kunnen]WKW:TGW:1EP|kán/[kunnen]WKW:TGW:3EP"
        );
        assert_eq!(readings_str("ín"), "ín/[in]VRZ|ín/[innen]WKW:TGW:1EP");
        assert_eq!(readings_str("deur-knop"), "deur-knop/[deurknop]ZNW:EKV:DE_");
    }

    #[test]
    fn flattened_and_hyphen_words_are_ignored_by_the_speller() {
        let tagged = Tagger::tag(tagger().as_ref(), &["déúr", "kómen", "deur-knop", "deur"]);
        assert!(tagged[0].is_ignored_by_speller());
        assert!(tagged[1].is_ignored_by_speller());
        assert!(tagged[2].is_ignored_by_speller());
        assert!(!tagged[3].is_ignored_by_speller());
    }
}
