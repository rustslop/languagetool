use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

type Readings = Vec<(&'static str, &'static str)>;

pub(crate) fn pt_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("pt/src/main/resources/org/languagetool/resource/pt"))
}

fn adj_part_fs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:V.P..SF.|A[QO].[FC][SN].)$").unwrap())
}

fn verb_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^V.+$").unwrap())
}

fn ordinal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d+[\d,.]*\.?[oºᵒaªᵃ][sˢ]?$").unwrap())
}

fn ordinal_masc_sg_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[oºᵒ]$").unwrap())
}

fn ordinal_fem_sg_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[aªᵃ]$").unwrap())
}

fn ordinal_masc_pl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[oºᵒ][sˢ]$").unwrap())
}

fn ordinal_fem_pl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[aªᵃ][sˢ]$").unwrap())
}

fn ordinal_suffix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[oºᵒaªᵃ][sˢ]?").unwrap())
}

fn percent_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\u{2212}?\d+[\d,.]*%$").unwrap())
}

fn degree_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\u{2212}?\d+[\d,.]*°$").unwrap())
}

fn prefixes_for_verbs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(soto-)(...+)$").unwrap())
}

fn mente_strip_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.+)mente$").unwrap())
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_uppercase()),
        _ => false,
    }
}

fn is_mixed_case(word: &str) -> bool {
    !is_all_uppercase(word) && !is_capitalized_word(word) && word.chars().any(char::is_uppercase)
}

fn is_mixed_case_hyphen_aware(word: &str) -> bool {
    word.split('-').any(is_mixed_case)
}

pub struct PortugueseTagger {
    dict: Option<Arc<MorfologikDictionary>>,
    manual: HashMap<Box<str>, Readings>,
    removals: HashMap<Box<str>, Readings>,
}

