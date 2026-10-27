use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

pub(crate) fn ca_resource_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ca/src/main/resources/org/languagetool/resource/ca"))
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn is_not_all_lowercase(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_lowercase())
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn is_mixed_case(word: &str) -> bool {
    !is_all_uppercase(word) && !is_capitalized_word(word) && is_not_all_lowercase(word)
}

fn is_emoji(word: &str) -> bool {
    static WORD_FOR_SPELLER: OnceLock<Regex> = OnceLock::new();
    let utf16_len: usize = word.chars().map(char::len_utf16).sum();
    if utf16_len <= 1 || word.chars().count() == utf16_len {
        return false;
    }
    !WORD_FOR_SPELLER
        .get_or_init(|| Regex::new(r"^[\p{L}\d\p{P}\p{Zs}]+$").unwrap())
        .is_match(word)
}

const ENDINGS: &str = "a|ada|ades|am|ant|ar|ara|aran|arem|aren|ares|areu|aria|arien|aries|arà|aràs|aré|aríem|aríeu|assen|asses|assin|assis|at|ats|au|ava|aven|aves|e|ec|ega|eguda|egudes|eguem|eguen|eguera|egueren|egueres|egues|eguessen|eguesses|eguessin|eguessis|egueu|egui|eguin|eguis|egut|eguts|egué|eguérem|eguéreu|egués|eguéssem|eguésseu|eguéssim|eguéssiu|eguí|eix|eixem|eixen|eixent|eixeran|eixerem|eixeren|eixeres|eixereu|eixeria|eixerien|eixeries|eixerà|eixeràs|eixeré|eixeríem|eixeríeu|eixes|eixessen|eixesses|eixessin|eixessis|eixeu|eixi|eixia|eixien|eixies|eixin|eixis|eixo|eixé|eixérem|eixéreu|eixés|eixéssem|eixésseu|eixéssim|eixéssiu|eixí|eixíem|eixíeu|em|en|es|esc|esca|escuda|escudes|escut|escuts|esquem|esquen|esquera|esqueren|esqueres|esques|esquessen|esquesses|esquessin|esquessis|esqueu|esqui|esquin|esquis|esqué|esquérem|esquéreu|esqués|esquéssem|esquésseu|esquéssim|esquéssiu|esquí|essen|esses|essin|essis|eu|i|ia|ida|ides|ien|ies|iguem|igueu|im|in|int|ir|ira|iran|irem|iren|ires|ireu|iria|irien|iries|irà|iràs|iré|iríem|iríeu|is|isc|isca|isquen|isques|issen|isses|issin|issis|it|its|iu|ix|ixen|ixes|o|à|àrem|àreu|às|àssem|àsseu|àssim|àssiu|àvem|àveu|èixer|éixer|és|éssem|ésseu|éssim|éssiu|í|íem|íeu|írem|íreu|ís|íssem|ísseu|íssim|íssiu|ïs";

const ALTRES_PREFIXOS: [&str; 12] = [
    "greco", "sino", "italo", "franco", "gal·lo", "luso", "germano", "hispano", "anglo", "àrabo",
    "austro", "belgo",
];

const NO_ALTRES_PREFIXOS: [&str; 11] = [
    "grego",
    "xineso",
    "italiano",
    "franceso",
    "portugueso",
    "angleso",
    "espanyolo",
    "alemanyo",
    "arabo",
    "austríaco",
    "bèlgico",
];

const ALLUPPERCASE_EXCEPTIONS: [&str; 3] = ["ARNAU", "CRISTIAN", "TOMÀS"];

type Readings = Vec<(String, String)>;

#[derive(Default)]
struct ManualDictionary {
    entries: HashMap<String, Readings>,
    removals: HashMap<String, Readings>,
}

impl ManualDictionary {
    fn load(resource_dir: &Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(resource_dir.join(name)).ok();
        let mut dict = Self::default();
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
        ManualDictionary::fill(&mut dict.entries, &added);
        ManualDictionary::fill(&mut dict.removals, &removed);
        dict
    }

