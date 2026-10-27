use crate::Tagger;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, PartialEq, Eq)]
struct TaggedWord {
    lemma: Box<str>,
    tag: u32,
}

pub struct EnglishTagger {
    tags: Vec<Box<str>>,
    dict: HashMap<Box<str>, Box<[TaggedWord]>>,
    fsa: HashMap<Box<str>, Vec<TaggedWord>>,
    manual: HashMap<Box<str>, Vec<TaggedWord>>,
    removals: HashMap<Box<str>, Vec<(Box<str>, u32)>>,
}

fn is_all_uppercase(s: &str) -> bool {
    !s.chars().any(|c| c.is_lowercase())
}

fn is_not_all_lowercase(s: &str) -> bool {
    s.chars().any(|c| c.is_uppercase())
}

fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => chars.all(|c| !c.is_uppercase()),
        _ => false,
    }
}

fn is_mixed_case(s: &str) -> bool {
    !is_all_uppercase(s) && !is_capitalized_word(s) && is_not_all_lowercase(s)
}

fn uppercase_first_char(s: &str) -> String {
    let mut out: String = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}

fn split_manual_line<'a>(line: &'a str, separator: &str) -> Option<(&'a str, &'a str, &'a str)> {
    let no_comment = line.split('#').next().unwrap_or("").trim();
    if no_comment.is_empty() {
        return None;
    }
    let mut parts = no_comment.splitn(3, separator);
    let form = parts.next()?.trim();
    let lemma = parts.next()?.trim();
    let tag = parts.next()?.trim();
    if form.is_empty() || lemma.is_empty() || tag.is_empty() {
        return None;
    }
    Some((form, lemma, tag))
}

impl EnglishTagger {
    pub fn new() -> Self {
        Self {
            tags: Vec::new(),
            dict: HashMap::new(),
            fsa: HashMap::new(),
            manual: HashMap::new(),
            removals: HashMap::new(),
        }
    }

    pub fn is_word_tagged(&self, word: &str) -> bool {
        !self.tag_word(word, 0, word.len()).is_empty()
    }

    pub fn from_resource_dir(dir: &Path) -> Self {
        let mut tagger = Self::new();
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        if let Some(data) = read("dict_decoded.txt") {
            tagger.load_fsa_data(&data);
        }
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
        tagger.load_manual_data(&added, &removed);
        tagger.finish();
        tagger
    }