impl PortugueseTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        let dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "portuguese");
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
        let mut tagger = Self {
            dict,
            manual: HashMap::new(),
            removals: HashMap::new(),
        };
        tagger.load_manual_data(&added, false);
        tagger.load_manual_data(&removed, true);
        tagger
    }

    pub fn shared_arc() -> Option<Arc<PortugueseTagger>> {
        crate::weak_shared(|| {
            pt_resource_dir().map(|dir| Arc::new(PortugueseTagger::from_resource_dir(&dir)))
        })
    }

    pub fn has_dictionary(&self) -> bool {
        self.dict.is_some()
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

    pub fn lookup(&self, word: &str) -> Readings {
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

    fn dict_lookup(&self, word: &str) -> Readings {
        self.dict
            .as_ref()
            .map(|d| d.lookup(word).to_vec())
            .unwrap_or_default()
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        !self.analyzed_tokens(word).is_empty()
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

    fn analyzed_tokens(&self, original: &str) -> Vec<AnalyzedToken> {
        self.analyzed_tokens_at(original, 0, original.len()).0
    }

    fn analyzed_tokens_at(
        &self,
        original: &str,
        start: usize,
        end: usize,
    ) -> (Vec<AnalyzedToken>, bool) {
        let mut word = original.to_string();
        if word.chars().count() > 1 {
            word = word.replace('\u{2019}', "'");
        }
        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let is_mixed = is_mixed_case_hyphen_aware(&word);

        let mut tokens: Vec<AnalyzedToken> = Vec::new();
        tokens.extend(self.readings_for(&word, &word, start, end));
        if !is_lowercase && !is_mixed {
            tokens.extend(self.readings_for(&lower_word, &word, start, end));
        }
        if tokens.is_empty() {
            tokens.extend(self.number_expression_tokens(&word, start, end));
        }
        if tokens.is_empty() && !is_mixed {
            tokens.extend(self.mente_adverb_tokens(&word, &lower_word, start, end));
        }
        let mut ignore_spelling = false;
        if tokens.is_empty() && !is_mixed {
            tokens.extend(self.prefixed_verb_tokens(&word, start, end));
            ignore_spelling = !tokens.is_empty();
        }
        (tokens, ignore_spelling)
    }

    fn number_expression_tokens(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        if ordinal_re().is_match(word) {
            return self.ordinal_tokens(word, start, end);
        }
        if degree_re().is_match(word) || percent_re().is_match(word) {
            return vec![
                AnalyzedToken::new(word, start, end)
                    .with_lemma(word.to_string())
                    .with_pos_tags(vec!["NCMP000".to_string()]),
            ];
        }
        Vec::new()
    }

    fn ordinal_tokens(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let lemma = ordinal_suffix_re().replace_all(word, "º").into_owned();
        let mut number_gender_tags = "";
        if ordinal_masc_sg_re().is_match(word) {
            number_gender_tags = "MS";
        }
        if ordinal_fem_sg_re().is_match(word) {
            number_gender_tags = "FS";
        }
        if ordinal_masc_pl_re().is_match(word) {
            number_gender_tags = "MP";
        }
        if ordinal_fem_pl_re().is_match(word) {
            number_gender_tags = "FP";
        }
        let noun_tag = format!("NC{number_gender_tags}000");
        let adj_tag = format!("AO0{number_gender_tags}0");
        vec![
            AnalyzedToken::new(word, start, end)
                .with_lemma(lemma.clone())
                .with_pos_tags(vec![noun_tag]),
            AnalyzedToken::new(word, start, end)
                .with_lemma(lemma)
                .with_pos_tags(vec![adj_tag]),
        ]
    }

    fn mente_adverb_tokens(
        &self,
        word: &str,
        lower_word: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        if word.ends_with("mente")
            && let Some(caps) = mente_strip_re().captures(lower_word)
        {
            let possible_adj = caps.get(1).unwrap().as_str();
            for (_, tag) in self.dict_lookup(possible_adj) {
                if adj_part_fs_re().is_match(tag) {
                    return vec![
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(lower_word.to_string())
                            .with_pos_tags(vec!["RG".to_string()]),
                    ];
                }
            }
        }
        Vec::new()
    }

    fn prefixed_verb_tokens(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let mut tokens = Vec::new();
        if let Some(caps) = prefixes_for_verbs_re().captures(word) {
            let prefix = caps.get(1).unwrap().as_str().to_lowercase();
            let possible_verb = caps.get(2).unwrap().as_str().to_lowercase();
            for (lemma, tag) in self.dict_lookup(&possible_verb) {
                if verb_re().is_match(tag) {
                    let new_lemma = format!("{prefix}{lemma}");
                    if self.dict_lookup(&new_lemma).is_empty() {
                        tokens.push(
                            AnalyzedToken::new(word, start, end)
                                .with_lemma(new_lemma)
                                .with_pos_tags(vec![tag.to_string()]),
                        );
                    }
                }
            }
        }
        tokens
    }
}

fn typewriter_apostrophe(original: &str) -> bool {
    original.chars().count() > 1 && original.contains('\'')
}

fn build_atr(
    surface: &str,
    readings: Vec<AnalyzedToken>,
    start: usize,
    end: usize,
) -> AnalyzedTokenReadings {
    let all_tags: Vec<String> = readings
        .iter()
        .flat_map(|r| r.pos_tags().to_vec())
        .collect();
    let lemma = readings
        .first()
        .and_then(|r| r.lemma().map(str::to_string))
        .unwrap_or_else(|| surface.to_string());
    let primary = AnalyzedToken::new(surface, start, end)
        .with_pos_tags(all_tags)
        .with_lemma(lemma);
    AnalyzedTokenReadings::new(primary).with_readings(readings)
}

impl Tagger for PortugueseTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for token_text in tokens {
            let start = byte_offset;
            let end = start + token_text.len();
            let (readings, ignore_spelling) = self.analyzed_tokens_at(token_text, start, end);
            let mut atr = build_atr(token_text, readings, start, end);
            if ignore_spelling {
                atr.ignore_spelling();
            }
            if typewriter_apostrophe(token_text) {
                atr.set_chunks(vec!["containsTypewriterApostrophe".to_string()]);
            }
            results.push(atr);
            byte_offset = end;
        }
        results
    }
}

