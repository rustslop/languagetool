use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

type Readings = Vec<(&'static str, &'static str)>;
type ManualMap = HashMap<Box<str>, Readings>;

const NEGATION_PREFIX: &str = "nie";
const MIN_NEGATED_LENGTH: usize = NEGATION_PREFIX.len() + 4;

pub(crate) fn pl_resource_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("pl/src/main/resources/org/languagetool/resource/pl"))
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

fn compar_superl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\A(?:adv|adj:.+):(?:com|sup)\z").expect("valid anchored tag regex")
    })
}

pub(crate) struct WordLookup {
    dict: Option<Arc<MorfologikDictionary>>,
    manual: ManualMap,
    removals: ManualMap,
}

fn load_manual_map(resource_dir: &Path, files: [&str; 2]) -> ManualMap {
    let mut map: ManualMap = HashMap::new();
    for file in files {
        let Ok(data) = std::fs::read_to_string(resource_dir.join(file)) else {
            continue;
        };
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
            map.entry(form.to_string().into_boxed_str())
                .or_default()
                .push((
                    og_tagger::morfologik::intern_string(lemma.to_string()),
                    og_tagger::morfologik::intern_string(tag.trim().to_string()),
                ));
        }
    }
    map
}

impl WordLookup {
    fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            dict: og_tagger::generic_tagger::shared_dictionary(resource_dir, "polish"),
            manual: load_manual_map(resource_dir, ["added.txt", "added_custom.txt"]),
            removals: load_manual_map(resource_dir, ["removed.txt", "removed_custom.txt"]),
        }
    }

    fn lookup(&self, word: &str) -> Readings {
        let mut readings: Readings = self
            .manual
            .get(word)
            .map(|v| v.to_vec())
            .unwrap_or_default();
        if let Some(dictionary) = &self.dict {
            readings.extend(dictionary.lookup(word).iter().copied());
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
}

fn push_split_readings(
    raw: &Readings,
    display: &str,
    start: usize,
    end: usize,
    out: &mut Vec<AnalyzedToken>,
) {
    for (lemma, tag) in raw {
        for curr_tag in tag.split('+') {
            out.push(
                AnalyzedToken::new(display, start, end)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![curr_tag.to_string()]),
            );
        }
    }
}

pub struct PolishTagger {
    lookup_tables: Arc<WordLookup>,
    negation_exceptions: HashSet<String>,
}

impl PolishTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        let negation_exceptions =
            std::fs::read_to_string(resource_dir.join("nie_compar_exceptions.txt"))
                .map(|data| {
                    data.lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(|line| line.to_lowercase())
                        .collect::<HashSet<String>>()
                })
                .unwrap_or_default();
        Self {
            lookup_tables: Arc::new(WordLookup::from_resource_dir(resource_dir)),
            negation_exceptions,
        }
    }

    pub fn shared_arc() -> Option<Arc<PolishTagger>> {
        crate::weak_shared(|| {
            let dir = pl_resource_dir()?;
            Some(Arc::new(PolishTagger::from_resource_dir(&dir)))
        })
    }

    pub fn has_dictionary(&self) -> bool {
        self.lookup_tables.dict.is_some()
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        self.lookup_tables.is_word_tagged(word)
    }

    fn raw_readings(&self, word: &str) -> Readings {
        self.lookup_tables.lookup(word)
    }

    fn additional_tags(&self, word: &str) -> Readings {
        let lower_word = word.to_lowercase();
        if lower_word.chars().count() < MIN_NEGATED_LENGTH
            || !lower_word.starts_with(NEGATION_PREFIX)
            || is_mixed_case(word)
            || self.negation_exceptions.contains(&lower_word)
        {
            return Vec::new();
        }
        let base = &lower_word[NEGATION_PREFIX.len()..];
        let mut result: Readings = Vec::new();
        for (lemma, tag) in self.raw_readings(base) {
            if compar_superl_re().is_match(tag) {
                result.push((
                    og_tagger::morfologik::intern_string(format!("{NEGATION_PREFIX}{lemma}")),
                    tag,
                ));
            }
        }
        result
    }

    pub(crate) fn analyzed_tokens_at(
        &self,
        word: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let mut tokens: Vec<AnalyzedToken> = Vec::new();
        push_split_readings(&self.raw_readings(word), word, start, end, &mut tokens);
        if !is_lowercase {
            push_split_readings(
                &self.raw_readings(&lower_word),
                word,
                start,
                end,
                &mut tokens,
            );
        }
        if tokens.is_empty() && is_lowercase {
            let upper = uppercase_first_char(word);
            push_split_readings(&self.raw_readings(&upper), word, start, end, &mut tokens);
        }
        if tokens.is_empty() {
            let additional = self.additional_tags(word);
            push_split_readings(&additional, word, start, end, &mut tokens);
        }
        tokens
    }
}