    pub fn load_fsa_data(&mut self, data: &str) -> usize {
        let mut count = 0;
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
            let tag = tag.trim();
            if form.is_empty() || tag.is_empty() {
                continue;
            }
            let tag_id = self.intern_tag(tag);
            let lemma: Box<str> = if lemma.is_empty() {
                form.into()
            } else {
                lemma.into()
            };
            self.fsa
                .entry(form.into())
                .or_default()
                .push(TaggedWord { lemma, tag: tag_id });
            count += 1;
        }
        count
    }

    pub fn load_manual_data(&mut self, added: &str, removed: &str) {
        let mut separator = "\t";
        for line in added.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("#separatorRegExp=") {
                separator = rest;
                continue;
            }
            if let Some((form, lemma, tag)) = split_manual_line(line, separator) {
                let tag_id = self.intern_tag(tag);
                self.manual
                    .entry(form.into())
                    .or_default()
                    .push(TaggedWord {
                        lemma: lemma.into(),
                        tag: tag_id,
                    });
            }
        }
        for line in removed.lines() {
            let line = line.trim();
            if let Some((form, lemma, tag)) = split_manual_line(line, separator) {
                let tag_id = self.intern_tag(tag);
                self.removals
                    .entry(form.into())
                    .or_default()
                    .push((lemma.into(), tag_id));
            }
        }
    }

    pub fn finish(&mut self) {
        let fsa = std::mem::take(&mut self.fsa);
        let manual = std::mem::take(&mut self.manual);
        let removals = std::mem::take(&mut self.removals);
        let mut merged: HashMap<Box<str>, Vec<TaggedWord>> = manual;
        for (form, mut readings) in fsa {
            readings.sort_by_cached_key(|tw| {
                (
                    fsa_order_key(&form, &tw.lemma),
                    self.tags[tw.tag as usize].clone(),
                )
            });
            merged.entry(form).or_default().append(&mut readings);
        }
        self.dict = merged
            .into_iter()
            .map(|(form, mut readings)| {
                if let Some(removals) = removals.get(&form) {
                    readings.retain(|tw| {
                        !removals
                            .iter()
                            .any(|(lemma, tag)| tw.tag == *tag && *tw.lemma == *lemma.as_ref())
                    });
                }
                (form, readings.into_boxed_slice())
            })
            .collect();
    }

    pub fn entry_count(&self) -> usize {
        self.dict.len()
    }

    fn intern_tag(&mut self, tag: &str) -> u32 {
        if let Some(pos) = self.tags.iter().position(|t| &**t == tag) {
            return pos as u32;
        }
        self.tags.push(tag.into());
        (self.tags.len() - 1) as u32
    }

    fn lookup(&self, word: &str) -> &[TaggedWord] {
        self.dict.get(word).map(|v| &**v).unwrap_or(&[])
    }

    fn readings_for(
        &self,
        surface: &str,
        word: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let entries = self.lookup(word);
        entries
            .iter()
            .map(|tw| {
                AnalyzedToken::new(surface, start, end)
                    .with_lemma(tw.lemma.to_string())
                    .with_pos_tags(vec![self.tags[tw.tag as usize].to_string()])
            })
            .collect()
    }

    pub fn tag_word(&self, surface: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let word = normalize_english_word(surface);

        let lower_word = word.to_lowercase();
        let is_lowercase = word == lower_word;
        let is_mixed = is_mixed_case(&word);
        let is_all_upper = is_all_uppercase(&word);

        let mut readings = self.readings_for(&word, &word, start, end);
        if !is_lowercase && !is_mixed {
            readings.extend(self.readings_for(&word, &lower_word, start, end));
        }
        if readings.is_empty() && is_all_upper {
            let first_upper = uppercase_first_char(&lower_word);
            readings.extend(self.readings_for(&word, &first_upper, start, end));
        }
        if readings.is_empty() && lower_word.ends_with("in'") {
            let mut corrected = word.to_string();
            let corrected_tail = if is_all_upper { 'G' } else { 'g' };
            corrected.pop();
            corrected.push(corrected_tail);
            readings.extend(self.readings_for(&word, &corrected, start, end));
            if !is_lowercase && !is_mixed {
                readings.extend(self.readings_for(&word, &corrected.to_lowercase(), start, end));
            }
        }
        readings
    }
}

fn normalize_english_word(surface: &str) -> std::borrow::Cow<'_, str> {
    if surface.chars().count() > 1 && surface.contains('’') {
        std::borrow::Cow::Owned(surface.replace('’', "'"))
    } else {
        std::borrow::Cow::Borrowed(surface)
    }
}

fn fsa_order_key(word: &str, lemma: &str) -> Vec<u8> {
    // morfologik SUFFIX encoder: the FSA orders a word's readings by the
    // encoded lemma (b'A' + trimmed-suffix-count, then the lemma tail) and tag
    let w = word.as_bytes();
    let l = lemma.as_bytes();
    let mut i = 0;
    while i < w.len() && i < l.len() && w[i] == l[i] {
        i += 1;
    }
    let mut key = vec![b'A'.wrapping_add((w.len() - i) as u8)];
    key.extend_from_slice(&l[i..]);
    key
}

impl Default for EnglishTagger {
    fn default() -> Self {
        Self::new()
    }
}

impl Tagger for EnglishTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for token_text in tokens {
            let start = byte_offset;
            let end = start + token_text.len();
            let has_typographic_apostrophe =
                token_text.chars().count() > 1 && token_text.contains('’');
            let word = normalize_english_word(token_text);
            let readings = self.tag_word(token_text, start, end);
            let mut atr = build_atr(&word, start, end, readings);
            if has_typographic_apostrophe {
                atr.mark_typographic_apostrophe();
            }
            results.push(atr);
            byte_offset = end;
        }
        results
    }
}

fn build_atr(
    word: &str,
    start: usize,
    end: usize,
    readings: Vec<AnalyzedToken>,
) -> AnalyzedTokenReadings {
    let all_tags: Vec<String> = readings
        .iter()
        .flat_map(|r| r.pos_tags().to_vec())
        .collect();
    let lemma = readings
        .first()
        .and_then(|r| r.lemma().map(str::to_string))
        .unwrap_or_else(|| word.to_string());
    let primary = AnalyzedToken::new(word, start, end)
        .with_pos_tags(all_tags)
        .with_lemma(lemma);
    let mut atr = AnalyzedTokenReadings::new(primary);
    if !readings.is_empty() {
        atr = atr.with_readings(readings);
    }
    atr
}

