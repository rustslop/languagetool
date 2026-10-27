use og_core::checker::Tagger as CoreTagger;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use og_tagger::Tagger;
use std::path::Path;
use std::sync::Arc;

const MAX_TOKEN_UTF16_LENGTH: usize = 50;

pub(crate) fn br_resource_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("br/src/main/resources/org/languagetool/resource/br"))
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

fn eq_ignore_unicode_case(a: &str, b: &str) -> bool {
    let mut a_chars = a.chars().flat_map(char::to_lowercase);
    let mut b_chars = b.chars().flat_map(char::to_lowercase);
    loop {
        match (a_chars.next(), b_chars.next()) {
            (Some(x), Some(y)) if x == y => continue,
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn strip_retry_suffix(word: &str) -> Option<&str> {
    for suffix in ["mañ", "se", "hont"] {
        let Some(cut) = word.len().checked_sub(suffix.len() + 1) else {
            continue;
        };
        if !word.is_char_boundary(cut) {
            continue;
        }
        let stem = &word[..cut];
        let tail = &word[cut..];
        if tail.starts_with('-')
            && eq_ignore_unicode_case(&tail[1..], suffix)
            && stem.chars().count() >= 2
        {
            return Some(stem);
        }
    }
    None
}

pub struct BretonTagger {
    inner: Arc<og_tagger::GenericTagger>,
}

impl BretonTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            inner: Arc::new(og_tagger::GenericTagger::from_resource_dir(
                resource_dir,
                "breton",
            )),
        }
    }

    pub fn shared_arc() -> Option<Arc<BretonTagger>> {
        crate::weak_shared(|| {
            let dir = br_resource_dir()?;
            Some(Arc::new(BretonTagger::from_resource_dir(&dir)))
        })
    }

    pub fn has_dictionary(&self) -> bool {
        self.inner.has_dictionary()
    }

    fn probe_readings(
        &self,
        probe: &str,
        display: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let readings = self.inner.tag_word(probe, start, end);
        if probe == display {
            return readings;
        }
        readings
            .into_iter()
            .map(|reading| {
                let lemma = reading
                    .lemma()
                    .map(str::to_string)
                    .unwrap_or_else(|| display.to_string());
                AnalyzedToken::new(display, start, end)
                    .with_lemma(lemma)
                    .with_pos_tags(reading.pos_tags().to_vec())
            })
            .collect()
    }

    // Java BretonTagger.tag: retry the dictionary without the -mañ/-se/-hont
    // suffix before falling back to an untagged reading.
    pub(crate) fn analyzed_tokens_at(
        &self,
        word: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        if word.chars().map(char::len_utf16).sum::<usize>() > MAX_TOKEN_UTF16_LENGTH {
            return Vec::new();
        }
        let mut probe = word;
        loop {
            let lower = probe.to_lowercase();
            let is_lowercase = probe == lower;
            let mut readings = self.probe_readings(probe, word, start, end);
            if readings.is_empty() && is_lowercase {
                let upper = uppercase_first_char(probe);
                let upper_readings = self.probe_readings(&upper, word, start, end);
                if !upper_readings.is_empty() {
                    readings = upper_readings;
                }
            }
            if readings.is_empty()
                && let Some(stem) = strip_retry_suffix(probe)
            {
                probe = stem;
                continue;
            }
            return readings;
        }
    }
}

impl Tagger for BretonTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut pos = 0usize;
        for word in tokens {
            let start = pos;
            let end = start + word.len();
            let readings = self.analyzed_tokens_at(word, start, end);
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
            results.push(AnalyzedTokenReadings::new(primary).with_readings(readings));
            pos = end;
        }
        results
    }
}

impl CoreTagger for BretonTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let (start, end) = (primary.start(), primary.end());
            let readings = self.analyzed_tokens_at(&surface, start, end);
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
            *atr = AnalyzedTokenReadings::new(updated).with_readings(readings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger() -> Arc<BretonTagger> {
        BretonTagger::shared_arc().expect("br resource dir with breton.dict")
    }

    fn readings_of(word: &str) -> Vec<(String, String)> {
        tagger()
            .analyzed_tokens_at(word, 0, word.len())
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
    fn dictionary_tags_c_h_words_whole() {
        let readings = readings_of("c’havotenn");
        assert!(
            readings.iter().any(|(lemma, _)| lemma == "gavotenn"),
            "expected the soft-mutation base lemma, got {readings:?}"
        );
    }

    #[test]
    fn retry_suffix_stems_tag_displaying_the_surface() {
        for (word, stem) in [
            ("e-kerzh-se", "e-kerzh"),
            ("dibenn-sizhun-mañ", "dibenn-sizhun"),
            ("a-raok-hont", "a-raok"),
        ] {
            let stem_readings = readings_of(stem);
            let readings = readings_of(word);
            assert_eq!(
                readings.len(),
                stem_readings.len(),
                "'{word}' must retry with stem '{stem}'"
            );
        }
    }

    #[test]
    fn retry_requires_two_chars_before_the_hyphen() {
        assert_eq!(strip_retry_suffix("xx-mañ"), Some("xx"));
        assert_eq!(strip_retry_suffix("x-mañ"), None);
        assert_eq!(strip_retry_suffix("xx-SE"), Some("xx"));
        assert_eq!(strip_retry_suffix("xx-MAÑ"), Some("xx"));
        assert_eq!(strip_retry_suffix("xx-hont"), Some("xx"));
        assert_eq!(strip_retry_suffix("xx-man"), None);
    }

    #[test]
    fn overlong_tokens_stay_untagged_like_java() {
        let long = "a".repeat(51);
        assert!(tagger().analyzed_tokens_at(&long, 0, long.len()).is_empty());
        let boundary = "a".repeat(50);
        let readings = tagger().analyzed_tokens_at(&boundary, 0, boundary.len());
        assert!(readings.is_empty(), "50 a's are not in the dictionary");
    }
}
