use og_core::Language;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MINIMAL_CONFIDENCE: f64 = 0.9;
const SHORT_TEXT_ALGORITHM: usize = 50;
const ALPHA: f64 = 0.5;
const BASE_FREQ: f64 = 10_000.0;
const CONV_THRESHOLD: f64 = 0.99999;
const N_TRIAL: usize = 7;
const ITERATION_LIMIT: usize = 1000;
const PROBABILITY_THRESHOLD: f64 = 0.1;
const DEFAULT_SEED: u64 = 41;

struct Profile {
    lang: String,
    freq: HashMap<Box<str>, u32>,
    totals: [u64; 3],
}

pub struct LanguageDetector {
    ngram_probs: HashMap<Box<str>, Vec<f64>>,
    langs: Vec<String>,
}

impl LanguageDetector {
    pub fn from_lt_modules(lt_modules: &Path) -> Self {
        let mut profiles = Vec::new();
        let bundled = [
            "af", "ar", "ast", "be", "bg", "bn", "br", "ca", "cs", "cy", "da", "de", "el", "en",
            "eo", "es", "et", "eu", "fa", "fi", "fr", "ga", "gl", "gu", "he", "hi", "hr", "hu",
            "id", "it", "ja", "ka", "kn", "ko", "la", "lt", "lv", "mk", "ml", "mr", "ne", "nl",
            "no", "pa", "pl", "pt", "ro", "ru", "sk", "sl", "so", "sq", "sr", "sv", "sw", "ta",
            "te", "th", "tl", "tr", "uk", "ur", "vi", "zh-cn", "zh-tw",
        ];
        for dir in bundled_profile_dirs() {
            for code in &bundled {
                if let Some(profile) = read_profile_file(&dir.join(format!("{code}.profile"))) {
                    profiles.push(profile);
                }
            }
            if !profiles.is_empty() {
                break;
            }
        }
        let languages = [
            "ar", "ast", "be", "br", "ca", "crh", "da", "de", "el", "en", "es", "fa", "fr", "ga",
            "gl", "it", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sr", "sv", "ta", "tl", "uk",
            "zh",
        ];
        for code in languages {
            let resource_dir = lt_modules.join(format!(
                "{code}/src/main/resources/org/languagetool/resource/{code}"
            ));
            if profiles.iter().any(|p| p.lang == code) {
                continue;
            }
            let profile_path = resource_dir.join(format!("{code}.profile"));
            if let Some(profile) = read_profile_file(&profile_path) {
                profiles.push(profile);
                continue;
            }
            let common_path = resource_dir.join("common_words.txt");
            if let Ok(words) = std::fs::read_to_string(&common_path) {
                profiles.push(profile_from_common_words(code, &words));
            }
        }
        let langs: Vec<String> = profiles.iter().map(|p| p.lang.clone()).collect();
        let ngram_probs = build_ngram_probs(&profiles);
        Self { ngram_probs, langs }
    }