impl og_core::checker::Tagger for EnglishTagger {
    fn tag(&self, tokens: &mut [og_core::AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let has_typographic_apostrophe = surface.chars().count() > 1 && surface.contains('’');
            let word = normalize_english_word(&surface);
            let readings = self.tag_word(&surface, start, end);
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| word.to_string());
            let updated = AnalyzedToken::new(word.as_ref(), start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut next = AnalyzedTokenReadings::new(updated);
            if !readings.is_empty() {
                next = next.with_readings(readings);
            }
            if has_typographic_apostrophe {
                next.mark_typographic_apostrophe();
            }
            *atr = next;
        }
    }
}

pub fn shared_english_tagger() -> Option<std::sync::Arc<EnglishTagger>> {
    static SHARED: std::sync::OnceLock<std::sync::Mutex<std::sync::Weak<EnglishTagger>>> =
        std::sync::OnceLock::new();
    let shared = SHARED.get_or_init(Default::default);
    if let Some(hit) = shared.lock().ok().and_then(|guard| guard.upgrade()) {
        return Some(hit);
    }
    let candidates = [
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        "languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
    ];
    let built = candidates.iter().find_map(|p| {
        let path = Path::new(p);
        if path.join("dict_decoded.txt").is_file() {
            Some(std::sync::Arc::new(EnglishTagger::from_resource_dir(path)))
        } else {
            None
        }
    });
    if let Some(built) = built.as_ref()
        && let Ok(mut guard) = shared.lock()
    {
        *guard = std::sync::Arc::downgrade(built);
    }
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded_tagger() -> EnglishTagger {
        let mut t = EnglishTagger::new();
        t.load_fsa_data(
            "the\tthe\tDT\nrun\trun\tNN\nrun\trun\tVB\nruns\trun\tVBZ\nrunning\trun\tVBG\nFrance\tFrance\tNNP\njohn\tjohn\tNNP\n",
        );
        t.finish();
        t
    }

    #[test]
    fn exact_form_lookup_before_lowercase() {
        let t = loaded_tagger();
        let atr = t.tag(&["the"]).remove(0);
        let tags: Vec<&str> = atr.all_pos_tags();
        assert_eq!(tags, vec!["DT"]);
    }

    #[test]
    fn capitalized_word_gets_lowercase_readings() {
        let t = loaded_tagger();
        let atr = t.tag(&["The"]).remove(0);
        assert!(atr.has_pos_tag("DT"));
    }

    #[test]
    fn all_uppercase_proper_noun_uses_first_upper_lookup() {
        let t = loaded_tagger();
        let atr = t.tag(&["FRANCE"]).remove(0);
        assert!(atr.has_pos_tag("NNP"));
    }

    #[test]
    fn mixed_case_gets_no_lowercase_readings() {
        let t = loaded_tagger();
        let atr = t.tag(&["RuNning"]).remove(0);
        assert!(atr.all_pos_tags().is_empty());
    }

    #[test]
    fn typographic_apostrophe_is_normalized() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("can't\tcan\tMD\n");
        t.finish();
        let atr = t.tag(&["can’t"]).remove(0);
        assert!(atr.has_pos_tag("MD"));
        assert_eq!(atr.token().token(), "can't");
    }

    #[test]
    fn typographic_apostrophe_sets_the_marker_chunk() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("'s\tbe\tVBZ\ncan't\tcan\tMD\n");
        t.finish();
        let curly = t.tag(&["can’t"]).remove(0);
        assert!(curly.has_typographic_apostrophe());
        let straight = t.tag(&["can't"]).remove(0);
        assert!(!straight.has_typographic_apostrophe());
        let lone_curly = t.tag(&["’"]).remove(0);
        assert!(!lone_curly.has_typographic_apostrophe());
    }

    #[test]
    fn in_suffix_falls_back_to_ing() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("running\trun\tVBG\n");
        t.finish();
        let atr = t.tag(&["runnin'"]).remove(0);
        assert!(atr.has_pos_tag("VBG"));
        let atr = t.tag(&["RUNNIN'"]).remove(0);
        assert!(atr.has_pos_tag("VBG"));
    }

    #[test]
    fn manual_additions_come_first_and_removals_apply() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("word\tword\tNN\n");
        t.load_manual_data("word\tword\tVB\n", "other\tother\tNNP\n");
        t.finish();
        let atr = t.tag(&["word"]).remove(0);
        assert_eq!(atr.all_pos_tags(), vec!["VB", "NN"]);
        assert!(t.tag(&["other"]).remove(0).all_pos_tags().is_empty());
    }

    #[test]
    fn offsets_are_exact_without_separator_guessing() {
        let t = loaded_tagger();
        let result = t.tag(&["the", "runs"]);
        assert_eq!(result[0].token().start(), 0);
        assert_eq!(result[0].token().end(), 3);
        assert_eq!(result[1].token().start(), 3);
        assert_eq!(result[1].token().end(), 7);
    }

    #[test]
    fn manual_lines_support_comments() {
        let mut t = EnglishTagger::new();
        t.load_manual_data("# comment\nword\tword\tNN # trailing\n", "");
        t.finish();
        assert!(t.tag(&["word"]).remove(0).has_pos_tag("NN"));
    }

    #[test]
    fn real_dictionary_loads_and_tags_like_java() {
        let dir = Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        );
        if !dir.exists() {
            return;
        }
        let t = EnglishTagger::from_resource_dir(dir);
        assert!(t.entry_count() > 200_000, "expected full dictionary");
        assert!(t.tag(&["the"]).remove(0).has_pos_tag("DT"));
        assert!(t.tag(&["House"]).remove(0).has_pos_tag("NN"));
        assert!(t.tag(&["running"]).remove(0).has_pos_tag("VBG"));
        assert!(t.tag(&["better"]).remove(0).has_pos_tag("JJR"));
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn is_word_tagged_answers_membership() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("run\trun\tVB\n");
        t.finish();
        assert!(t.is_word_tagged("run"));
        assert!(!t.is_word_tagged("walk"));
    }

    #[test]
    fn manual_separator_override_and_malformed_lines() {
        let mut t = EnglishTagger::new();
        t.load_manual_data("#separatorRegExp=,\nword,word,NN\nbad,line\n", "");
        t.finish();
        assert!(t.tag(&["word"]).remove(0).has_pos_tag("NN"));
        assert!(t.tag(&["bad"]).remove(0).all_pos_tags().is_empty());
    }

    #[test]
    fn fsa_lines_with_empty_lemma_default_to_the_form() {
        let mut t = EnglishTagger::new();
        let count = t.load_fsa_data("word\t\tNN\n# comment\n\nbroken\n");
        assert_eq!(count, 1);
        t.finish();
        let atr = t.tag(&["word"]).remove(0);
        assert_eq!(atr.token().lemma(), Some("word"));
        assert!(atr.has_pos_tag("NN"));
    }

    #[test]
    fn from_resource_dir_without_files_yields_an_empty_tagger() {
        let dir = std::env::temp_dir();
        let t = EnglishTagger::from_resource_dir(&dir);
        assert_eq!(t.entry_count(), 0);
        assert!(t.tag(&["anything"]).remove(0).all_pos_tags().is_empty());
    }

    #[test]
    fn checker_trait_tagging_rewrites_sentence_tokens() {
        let mut t = EnglishTagger::new();
        t.load_fsa_data("the\tthe\tDT\ncan't\tcan\tMD\n");
        t.finish();
        let mut tokens = vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("the", 0, 3)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("can’t", 4, 9)),
        ];
        <EnglishTagger as og_core::checker::Tagger>::tag(&t, &mut tokens);
        assert_eq!(tokens[1].token().token(), "the");
        assert!(tokens[1].has_pos_tag("DT"));
        assert_eq!(tokens[2].token().token(), "can't");
        assert!(tokens[2].has_pos_tag("MD"));
        assert!(tokens[2].has_typographic_apostrophe());
        assert_eq!(tokens[2].token().start(), 4);
        assert_eq!(tokens[2].token().end(), 9);
        assert!(tokens[0].token().token() == "<S>");
    }

    #[test]
    fn shared_english_tagger_returns_the_same_instance() {
        let first = shared_english_tagger().expect("english resources");
        assert!(first.tag(&["the"]).remove(0).has_pos_tag("DT"));
        let second = shared_english_tagger().expect("english resources again");
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }
}