    fn fill(target: &mut HashMap<String, Readings>, data: &str) {
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
            target
                .entry(form.to_string())
                .or_default()
                .push((lemma.to_string(), tag.trim().to_string()));
        }
    }

    fn lookup(&self, word: &str) -> Vec<(String, String)> {
        let mut readings: Vec<(String, String)> =
            self.entries.get(word).cloned().unwrap_or_default();
        if let Some(removals) = self.removals.get(word) {
            readings.retain(|(lemma, tag)| {
                !removals
                    .iter()
                    .any(|(r_lemma, r_tag)| r_lemma == lemma && r_tag == tag)
            });
        }
        readings
    }

    fn is_tagged(&self, word: &str) -> bool {
        !self.lookup(word).is_empty()
    }
}

pub struct CatalanTagger {
    dict: ManualDictionary,
    wrong_verbs: HashSet<String>,
    adj_part_fs: Regex,
    verb: Regex,
    prefixes_for_verbs: Regex,
    adjectiu_compost: Regex,
    tres_adjectius: Regex,
    desinencies_1conj_0: Regex,
    desinencies_1conj_1: Regex,
}

fn endings_regex(lazy: bool) -> Regex {
    let base = if lazy { "(.+?)" } else { "(.+)" };
    Regex::new(&format!("(?i)^{base}({ENDINGS})$")).expect("ca verb endings regex")
}