impl Tagger for PolishTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        tokens
            .iter()
            .map(|word| {
                let readings = self.analyzed_tokens_at(word, 0, word.len());
                AnalyzedTokenReadings::new(AnalyzedToken::new(*word, 0, word.len()))
                    .with_readings(readings)
            })
            .collect()
    }
}

impl CoreTagger for PolishTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let readings = self.analyzed_tokens_at(&surface, start, end);
            *atr = AnalyzedTokenReadings::new(AnalyzedToken::new(&surface, start, end))
                .with_readings(readings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> Arc<PolishTagger> {
        PolishTagger::shared_arc().expect("pl resource dir with polish.dict")
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
    fn java_pins_plus_separated_readings_are_split_per_segment() {
        let joined = readings_str("były");
        assert!(joined.contains("były/[były]adj:sg:acc:m3:pos"), "{joined}");
        assert!(
            joined.contains("były/[były]adj:sg:nom.voc:m1.m2.m3:pos"),
            "{joined}"
        );
        assert!(joined.contains("były/[były]subst:sg:nom:m1"), "{joined}");
        assert!(
            joined.contains("były/[być]verb:praet:pl:m2.m3.f.n1.n2.p2.p3:ter:imperf:nonrefl"),
            "{joined}"
        );
        assert!(
            !joined.contains('+'),
            "fused tags must never survive the PolishTagger: {joined}"
        );
    }

    #[test]
    fn java_pins_plain_word_lemma_and_uppercase_surface_fallback() {
        assert_eq!(readings_str("kobieta"), "kobieta/[kobieta]subst:sg:nom:f");
        let byl = readings_str("Były");
        assert!(
            byl.contains("Były/[być]verb:praet"),
            "uppercase surface keeps the dict lemma surface: {byl}"
        );
        assert!(
            byl.starts_with("Były/"),
            "surface stays the original token: {byl}"
        );
    }

    #[test]
    fn java_pins_joined_nie_with_comparative_and_superlative() {
        let readings = readings_str("nielepiej");
        assert!(
            readings.contains("nielepiej/[niedobrze]adv:com"),
            "nielepiej gets the negated comparative of dobrze: {readings}"
        );
        let readings = readings_str("nienajlepiej");
        assert!(
            readings.contains("nienajlepiej/[niedobrze]adv:sup"),
            "nienajlepiej gets the negated superlative of dobrze: {readings}"
        );
    }

    #[test]
    fn java_pins_nie_joined_forms_that_stay_untagged() {
        assert!(
            readings_str("niepięknie").starts_with("niepięknie/[niepięknie]adv:"),
            "dict-contained negated adverb is tagged directly from polish.dict"
        );
        assert_eq!(
            readings_str("nienielepiej"),
            "nienielepiej/[null]",
            "double-nie stays untagged: only one prefix lookup"
        );
        assert!(
            readings_str("niemdlej").starts_with("niemdlej/[null]"),
            "niemdlej is in the imperative-homograph exception list"
        );
    }

    #[test]
    fn java_pins_manual_additions_before_dictionary_readings() {
        let readings = readings_str("wieczora");
        assert!(
            readings.contains("wieczora/[wieczór]subst:sg:gen:m3"),
            "added.txt entry is tagged: {readings}"
        );
    }
}