    pub fn shared() -> Option<Arc<Self>> {
        static DETECTOR: og_core::releasable::Releasable<LanguageDetector> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&DETECTOR, || {
            crate::engine::find_lt_resource_dir().map(|root| Arc::new(Self::from_lt_modules(&root)))
        })
    }

    pub fn detect(&self, text: &str, preferred: &[&str]) -> Option<(String, f64)> {
        let (lang, prob) = self.best_from_scores(self.all_scores(text))?;
        if !preferred.is_empty() && !preferred.contains(&lang.as_str()) {
            return None;
        }
        Some((lang, prob))
    }

    pub fn all_scores(&self, text: &str) -> Vec<(String, f64)> {
        let filtered = normalize_text(text);
        if filtered.is_empty() {
            return Vec::new();
        }
        let probs = self.detect_block(&filtered);
        let mut out: Vec<(String, f64)> = self
            .langs
            .iter()
            .zip(probs.iter())
            .filter(|(_, p)| **p >= PROBABILITY_THRESHOLD)
            .map(|(lang, p)| (lang.clone(), *p))
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    fn detect_block(&self, text: &str) -> Vec<f64> {
        let padded = format!(" {text} ");
        let padded = padded.trim();
        let padded = format!(" {padded} ");
        if padded.chars().count() <= SHORT_TEXT_ALGORITHM {
            let grams = extract_counted_grams(&padded);
            if grams.is_empty() {
                return vec![0.0; self.langs.len()];
            }
            self.detect_short(&grams)
        } else {
            let grams = extract_grams_java(&padded);
            if grams.is_empty() {
                return vec![0.0; self.langs.len()];
            }
            self.detect_long(&grams)
        }
    }

    fn detect_short(&self, grams: &HashMap<String, u32>) -> Vec<f64> {
        let mut prob = self.init_probability();
        for (gram, count) in grams {
            self.update_lang_prob(&mut prob, gram, *count as usize, ALPHA);
            if normalize_prob(&mut prob) > CONV_THRESHOLD {
                break;
            }
        }
        normalize_prob(&mut prob);
        prob
    }

    fn detect_long(&self, grams: &[String]) -> Vec<f64> {
        let mut langprob = vec![0.0; self.langs.len()];
        let mut rng = Lcg::new(DEFAULT_SEED);
        for _ in 0..N_TRIAL {
            let mut prob = self.init_probability();
            let alpha = ALPHA + rng.next_gaussian() * 0.05;
            for i in 0..ITERATION_LIMIT {
                let r = rng.next_below(grams.len());
                self.update_lang_prob(&mut prob, &grams[r], 1, alpha);
                if i % 5 == 0 && normalize_prob(&mut prob) > CONV_THRESHOLD {
                    break;
                }
            }
            for (j, lp) in langprob.iter_mut().enumerate() {
                *lp += prob[j] / N_TRIAL as f64;
            }
        }
        langprob
    }

    fn init_probability(&self) -> Vec<f64> {
        vec![1.0 / self.langs.len() as f64; self.langs.len()]
    }

    fn update_lang_prob(&self, prob: &mut [f64], gram: &str, count: usize, alpha: f64) {
        let Some(lang_probs) = self.ngram_probs.get(gram) else {
            return;
        };
        let weight = alpha / BASE_FREQ;
        for (i, p) in prob.iter_mut().enumerate() {
            for _ in 0..count {
                *p *= weight + lang_probs[i];
            }
        }
    }

    fn best_from_scores(&self, scores: Vec<(String, f64)>) -> Option<(String, f64)> {
        if scores.is_empty() {
            return None;
        }
        let max = scores
            .iter()
            .fold(f64::NEG_INFINITY, |acc, (_, s)| acc.max(*s));
        if !max.is_finite() {
            return None;
        }
        let exp_sum: f64 = scores.iter().map(|(_, s)| (s - max).exp()).sum();
        let mut best: Option<(usize, f64)> = None;
        for (idx, (_, score)) in scores.iter().enumerate() {
            let prob = (score - max).exp() / exp_sum;
            if best.is_none_or(|(_, p)| prob > p) {
                best = Some((idx, prob));
            }
        }
        let (idx, prob) = best?;
        if prob < MINIMAL_CONFIDENCE {
            return None;
        }
        Some((scores[idx].0.clone(), prob))
    }
}

fn read_profile_file(path: &Path) -> Option<Profile> {
    let raw = std::fs::read_to_string(path).ok()?;
    parse_optimaize_profile(&raw)
}