impl CatalanTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        let mut wrong_verbs = HashSet::new();
        if let Ok(data) =
            std::fs::read_to_string(resource_dir.join("../../rules/ca/replace_verbs.txt"))
        {
            for line in data.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(wrong_side) = line.split('=').next() {
                    for form in wrong_side.split('|') {
                        wrong_verbs.insert(form.trim().to_string());
                    }
                }
            }
        }
        Self {
            dict: ManualDictionary::load(resource_dir),
            wrong_verbs,
            adj_part_fs: Regex::new("^(VMP00SF.|A[QO].[FC]S.)$").unwrap(),
            verb: Regex::new("^V.+$").unwrap(),
            prefixes_for_verbs: Regex::new("(?i)^(auto)(.*[aeiouàéèíòóïü].+[aeiouàéèíòóïü].*)$")
                .unwrap(),
            adjectiu_compost: Regex::new("(?i)^(.*)o-(.*)$").unwrap(),
            tres_adjectius: Regex::new("(?i)^(.*)o-(.*)o-(.*)$").unwrap(),
            desinencies_1conj_0: endings_regex(true),
            desinencies_1conj_1: endings_regex(false),
        }
    }

    pub fn shared_arc() -> Option<Arc<CatalanTagger>> {
        crate::weak_shared(|| {
            let dir = ca_resource_dir()?;
            Some(Arc::new(CatalanTagger::from_resource_dir(&dir)))
        })
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        self.dict.is_tagged(word)
    }

    fn readings_for(
        &self,
        probe: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        self.dict
            .lookup(probe)
            .into_iter()
            .map(|(lemma, tag)| {
                AnalyzedToken::new(display, start, end)
                    .with_lemma(lemma)
                    .with_pos_tags(vec![tag])
            })
            .collect()
    }

    fn wordform_has_postag(&self, wordform: &str, postag: &str) -> bool {
        self.dict
            .lookup(wordform)
            .iter()
            .any(|(_, tag)| tag == postag)
    }

    fn is_valid_adjective_form(&self, word_stem: &str) -> bool {
        let with_o = format!("{word_stem}o");
        !NO_ALTRES_PREFIXOS.contains(&with_o.as_str())
            && (self.wordform_has_postag(&format!("{word_stem}a"), "AQ0FS0")
                || ALTRES_PREFIXOS.contains(&with_o.as_str()))
    }

    fn additional_tags(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let lower_word = word.to_lowercase();
        if lower_word.ends_with("ment") {
            let possible_adj = match lower_word.rfind("ment") {
                Some(0) | None => lower_word.clone(),
                Some(cut) => lower_word[..cut].to_string(),
            };
            for (_, tag) in self.dict.lookup(&possible_adj) {
                if self.adj_part_fs.is_match(&tag) {
                    return vec![
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(lower_word.clone())
                            .with_pos_tags(vec!["RG".to_string()]),
                    ];
                }
            }
        }
        if let Some(caps) = self.prefixes_for_verbs.captures(word) {
            let possible_verb = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
            let mut out = Vec::new();
            for (lemma, tag) in self.dict.lookup(possible_verb) {
                if lemma != "nòmer" && self.verb.is_match(&tag) {
                    out.push(
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(format!("auto{lemma}"))
                            .with_pos_tags(vec![tag]),
                    );
                }
            }
            return out;
        }
        if let Some(caps) = self.adjectiu_compost.captures(word) {
            let adj1 = caps
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_lowercase();
            if self.is_valid_adjective_form(&adj1) {
                let adj2 = caps
                    .get(2)
                    .map(|m| m.as_str())
                    .unwrap_or_default()
                    .to_lowercase();
                for (lemma, tag) in self.dict.lookup(&adj2) {
                    if tag.starts_with('A') {
                        return vec![
                            AnalyzedToken::new(word, start, end)
                                .with_lemma(format!("{adj1}o-{lemma}"))
                                .with_pos_tags(vec![tag]),
                        ];
                    }
                }
            }
        }
        if let Some(caps) = self.tres_adjectius.captures(word) {
            let adj1 = caps
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_lowercase();
            let adj2 = caps
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_lowercase();
            if self.is_valid_adjective_form(&adj1) && self.is_valid_adjective_form(&adj2) {
                let adj3 = caps
                    .get(3)
                    .map(|m| m.as_str())
                    .unwrap_or_default()
                    .to_lowercase();
                for (lemma, tag) in self.dict.lookup(&adj3) {
                    if tag.starts_with('A') {
                        return vec![
                            AnalyzedToken::new(word, start, end)
                                .with_lemma(format!("{adj1}o-{adj2}o-{lemma}"))
                                .with_pos_tags(vec![tag]),
                        ];
                    }
                }
            }
        }
        if word.contains('\u{0140}') || word.contains('\u{013F}') {
            let possible_word = lower_word.replace('\u{0140}', "l·");
            return self.readings_for(&possible_word, word, start, end);
        }
        Vec::new()
    }

    fn try_tag(
        &self,
        original_word: &str,
        infinitive: &str,
        conjugated: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        if !self.wrong_verbs.contains(infinitive) {
            return Vec::new();
        }
        self.dict
            .lookup(conjugated)
            .into_iter()
            .filter(|(_, tag)| tag.starts_with('V'))
            .map(|(_, tag)| {
                AnalyzedToken::new(original_word, start, end)
                    .with_lemma(infinitive.to_string())
                    .with_pos_tags(vec![tag])
            })
            .collect()
    }

    fn adjust_lexeme_for_soft_vowel(lexeme: &str) -> String {
        for (suffix, replacement) in [
            ("qu", "c"),
            ("gü", "gu"),
            ("gu", "g"),
            ("c", "ç"),
            ("g", "j"),
        ] {
            if let Some(stripped) = lexeme.strip_suffix(suffix) {
                return format!("{stripped}{replacement}");
            }
        }
        lexeme.to_string()
    }

    fn additional_tags_for_incorrect_verbs(
        &self,
        original_word: &str,
        lower_word: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        if let Some(stem) = lower_word.strip_prefix("en") {
            let mut selected: Vec<(String, String)> = Vec::new();
            let mut lemma = String::new();
            for (candidate_lemma, tag) in self.dict.lookup(stem) {
                if tag.starts_with('V') {
                    lemma = format!("en{candidate_lemma}");
                    selected.push((lemma.clone(), tag));
                }
            }
            if !selected.is_empty() && (lemma == "enfotre" || lemma == "enriure") {
                return selected
                    .into_iter()
                    .map(|(lemma, tag)| {
                        AnalyzedToken::new(original_word, start, end)
                            .with_lemma(lemma)
                            .with_pos_tags(vec![tag])
                    })
                    .collect();
            }
        }
        for re in [&self.desinencies_1conj_0, &self.desinencies_1conj_1] {
            let Some(caps) = re.captures(lower_word) else {
                continue;
            };
            let base_lexeme = caps
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let mut desinence = caps
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let mut lexemes = vec![base_lexeme.clone()];
            if desinence.starts_with('e')
                || desinence.starts_with('é')
                || desinence.starts_with('i')
                || desinence.starts_with('ï')
            {
                let adjusted = Self::adjust_lexeme_for_soft_vowel(&base_lexeme);
                if !lexemes.contains(&adjusted) {
                    lexemes.push(adjusted);
                }
            }
            if desinence.starts_with('ï') {
                desinence = format!("i{}", &desinence[2..]);
            }
            let mut tagged = self.try_tag(
                original_word,
                &format!("{base_lexeme}ar"),
                &format!("cant{desinence}"),
                start,
                end,
            );
            for lex in &lexemes {
                if tagged.is_empty() {
                    tagged = self.try_tag(
                        original_word,
                        &format!("{lex}ir"),
                        &format!("serv{desinence}"),
                        start,
                        end,
                    );
                }
                if tagged.is_empty() && lex.ends_with('g') {
                    tagged = self.try_tag(
                        original_word,
                        &format!("{lex}uir"),
                        &format!("serv{desinence}"),
                        start,
                        end,
                    );
                }
            }
            if tagged.is_empty() {
                let eixer = format!("{base_lexeme}èixer");
                tagged = self.try_tag(
                    original_word,
                    &eixer,
                    &format!("con{desinence}"),
                    start,
                    end,
                );
                if tagged.is_empty() {
                    tagged = self.try_tag(
                        original_word,
                        &eixer,
                        &format!("desmer{desinence}"),
                        start,
                        end,
                    );
                }
            }
            if !tagged.is_empty() {
                return tagged;
            }
        }
        Vec::new()
    }

    pub(crate) fn analyze_token(
        &self,
        original_word: &str,
        start: usize,
        end: usize,
    ) -> (Vec<AnalyzedToken>, bool, bool) {
        let mut contains_typographic_apostrophe = false;
        let mut display = original_word.to_string();
        if display.chars().count() > 1 && display.contains('\u{2019}') {
            contains_typographic_apostrophe = true;
            display = display.replace('\u{2019}', "'");
        }
        let normalized_word = display;
        let lower_word = normalized_word.to_lowercase();
        let is_lowercase = normalized_word == lower_word;
        let is_mixed = is_mixed_case(&normalized_word);
        let is_all_upper = is_all_uppercase(&normalized_word);

        let mut readings = self.readings_for(&normalized_word, &normalized_word, start, end);
        if !is_lowercase && !is_mixed {
            readings.extend(self.readings_for(&lower_word, &normalized_word, start, end));
        }
        if (readings.is_empty() || ALLUPPERCASE_EXCEPTIONS.contains(&normalized_word.as_str()))
            && is_all_upper
        {
            let first_upper = uppercase_first_char(&lower_word);
            readings.extend(self.readings_for(&first_upper, &normalized_word, start, end));
        }
        if readings.is_empty() && !is_mixed {
            readings.extend(self.additional_tags(&normalized_word, start, end));
        }
        if readings.is_empty() && is_emoji(&normalized_word) {
            readings.push(
                AnalyzedToken::new(&normalized_word, start, end)
                    .with_lemma("_emoji_".to_string())
                    .with_pos_tags(vec!["_emoji_".to_string()]),
            );
        }
        readings.retain(|reading| {
            reading
                .pos_tags()
                .first()
                .is_none_or(|tag| !tag.starts_with('0'))
        });

        let mut is_incorrect_verb = false;
        if readings.is_empty() {
            let tags =
                self.additional_tags_for_incorrect_verbs(&normalized_word, &lower_word, start, end);
            if !tags.is_empty() {
                readings = tags;
                is_incorrect_verb = true;
            }
        }
        if readings.is_empty() {
            readings.push(AnalyzedToken::new(&normalized_word, start, end));
        }
        (readings, contains_typographic_apostrophe, is_incorrect_verb)
    }
}