impl CoreTagger for PortugueseTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let (readings, ignore_spelling) = self.analyzed_tokens_at(&surface, start, end);
            let mut next = build_atr(&surface, readings, start, end);
            if ignore_spelling {
                next.ignore_spelling();
            }
            if typewriter_apostrophe(&surface) {
                next.set_chunks(vec!["containsTypewriterApostrophe".to_string()]);
            }
            *atr = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> Arc<PortugueseTagger> {
        PortugueseTagger::shared_arc().expect("pt resource dir with portuguese.dict")
    }

    fn readings_str(text: &str) -> String {
        let tagged = Tagger::tag(tagger().as_ref(), &[text]);
        let readings: Vec<String> = tagged[0]
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
        readings.join("|")
    }

    fn sorted_readings(text: &str) -> String {
        let joined = readings_str(text);
        let mut items: Vec<&str> = joined.split('|').collect();
        items.sort_unstable();
        items.join("|")
    }

    #[test]
    fn java_pins_ordinal_abbreviations() {
        assert_eq!(
            sorted_readings("1.as"),
            "1.as/[1.º]AO0FP0|1.as/[1.º]NCFP000"
        );
        assert_eq!(sorted_readings("2as"), "2as/[2º]AO0FP0|2as/[2º]NCFP000");
        assert_eq!(
            sorted_readings("300as"),
            "300as/[300º]AO0FP0|300as/[300º]NCFP000"
        );
        assert_eq!(
            sorted_readings("4.ªˢ"),
            "4.ªˢ/[4.º]AO0FP0|4.ªˢ/[4.º]NCFP000"
        );
        assert_eq!(sorted_readings("5ªˢ"), "5ªˢ/[5º]AO0FP0|5ªˢ/[5º]NCFP000");
        assert_eq!(
            sorted_readings("600ªˢ"),
            "600ªˢ/[600º]AO0FP0|600ªˢ/[600º]NCFP000"
        );
        assert_eq!(
            sorted_readings("7.ᵃˢ"),
            "7.ᵃˢ/[7.º]AO0FP0|7.ᵃˢ/[7.º]NCFP000"
        );
        assert_eq!(sorted_readings("8ᵃˢ"), "8ᵃˢ/[8º]AO0FP0|8ᵃˢ/[8º]NCFP000");
        assert_eq!(
            sorted_readings("900ᵃˢ"),
            "900ᵃˢ/[900º]AO0FP0|900ᵃˢ/[900º]NCFP000"
        );
    }

    #[test]
    fn java_pins_percent_and_degree() {
        assert_eq!(sorted_readings("10%"), "10%/[10%]NCMP000");
        assert_eq!(sorted_readings("−11.000%"), "−11.000%/[−11.000%]NCMP000");
        assert_eq!(sorted_readings("12°"), "12°/[12°]NCMP000");
    }

    #[test]
    fn java_pins_productive_prefix() {
        assert_eq!(
            sorted_readings("soto-trepei"),
            "soto-trepei/[soto-trepar]VMIS1S0"
        );
        assert!(
            Tagger::tag(tagger().as_ref(), &["xoxotrepei"])[0]
                .readings()
                .is_empty(),
            "xoxotrepei must stay untagged (Java: [null]null)"
        );
    }

    #[test]
    fn java_pins_plain_words() {
        assert_eq!(sorted_readings("tentou"), "tentou/[tentar]VMIS3S0");
        let são = sorted_readings("são");
        assert!(são.contains("são/[são]AQ0MS0"), "{são}");
        assert!(são.contains("são/[ser]VMIP3P0"), "{são}");
        let estes = sorted_readings("Estes");
        assert!(estes.contains("Estes/[este]DD0MP0"), "{estes}");
        assert!(estes.contains("Estes/[este]PD0MP000"), "{estes}");
    }

    #[test]
    fn mente_adverbs_tag_rg_when_stem_is_participle_or_adjective() {
        let destacadamente = sorted_readings("destacadamente");
        assert_eq!(destacadamente, "destacadamente/[destacadamente]RG");
        let tagged = Tagger::tag(tagger().as_ref(), &["xoxamente"]);
        assert!(
            tagged[0].readings().is_empty(),
            "stem not in dict stays untagged"
        );
    }

    #[test]
    fn typewriter_apostrophe_normalization_and_chunk() {
        let tagged = Tagger::tag(tagger().as_ref(), &["d'agua", "d’agua"]);
        assert!(
            tagged[0]
                .readings()
                .iter()
                .all(|r| !r.pos_tags().is_empty() || r.lemma().is_none()),
            "typewriter form keeps readings"
        );
        assert!(
            tagged[0]
                .chunks()
                .iter()
                .any(|c| c == "containsTypewriterApostrophe")
        );
        assert!(
            !tagged[1]
                .chunks()
                .iter()
                .any(|c| c == "containsTypewriterApostrophe"),
            "typographic-only words set no chunk (PortugueseTagger checks before replacement)"
        );
        let normalized = tagged[1]
            .readings()
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect::<Vec<_>>();
        let direct = tagged[0]
            .readings()
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect::<Vec<_>>();
        assert_eq!(
            normalized, direct,
            "’ normalizes to ' before lookup (same readings)"
        );
    }

    #[test]
    fn single_char_word_skips_apostrophe_normalization() {
        let tagged = Tagger::tag(tagger().as_ref(), &["’"]);
        assert!(tagged[0].chunks().is_empty());
        assert!(tagged[0].readings().is_empty());
    }

    #[test]
    fn ignore_spelling_set_for_prefixed_verbs_only() {
        let tagged = Tagger::tag(tagger().as_ref(), &["soto-trepei", "tentou"]);
        assert!(tagged[0].is_ignored_by_speller());
        assert!(!tagged[1].is_ignored_by_speller());
    }

    #[test]
    fn clitic_chains_and_contractions_are_data_blocked() {
        let pin_words = ["deixe-me", "distribuí-lo-ei", "das", "ao", "jiu-jitsu"];
        let tagger = tagger();
        for word in pin_words {
            let readings = Tagger::tag(tagger.as_ref(), &[word])[0].readings().to_vec();
            let has_chain = readings
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t.contains(':')));
            if has_chain {
                panic!("{word} unexpectedly carries chain tags: {readings:?}");
            }
        }
    }
}
