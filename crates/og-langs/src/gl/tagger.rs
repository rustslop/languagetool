use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

type Readings = Vec<(&'static str, &'static str)>;

pub(crate) fn gl_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("gl/src/main/resources/org/languagetool/resource/gl"))
}

fn adj_part_fs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:V.P..SF.|A[QO].[FC][SN].)$").unwrap())
}

fn verb_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^V.+$").unwrap())
}

fn prefixes_for_verbs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(auto|re)(...+)$").unwrap())
}

fn mente_strip_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(.+)mente$").unwrap())
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_alphabetic() || c.is_lowercase()),
        _ => false,
    }
}

fn is_not_all_lowercase(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_lowercase())
}

fn is_mixed_case(word: &str) -> bool {
    !is_all_uppercase(word) && !is_capitalized_word(word) && is_not_all_lowercase(word)
}

pub struct GalicianTagger {
    dict: Option<Arc<MorfologikDictionary>>,
    manual: HashMap<Box<str>, Readings>,
    removals: HashMap<Box<str>, Readings>,
}

impl GalicianTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        let dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "galician");
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

    pub fn shared_arc() -> Option<Arc<GalicianTagger>> {
        crate::weak_shared(|| {
            gl_resource_dir().map(|dir| Arc::new(GalicianTagger::from_resource_dir(&dir)))
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
            let line = line.split('#').next().unwrap_or("").trim();
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

    fn mente_adverb_tokens(
        &self,
        word: &str,
        lower_word: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Option<AnalyzedToken> {
        if word.ends_with("mente")
            && let Some(caps) = mente_strip_re().captures(lower_word)
        {
            let possible_adj = caps.get(1).unwrap().as_str();
            for (_, tag) in self.dict_lookup(possible_adj) {
                if adj_part_fs_re().is_match(tag) {
                    return Some(
                        AnalyzedToken::new(display, start, end)
                            .with_lemma(lower_word.to_string())
                            .with_pos_tags(vec!["RM".to_string()]),
                    );
                }
            }
        }
        None
    }

    fn prefixed_verb_tokens(
        &self,
        word: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let mut tokens = Vec::new();
        if let Some(caps) = prefixes_for_verbs_re().captures(word) {
            let prefix = caps.get(1).unwrap().as_str().to_lowercase();
            let possible_verb = caps.get(2).unwrap().as_str().to_lowercase();
            for (lemma, tag) in self.dict_lookup(&possible_verb) {
                if verb_re().is_match(tag) {
                    let new_lemma = format!("{prefix}{lemma}");
                    tokens.push(
                        AnalyzedToken::new(display, start, end)
                            .with_lemma(new_lemma)
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
        }
        tokens
    }

    fn additional_tags(
        &self,
        word: &str,
        lower_word: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        if let Some(adverb) = self.mente_adverb_tokens(word, lower_word, display, start, end) {
            return vec![adverb];
        }
        self.prefixed_verb_tokens(word, display, start, end)
    }

    fn analyzed_tokens_at(&self, original: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let mut word = original.to_string();
        if word.chars().count() > 1 {
            word = word.replace('\u{2019}', "'");
        }
        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let is_mixed = is_mixed_case(&word);

        let mut tokens: Vec<AnalyzedToken> = Vec::new();
        tokens.extend(self.readings_for(&word, original, start, end));
        if !is_lowercase && !is_mixed {
            tokens.extend(self.readings_for(&lower_word, original, start, end));
        }
        if tokens.is_empty() && !is_mixed {
            tokens.extend(self.additional_tags(&word, &lower_word, original, start, end));
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

impl Tagger for GalicianTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for token_text in tokens {
            let start = byte_offset;
            let end = start + token_text.len();
            let readings = self.analyzed_tokens_at(token_text, start, end);
            let mut atr = build_atr(token_text, readings, start, end);
            if typewriter_apostrophe(token_text) {
                atr.set_chunks(vec!["containsTypewriterApostrophe".to_string()]);
            }
            results.push(atr);
            byte_offset = end;
        }
        results
    }
}

impl CoreTagger for GalicianTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let readings = self.analyzed_tokens_at(&surface, start, end);
            let mut next = build_atr(&surface, readings, start, end);
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

    fn tagger() -> Arc<GalicianTagger> {
        GalicianTagger::shared_arc().expect("gl resource dir with galician.dict")
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

    #[test]
    fn java_pins_todo_vai_mudar() {
        assert_eq!(
            readings_str("Todo"),
            "Todo/[todo]DI0MS0|Todo/[todo]PI0MS000"
        );
        assert_eq!(readings_str("vai"), "vai/[ir]VMIP3S0|vai/[ir]VMM02S0");
        assert_eq!(
            readings_str("mudar"),
            "mudar/[mudar]VMN0000|mudar/[mudar]VMN01S0|mudar/[mudar]VMN03S0|mudar/[mudar]VMSF1S0|mudar/[mudar]VMSF3S0"
        );
    }

    #[test]
    fn java_pins_the_long_tagger_test_sentence() {
        assert_eq!(
            readings_str("Se"),
            "Se/[se]CS|Se/[se]PP3PN000|Se/[se]PP3SN000"
        );
        assert_eq!(readings_str("aínda"), "aínda/[aínda]CS|aínda/[aínda]RG");
        assert_eq!(readings_str("somos"), "somos/[ser]VSIP1P0");
        assert_eq!(
            readings_str("galegos"),
            "galegos/[galego]AQ0MP0|galegos/[galego]NCMP000"
        );
        assert_eq!(readings_str("é"), "é/[ser]VSIP3S0");
        assert_eq!(readings_str("por"), "por/[por]SPS00");
        assert_eq!(
            readings_str("obra"),
            "obra/[obra]NCFS000|obra/[obrar]VMIP3S0|obra/[obrar]VMM02S0"
        );
        assert_eq!(readings_str("e"), "e/[e]CC|e/[e]NCMS000");
        assert_eq!(readings_str("graza"), "graza/[graza]NCFS000");
        assert_eq!(readings_str("do"), "do/[de]SPS00:DA");
        assert_eq!(readings_str("idioma"), "idioma/[idioma]NCMS000");
    }

    #[test]
    fn untagged_words_stay_untagged() {
        let tagged = Tagger::tag(tagger().as_ref(), &["xoxopeta"]);
        assert!(
            tagged[0].readings().is_empty(),
            "unknown words carry no readings (Java adds AnalyzedToken(word, null, null))"
        );
    }

    #[test]
    fn typewriter_apostrophe_normalizes_and_sets_the_chunk() {
        let tagged = Tagger::tag(tagger().as_ref(), &["d’auga", "d'auga"]);
        assert!(
            !tagged[0]
                .chunks()
                .iter()
                .any(|c| c == "containsTypewriterApostrophe"),
            "typographic-only words set no chunk"
        );
        assert!(
            tagged[1]
                .chunks()
                .iter()
                .any(|c| c == "containsTypewriterApostrophe"),
            "the typewriter apostrophe sets the chunk tag (GalicianTagger.java:97-101)"
        );
        let typographic_tags: Vec<Vec<String>> = tagged[0]
            .readings()
            .iter()
            .map(|r| r.pos_tags().to_vec())
            .collect();
        let typewriter_tags: Vec<Vec<String>> = tagged[1]
            .readings()
            .iter()
            .map(|r| r.pos_tags().to_vec())
            .collect();
        assert_eq!(
            typographic_tags, typewriter_tags,
            "’ normalizes to ' before lookup"
        );
    }

    #[test]
    fn mente_adverbs_tag_rm_when_the_stem_is_adjective_or_participle() {
        assert_eq!(
            readings_str("doentemente"),
            "doentemente/[doentemente]RM",
            "'doentemente' is absent from galician.dict but its stem 'doente' carries \
             A[QO].FS. so additionalTags assigns RM (GalicianTagger.java:114-129)"
        );
        assert_eq!(
            readings_str("duramente"),
            "duramente/[duramente]RG",
            "'duramente' sits in galician.dict directly, so the dict RG reading wins and \
             additionalTags never runs"
        );
        let tagged = Tagger::tag(tagger().as_ref(), &["xoxamente"]);
        assert!(
            tagged[0].readings().is_empty(),
            "a stem missing from the dict leaves the adverb untagged"
        );
    }

    #[test]
    fn auto_re_prefixes_tag_verbs_with_the_composed_lemma() {
        let tagger = tagger();
        assert!(
            tagger.dict_lookup("autopoñer").is_empty(),
            "'autopoñer' must be absent from galician.dict so the prefix path is exercised"
        );
        assert_eq!(
            readings_str("autopoñer"),
            "autopoñer/[autopoñer]VMN0000|autopoñer/[autopoñer]VMN01S0|autopoñer/[autopoñer]VMN03S0",
            "the auto-prefixed verb copies the dict tags of 'poñer' with the composed lemma \
             (GalicianTagger.java:130-152)"
        );
        assert!(
            readings_str("repoñer").starts_with("repoñer/[repoñer]VM"),
            "'repoñer' sits in galician.dict directly and keeps its dict readings"
        );
        let tagged = Tagger::tag(tagger.as_ref(), &["refixear"]);
        assert!(
            tagged[0].readings().is_empty(),
            "a stem missing from the dict leaves the prefixed verb untagged"
        );
    }

    #[test]
    fn mixed_case_words_get_no_lowercase_or_additional_tags() {
        let tagged = Tagger::tag(tagger().as_ref(), &["GaLeGo"]);
        assert!(
            tagged[0].readings().is_empty(),
            "mixed case skips both the lowercase lookup and additionalTags (GalicianTagger.java:81-91)"
        );
    }

    #[test]
    fn dictionary_loads() {
        assert!(tagger().has_dictionary());
    }
}