fn bundled_profile_dirs() -> Vec<PathBuf> {
    [
        PathBuf::from("lang-profiles"),
        PathBuf::from("../lang-profiles"),
        PathBuf::from("/home/agent/opengrammar-rs/lang-profiles"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

fn build_ngram_probs(profiles: &[Profile]) -> HashMap<Box<str>, Vec<f64>> {
    let langsize = profiles.len();
    let mut map: HashMap<Box<str>, Vec<f64>> = HashMap::new();
    for (index, profile) in profiles.iter().enumerate() {
        for (gram, frequency) in &profile.freq {
            let entry = map
                .entry(gram.clone())
                .or_insert_with(|| vec![0.0; langsize]);
            let len = gram.chars().count().clamp(1, 3);
            let total = profile.totals[len - 1].max(1) as f64;
            entry[index] = *frequency as f64 / total;
        }
    }
    map
}

fn standard_filter_ok(gram: &str) -> bool {
    let chars: Vec<char> = gram.chars().collect();
    match chars.len() {
        1 => chars[0] != ' ',
        2 => true,
        3 => chars[1] != ' ',
        _ => chars[1] != ' ' && chars[2] != ' ',
    }
}

fn extract_counted_grams(padded: &str) -> HashMap<String, u32> {
    let chars: Vec<char> = padded.chars().collect();
    let mut out = HashMap::new();
    for n in 1..=3usize {
        if chars.len() < n {
            continue;
        }
        for window in chars.windows(n) {
            let gram: String = window.iter().collect();
            if standard_filter_ok(&gram) {
                *out.entry(gram).or_insert(0u32) += 1;
            }
        }
    }
    out
}

fn extract_grams_java(padded: &str) -> Vec<String> {
    let chars: Vec<char> = padded.chars().collect();
    let mut out = Vec::new();
    for n in 1..=3usize {
        if chars.len() < n {
            continue;
        }
        for window in chars.windows(n) {
            let gram: String = window.iter().collect();
            if standard_filter_ok(&gram) {
                out.push(gram);
            }
        }
    }
    out
}

fn normalize_prob(prob: &mut [f64]) -> f64 {
    let sump: f64 = prob.iter().sum();
    if sump <= 0.0 {
        return 0.0;
    }
    let mut maxp = 0.0f64;
    for p in prob.iter_mut() {
        *p /= sump;
        if *p > maxp {
            maxp = *p;
        }
    }
    maxp
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_below(&mut self, bound: usize) -> usize {
        (self.next() >> 16) as usize % bound.max(1)
    }

    fn next_gaussian(&mut self) -> f64 {
        let u1 = (self.next_below(1_000_003) as f64 + 1.0) / 1_000_004.0;
        let u2 = (self.next_below(1_000_003) as f64 + 1.0) / 1_000_004.0;
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

fn parse_optimaize_profile(raw: &str) -> Option<Profile> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let lang = value.get("lang")?.as_str()?.to_string();
    let freq_raw = value.get("freq")?.as_object()?;
    let mut freq: HashMap<Box<str>, u32> = HashMap::with_capacity(freq_raw.len());
    let mut totals = [0u64; 3];
    for (gram, count) in freq_raw {
        let count = count.as_u64()? as u32;
        let len = gram.chars().count().clamp(1, 3);
        totals[len - 1] += count as u64;
        freq.insert(gram.clone().into_boxed_str(), count);
    }
    if let Some(n_words) = value.get("n_words").and_then(|v| v.as_array()) {
        for (i, n) in n_words.iter().take(3).enumerate() {
            if let Some(n) = n.as_u64() {
                totals[i] = n;
            }
        }
    }
    for total in &mut totals {
        if *total == 0 {
            *total = 1;
        }
    }
    Some(Profile { lang, freq, totals })
}

fn profile_from_common_words(code: &str, words: &str) -> Profile {
    let mut freq: HashMap<Box<str>, u32> = HashMap::new();
    let mut totals = [0u64; 3];
    for line in words.lines() {
        let word = line.trim();
        if word.is_empty() || word.starts_with('#') {
            continue;
        }
        let padded = format!(" {word} ");
        let chars: Vec<char> = padded.chars().collect();
        for n in 1..=3 {
            if chars.len() < n {
                continue;
            }
            for window in chars.windows(n) {
                let gram: String = window.iter().collect();
                *freq.entry(gram.into_boxed_str()).or_insert(0) += 1;
                totals[n - 1] += 1;
            }
        }
    }
    for total in &mut totals {
        if *total == 0 {
            *total = 1;
        }
    }
    Profile {
        lang: code.to_string(),
        freq,
        totals,
    }
}

fn normalize_text(text: &str) -> String {
    let url_free = strip_urls(text);
    let mut out = String::with_capacity(url_free.len());
    let mut last_was_space = true;
    for c in url_free.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '’' {
            out.push(c.to_lowercase().next().unwrap_or(c));
            last_was_space = false;
        } else if !last_was_space {
            out.push(' ');
            last_was_space = true;
        }
    }
    out.trim_end().to_string()
}

fn strip_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("http://").or_else(|| rest.find("https://")) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

pub fn detected_language_from_code(code: &str) -> Option<Language> {
    Language::from_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> Option<LanguageDetector> {
        crate::engine::find_lt_resource_dir().map(|root| LanguageDetector::from_lt_modules(&root))
    }

    #[test]
    fn detects_english_and_german() {
        let Some(detector) = detector() else {
            return;
        };
        let (lang, conf) = detector
            .detect(
                "The quick brown fox jumps over the lazy dog and then it runs away quickly.",
                &[],
            )
            .expect("english detection");
        assert_eq!(lang, "en");
        assert!(conf > 0.9, "confidence {conf}");

        let (lang, _) = detector
            .detect(
                "Der schnelle braune Fuchs springt über den faulen Hund und rennt danach schnell davon.",
                &[],
            )
            .expect("german detection");
        assert_eq!(lang, "de");
    }

    #[test]
    fn preferred_languages_restrict_detection() {
        let Some(detector) = detector() else {
            return;
        };
        let result = detector.detect("The quick brown fox jumps over the lazy dog.", &["de"]);
        assert!(result.is_none());
    }
}