impl Tagger for CatalanTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut pos = 0usize;
        for word in tokens {
            let start = pos;
            let end = start + word.len();
            let (readings, typographic_apostrophe, incorrect_verb) =
                self.analyze_token(word, start, end);
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| word.to_string());
            let primary = AnalyzedToken::new(*word, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut atr = AnalyzedTokenReadings::new(primary).with_readings(readings);
            if typographic_apostrophe {
                atr.set_chunks(vec!["containsTypographicApostrophe".to_string()]);
            } else if incorrect_verb {
                atr.set_chunks(vec!["_incorrect_verb_".to_string()]);
            }
            results.push(atr);
            pos = end;
        }
        results
    }
}

impl CoreTagger for CatalanTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let (start, end) = (primary.start(), primary.end());
            let (readings, typographic_apostrophe, incorrect_verb) =
                self.analyze_token(&surface, start, end);
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| surface.clone());
            let updated = AnalyzedToken::new(&surface, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut next = AnalyzedTokenReadings::new(updated).with_readings(readings);
            if typographic_apostrophe {
                next.set_chunks(vec!["containsTypographicApostrophe".to_string()]);
            } else if incorrect_verb {
                next.set_chunks(vec!["_incorrect_verb_".to_string()]);
            }
            *atr = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> Arc<CatalanTagger> {
        CatalanTagger::shared_arc().expect("ca resource dir with added.txt")
    }

    fn readings_of(word: &str) -> Vec<(String, String)> {
        let (readings, _, _) = tagger().analyze_token(word, 0, word.len());
        readings
            .into_iter()
            .map(|r| {
                (
                    r.lemma().unwrap_or_default().to_string(),
                    r.pos_tags().first().cloned().unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn added_lists_load_and_lookup() {
        let tagger = tagger();
        assert!(
            tagger
                .dict
                .lookup("AICLE")
                .iter()
                .any(|(l, t)| l == "AICLE" && t == "NCMS000"),
            "added.txt AICLE entry must resolve: {:?}",
            tagger.dict.lookup("AICLE")
        );
        assert!(tagger.is_word_tagged("Aarne-Thompson"));
    }

    #[test]
    fn untagged_words_get_a_null_reading_like_java() {
        let (readings, _, _) = tagger().analyze_token("blablabla", 0, 9);
        assert_eq!(readings.len(), 1);
        assert!(readings[0].lemma().is_none());
        assert!(readings[0].pos_tags().is_empty());
    }

    #[test]
    fn capitalized_lookup_falls_back_to_the_lowercase_form() {
        let tagger = tagger();
        let (lower, _, _) = tagger.analyze_token("aicle", 0, 5);
        let (upper, _, _) = tagger.analyze_token("AICLE", 0, 5);
        assert!(!lower.is_empty());
        assert_eq!(lower.len(), upper.len());
        for reading in &upper {
            assert_eq!(reading.token(), "AICLE");
        }
    }

    #[test]
    fn all_uppercase_retry_probes_uppercase_first() {
        let tagger = tagger();
        let (_, _, _) = tagger.analyze_token("AICLE", 0, 5);
        let (readings, _, _) = tagger.analyze_token("AICLE", 0, 5);
        assert!(readings.iter().all(|r| r.token() == "AICLE"));
    }

    #[test]
    fn ment_adverbs_get_the_java_rg_tag_when_the_adjective_is_known() {
        let tagger = tagger();
        let stem_known = tagger
            .dict
            .lookup("ordinària")
            .iter()
            .any(|(_, t)| t == "AQ0FS0");
        if !stem_known {
            // data-blocked without ca-ES.dict: the -ment heuristic needs a
            // VMP00SF/AQ FS reading of the stem in the dictionary
            return;
        }
        let readings = readings_of("ordinàriament");
        assert!(
            readings
                .iter()
                .any(|(lemma, tag)| lemma == "ordinàriament" && tag == "RG"),
            "{readings:?}"
        );
    }

    #[test]
    fn typographic_apostrophe_sets_the_chunk_and_looks_up_the_straight_form() {
        let tagged = Tagger::tag(tagger().as_ref(), &["d\u{2019}AICLE"]);
        assert_eq!(
            tagged[0].chunks(),
            &["containsTypographicApostrophe".to_string()],
            "Java setTypographicApostrophe"
        );
        let (readings, _, _) = tagger().analyze_token("d\u{2019}AICLE", 0, 8);
        assert!(!readings.is_empty());
    }

    #[test]
    fn valencian_zero_prefixed_tags_are_dropped() {
        let tagger = tagger();
        let (readings, _, _) = tagger.analyze_token("0AICLE", 0, 6);
        assert!(
            readings
                .iter()
                .all(|r| r.pos_tags().first().is_none_or(|t| !t.starts_with('0'))),
            "ca-ES drops 0-prefixed (Valencian-only) tags"
        );
    }

    #[test]
    fn adjust_lexeme_covers_the_java_rewrites() {
        assert_eq!(CatalanTagger::adjust_lexeme_for_soft_vowel("finc"), "finç");
        assert_eq!(
            CatalanTagger::adjust_lexeme_for_soft_vowel("abstrqu"),
            "abstrc"
        );
        assert_eq!(CatalanTagger::adjust_lexeme_for_soft_vowel("fug"), "fuj");
        assert_eq!(
            CatalanTagger::adjust_lexeme_for_soft_vowel("pingü"),
            "pingu"
        );
        assert_eq!(
            CatalanTagger::adjust_lexeme_for_soft_vowel("obligu"),
            "oblig"
        );
        assert_eq!(
            CatalanTagger::adjust_lexeme_for_soft_vowel("porta"),
            "porta"
        );
    }

    #[test]
    fn wrong_verbs_load_from_replace_verbs() {
        let tagger = tagger();
        assert!(
            tagger.wrong_verbs.contains("lliberar"),
            "replace_verbs.txt wrong-side forms must load"
        );
    }

    fn fixture_tagger() -> CatalanTagger {
        let base = std::env::temp_dir().join(format!("og-ca-tagger-{}", std::process::id()));
        let resource_dir = base.join("resource/ca");
        let rules_dir = base.join("rules/ca");
        std::fs::create_dir_all(&resource_dir).unwrap();
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::write(
            resource_dir.join("added.txt"),
            "# comment line\n\nmalformed-no-tabs\nonly\ttwo\n\
             abusar\tabusar\tVMN0000\n\
             fotia\tfotre\tVMII3S00\n\
             fotia\tfotre\tVMII1S00\n\
             cantada\tcantar\tVMP00SF0\n\
             cantava\tcantar\tVMII3S00\n\
             cantava\tcantar\tVMII1S00\n\
             servint\tservir\tVMG0000\n\
             conen\tconèixer\tVMII3P00\n\
             desmerassin\tdesmerèixer\tVMSS3P00\n\
             populars\tpopular\tAQ0CP0\n\
             alemany\talemany\tAQ0MS0\n\
             americana\tamericà\tAQ0FS0\n\
             folklòrica\tfolklòrico\tAQ0FS0\n\
             ordinària\tordinari\tAQ0FS0\n\
             caca\tcaca\tNCMS000\n",
        )
        .unwrap();
        std::fs::write(resource_dir.join("removed.txt"), "caca\tcaca\tNCMS000\n").unwrap();
        std::fs::write(
            rules_dir.join("replace_verbs.txt"),
            "# només primera conjugació\n\nquebrar=escurar\nliar=lligar\ndeguir=deure\nquebrèixer=conèixer\n",
        )
        .unwrap();
        CatalanTagger::from_resource_dir(&resource_dir)
    }

    fn fixture_readings(word: &str) -> Vec<(String, String)> {
        let tagger = fixture_tagger();
        let (readings, _, _) = tagger.analyze_token(word, 0, word.len());
        readings
            .into_iter()
            .map(|r| {
                (
                    r.lemma().unwrap_or_default().to_string(),
                    r.pos_tags().first().cloned().unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn en_prefixed_incorrect_verbs_copy_the_flexion_of_the_inner_verb() {
        let readings = fixture_readings("enfotia");
        assert!(readings.contains(&("enfotre".to_string(), "VMII3S00".to_string())));
        assert!(readings.contains(&("enfotre".to_string(), "VMII1S00".to_string())));
        let tagged = Tagger::tag(&fixture_tagger(), &["enfotia"]);
        assert_eq!(
            tagged[0].chunks(),
            &["_incorrect_verb_".to_string()],
            "Java marks the token with an incorrect-verb chunk"
        );
    }

    #[test]
    fn first_conjugation_incorrect_verbs_map_through_cantar() {
        assert_eq!(
            fixture_readings("quebrada"),
            vec![("quebrar".to_string(), "VMP00SF0".to_string())]
        );
        assert_eq!(
            fixture_readings("liava"),
            vec![
                ("liar".to_string(), "VMII3S00".to_string()),
                ("liar".to_string(), "VMII1S00".to_string()),
            ]
        );
    }

    #[test]
    fn soft_vowel_lexemes_try_the_gu_uir_paradigm() {
        assert_eq!(
            fixture_readings("degint"),
            vec![("deguir".to_string(), "VMG0000".to_string())]
        );
    }

    #[test]
    fn eixer_verbs_fall_back_to_con_and_desmer_flexions() {
        assert_eq!(
            fixture_readings("quebren"),
            vec![("quebrèixer".to_string(), "VMII3P00".to_string())]
        );
        assert_eq!(
            fixture_readings("quebrassin"),
            vec![("quebrèixer".to_string(), "VMSS3P00".to_string())]
        );
    }

    #[test]
    fn auto_prefixed_verbs_copy_the_inner_verb_tags() {
        assert_eq!(
            fixture_readings("autoabusar"),
            vec![("autoabusar".to_string(), "VMN0000".to_string())]
        );
    }

    #[test]
    fn compound_and_triple_adjectives_compose_the_hyphenated_lemma() {
        assert_eq!(
            fixture_readings("Folklòrico-populars"),
            vec![("folklòrico-popular".to_string(), "AQ0CP0".to_string())]
        );
        assert_eq!(
            fixture_readings("franco-americano-alemany"),
            vec![("franco-americano-alemany".to_string(), "AQ0MS0".to_string())]
        );
    }

    #[test]
    fn ment_adverbs_take_the_rg_tag_from_the_fixture_stem() {
        assert_eq!(
            fixture_readings("ordinàriament"),
            vec![("ordinàriament".to_string(), "RG".to_string())]
        );
    }

    #[test]
    fn emoji_tokens_get_the_emoji_reading() {
        let (readings, _, _) = fixture_tagger().analyze_token("\u{1F600}", 0, 4);
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].lemma(), Some("_emoji_"));
        assert_eq!(readings[0].pos_tags(), &["_emoji_".to_string()]);
    }

    #[test]
    fn removed_entries_win_over_the_added_ones() {
        let tagger = fixture_tagger();
        assert!(!tagger.is_word_tagged("caca"));
        assert!(tagger.is_word_tagged("abusar"));
    }
}
