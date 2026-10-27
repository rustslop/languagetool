use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

type Readings = Vec<(&'static str, &'static str)>;

fn adjective_part_fs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:VMP00SF|A[QO].[FC]S.)$").unwrap())
}

fn verb_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^V.+$").unwrap())
}

fn prefixes_for_verbs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(auto)([^r]...+)$").unwrap())
}

fn prefixes_for_verbs2_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(autor)(r...+)$").unwrap())
}

fn prefixes_for_adjectives_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(super)(.*[aeiouàéèíòóïü].+[aeiouàéèíòóïü].*)$").unwrap())
}

fn prefixes_for_adj_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(.+)-(.+)$").unwrap())
}

fn adj_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^AQ.+$").unwrap())
}

fn adj_ms_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:AQ.MS.|AQ.CS.|AQ.MN.)$").unwrap())
}

fn no_prefixes_for_adj_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(?:anti|pre|ex|pro|afro|ultra|super|súper)$").unwrap())
}

fn adj_vp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:AQ.*|V.P.*)$").unwrap())
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

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => word.to_string(),
    }
}

fn is_word_for_speller(word: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[\p{L}0-9\p{P}\p{Zs}]+$").unwrap())
        .is_match(word)
}

fn is_emoji(word: &str) -> bool {
    let utf16_len: usize = word.chars().map(char::len_utf16).sum();
    utf16_len > 1 && word.chars().count() != utf16_len && !is_word_for_speller(word)
}

fn intern_tag(tag: &str) -> &'static str {
    static POOL: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let mut guard = POOL
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
        .lock()
        .unwrap();
    if let Some(existing) = guard.get(tag) {
        return existing;
    }
    let leaked: &'static str = og_tagger::morfologik::intern_string(tag.to_string());
    guard.insert(leaked);
    leaked
}

fn current_scheme_tag(tag: &str) -> &'static str {
    if tag.contains('+') {
        intern_tag(&tag.replace('+', ":"))
    } else {
        intern_tag(tag)
    }
}

pub struct SpanishTagger {
    dict: Option<Arc<MorfologikDictionary>>,
    manual: HashMap<Box<str>, Readings>,
    removals: HashMap<Box<str>, Readings>,
}

impl SpanishTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        let dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "spanish");
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

    pub fn shared() -> Option<Arc<SpanishTagger>> {
        Self::shared_arc()
    }

    pub fn shared_arc() -> Option<Arc<SpanishTagger>> {
        crate::weak_shared(|| {
            crate::es::es_resource_dir().map(|dir| Arc::new(SpanishTagger::from_resource_dir(&dir)))
        })
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
            readings.extend(
                dict.lookup(word)
                    .iter()
                    .map(|(lemma, tag)| (*lemma, current_scheme_tag(tag))),
            );
        }
        if let Some(removals) = self.removals.get(word) {
            readings.retain(|(lemma, tag)| {
                !removals.iter().any(|(r_lemma, r_tag)| {
                    r_lemma == lemma && (*r_tag == *tag || r_tag.replace('+', ":") == *tag)
                })
            });
        }
        readings
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        let normalized = word.replace('\u{2019}', "'");
        let readings = self.readings_for_word(&normalized, "", "");
        readings.first().is_some_and(|r| !r.pos_tags().is_empty())
    }

    fn readings_for_word(&self, word: &str, prev: &str, next: &str) -> Vec<AnalyzedToken> {
        let mut word = word.to_string();
        if word.chars().count() > 1
            || prev.eq_ignore_ascii_case("l")
            || prev.eq_ignore_ascii_case("d")
            || next.eq_ignore_ascii_case("s")
        {
            word = word.replace('\u{2019}', "'");
        }
        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let is_mixed = is_mixed_case(&word);
        let is_all_upper = is_all_uppercase(&word);

        let mut l: Vec<AnalyzedToken> = Vec::new();
        l.extend(self.lookup(&word).into_iter().map(|(lemma, tag)| {
            AnalyzedToken::new(word.clone(), 0, 0)
                .with_lemma(lemma.to_string())
                .with_pos_tags(vec![tag.to_string()])
        }));
        if !is_lowercase && !is_mixed {
            l.extend(self.lookup(&lower_word).into_iter().map(|(lemma, tag)| {
                AnalyzedToken::new(word.clone(), 0, 0)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![tag.to_string()])
            }));
        }
        if is_all_upper {
            let first_upper = uppercase_first_char(&lower_word);
            l.extend(self.lookup(&first_upper).into_iter().map(|(lemma, tag)| {
                AnalyzedToken::new(word.clone(), 0, 0)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![tag.to_string()])
            }));
        }
        if l.is_empty() && !is_mixed {
            l.extend(self.additional_tags(&word));
        }
        if l.is_empty() && is_emoji(&word) {
            l.push(
                AnalyzedToken::new(word.clone(), 0, 0)
                    .with_lemma("_emoji_".to_string())
                    .with_pos_tags(vec!["_emoji_".to_string()]),
            );
        }
        l
    }

    fn dict_lookup(&self, word: &str) -> Readings {
        self.dict
            .as_ref()
            .map(|d| {
                d.lookup(word)
                    .iter()
                    .map(|(lemma, tag)| (*lemma, current_scheme_tag(tag)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn additional_tags(&self, word: &str) -> Vec<AnalyzedToken> {
        let mut additional: Vec<AnalyzedToken> = Vec::new();
        let lower_word = word.to_lowercase();
        if let Some(possible_adj) = lower_word
            .strip_suffix("mente")
            .filter(|_| lower_word.len() > 5)
        {
            for (_, tag) in self.dict_lookup(possible_adj) {
                if adjective_part_fs_re().is_match(tag) {
                    additional.push(
                        AnalyzedToken::new(word, 0, 0)
                            .with_lemma(lower_word.clone())
                            .with_pos_tags(vec!["RG".to_string()]),
                    );
                    return additional;
                }
            }
        }
        if let Some(caps) = prefixes_for_verbs_re().captures(word) {
            let prefix = caps.get(1).unwrap().as_str();
            let possible_verb = caps.get(2).unwrap().as_str().to_lowercase();
            for (lemma, tag) in self.dict_lookup(&possible_verb) {
                if verb_re().is_match(tag) {
                    additional.push(
                        AnalyzedToken::new(word, 0, 0)
                            .with_lemma(format!("{}{}", prefix.to_lowercase(), lemma))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
            return additional;
        }
        if let Some(caps) = prefixes_for_verbs2_re().captures(word) {
            let prefix = caps.get(1).unwrap().as_str();
            let possible_verb = caps.get(2).unwrap().as_str().to_lowercase();
            for (lemma, tag) in self.dict_lookup(&possible_verb) {
                if verb_re().is_match(tag) {
                    additional.push(
                        AnalyzedToken::new(word, 0, 0)
                            .with_lemma(format!("{}{}", prefix.to_lowercase(), lemma))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
            return additional;
        }
        if let Some(caps) = prefixes_for_adjectives_re().captures(word) {
            let prefix = caps.get(1).unwrap().as_str();
            let possible_stem = caps.get(2).unwrap().as_str().to_lowercase();
            for (lemma, tag) in self.dict_lookup(&possible_stem) {
                if adj_vp_re().is_match(tag) {
                    additional.push(
                        AnalyzedToken::new(word, 0, 0)
                            .with_lemma(format!("{}{}", prefix.to_lowercase(), lemma))
                            .with_pos_tags(vec![tag.to_string()]),
                    );
                }
            }
            return additional;
        }
        if let Some(caps) = prefixes_for_adj_re().captures(word) {
            let possible_adj_prefix = caps.get(1).unwrap().as_str().to_lowercase();
            if !no_prefixes_for_adj_re().is_match(&possible_adj_prefix) {
                let possible_adj = caps.get(2).unwrap().as_str().to_lowercase();
                let prefix_matches = self
                    .dict_lookup(&possible_adj_prefix)
                    .iter()
                    .any(|(_, tag)| adj_ms_re().is_match(tag));
                let mut adj_matches = false;
                let mut new_postag = "";
                let mut new_lemma = String::new();
                for (lemma, tag) in self.dict_lookup(&possible_adj) {
                    if adj_re().is_match(tag) {
                        adj_matches = true;
                        new_postag = tag;
                        new_lemma = format!("{}-{}", possible_adj_prefix, lemma);
                        break;
                    }
                }
                if adj_matches && prefix_matches {
                    additional.push(
                        AnalyzedToken::new(word, 0, 0)
                            .with_lemma(new_lemma)
                            .with_pos_tags(vec![new_postag.to_string()]),
                    );
                    return additional;
                }
            }
        }
        additional
    }
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

impl Tagger for SpanishTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for (i, token_text) in tokens.iter().enumerate() {
            let start = byte_offset;
            let end = start + token_text.len();
            let prev = if i > 0 { tokens[i - 1] } else { "" };
            let next = if i + 1 < tokens.len() {
                tokens[i + 1]
            } else {
                ""
            };
            let readings = self.readings_for_word(token_text, prev, next);
            let mut atr = build_atr(token_text, readings, start, end);
            if token_text.contains('\u{2019}')
                && (token_text.chars().count() > 1
                    || prev.eq_ignore_ascii_case("l")
                    || prev.eq_ignore_ascii_case("d")
                    || next.eq_ignore_ascii_case("s"))
            {
                atr.set_chunks(vec!["containsTypographicApostrophe".to_string()]);
            }
            results.push(atr);
            byte_offset = end;
        }
        results
    }
}

impl CoreTagger for SpanishTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        let surfaces: Vec<String> = tokens
            .iter()
            .skip(1)
            .map(|atr| atr.token().token().to_string())
            .collect();
        for (i, surface) in surfaces.iter().enumerate() {
            let token_idx = i + 1;
            let primary = tokens[token_idx].token().clone();
            let prev = if token_idx > 1 {
                tokens[token_idx - 1].token().token().to_string()
            } else {
                String::new()
            };
            let next = if token_idx + 1 < tokens.len() {
                tokens[token_idx + 1].token().token().to_string()
            } else {
                String::new()
            };
            let readings = self.readings_for_word(surface, &prev, &next);
            let typographic = surface.contains('\u{2019}')
                && (surface.chars().count() > 1
                    || prev.eq_ignore_ascii_case("l")
                    || prev.eq_ignore_ascii_case("d")
                    || next.eq_ignore_ascii_case("s"));
            let mut next_atr = build_atr(surface, readings, primary.start(), primary.end());
            if typographic {
                next_atr.set_chunks(vec!["containsTypographicApostrophe".to_string()]);
            }
            tokens[token_idx] = next_atr;
        }
    }
}
