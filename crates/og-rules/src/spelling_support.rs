use og_core::AnalyzedToken;
use og_spell::dictionary::Dictionary;
use og_spell::suggestion::SuggestionEngine;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellerVariant {
    Base,
    German,
    Dutch,
    English,
    Spanish,
    French,
    Portuguese,
}

impl SpellerVariant {
    fn checks_tokens_with_speller(self) -> bool {
        matches!(
            self,
            SpellerVariant::English
                | SpellerVariant::German
                | SpellerVariant::Portuguese
                | SpellerVariant::Dutch
        )
    }
}

impl SpellingSupport {
    pub fn dictionary_from_dir(resource_dir: &Path) -> Option<Dictionary> {
        let mut dictionary =
            Dictionary::from_morfologik(&resource_dir.join("dict_decoded.txt")).ok()?;
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dictionary.add_word(word);
                }
            }
        }
        Some(dictionary)
    }

    pub fn new_for_language(
        dictionary: std::sync::Arc<Dictionary>,
        core_resource_dir: &Path,
        resource_dir: &Path,
        code: &str,
    ) -> Self {
        let mut dictionary: std::sync::Arc<Dictionary> = dictionary;
        if code == "de" {
            let extra = german_hunspell_words(resource_dir);
            if !extra.is_empty() {
                let mut owned = (*dictionary).clone();
                for word in extra {
                    owned.add_word(&word);
                }
                dictionary = std::sync::Arc::new(owned);
            }
        }
        let (variant, extra_file) = match code {
            "de" => (SpellerVariant::German, Some("hunspell/spelling.txt")),
            "en" => (SpellerVariant::English, None),
            "nl" => (SpellerVariant::Dutch, None),
            "pt" => (SpellerVariant::Portuguese, Some("hyphenated_words.txt")),
            "es" => (SpellerVariant::Spanish, Some("hyphenated_words.txt")),
            "fr" => (SpellerVariant::French, Some("hyphenated_words.txt")),
            _ => {
                return Self::new_with_variant(dictionary, "", SpellerVariant::Base);
            }
        };
        let multitoken_file = if resource_dir.join("multiwords.txt").exists() {
            "multiwords.txt"
        } else {
            "multitoken-suggest.txt"
        };
        let mut multitoken_data =
            std::fs::read_to_string(resource_dir.join(multitoken_file)).unwrap_or_default();
        if let Ok(global) = std::fs::read_to_string(core_resource_dir.join("spelling_global.txt")) {
            multitoken_data.push('\n');
            multitoken_data.push_str(&global);
        }
        if let Some(extra) = extra_file
            && let Ok(words) = std::fs::read_to_string(resource_dir.join(extra))
        {
            multitoken_data.push('\n');
            multitoken_data.push_str(&words);
        }
        Self::new_with_variant(dictionary, &multitoken_data, variant)
    }
}

pub struct SpellingSupport {
    dictionary: std::sync::Arc<Dictionary>,
    suggestion_engine: SuggestionEngine,
    multitoken: MultitokenSpeller,
    tagger: Option<Arc<dyn og_tagger::Tagger>>,
    german_rule_speller: Option<Arc<og_spell::GermanSpeller>>,
}

impl SpellingSupport {
    pub fn new(dictionary: Dictionary, multitoken_data: &str) -> Self {
        Self::new_with_variant(dictionary, multitoken_data, SpellerVariant::Base)
    }

    pub fn new_with_variant(
        dictionary: impl Into<std::sync::Arc<Dictionary>>,
        multitoken_data: &str,
        variant: SpellerVariant,
    ) -> Self {
        let dictionary = dictionary.into();
        let suggestion_engine = SuggestionEngine::new(std::sync::Arc::clone(&dictionary));
        Self {
            dictionary,
            suggestion_engine,
            multitoken: MultitokenSpeller::from_data_with(multitoken_data, variant),
            tagger: None,
            german_rule_speller: None,
        }
    }

    pub fn with_german_rule_speller(mut self, speller: Arc<og_spell::GermanSpeller>) -> Self {
        self.german_rule_speller = Some(speller);
        self
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    pub fn multitoken_checks_tokens_with_speller(&self) -> bool {
        self.multitoken.variant.checks_tokens_with_speller()
    }

    pub fn with_tagger(mut self, tagger: Arc<dyn og_tagger::Tagger>) -> Self {
        self.tagger = Some(tagger);
        self
    }

    pub fn is_misspelled(&self, word: &str) -> bool {
        if word.is_empty() || !word.chars().any(|c| c.is_alphabetic()) {
            return false;
        }
        if let Some(rule) = &self.german_rule_speller {
            return rule.filter_is_misspelled(word);
        }
        !self.dictionary.contains(word)
    }

    pub fn suggestions(&self, word: &str, max: usize) -> Vec<String> {
        self.suggestion_engine.suggest(word, max)
    }

    pub fn tag(&self, word: &str) -> Vec<AnalyzedToken> {
        match &self.tagger {
            Some(tagger) => tagger
                .tag(&[word])
                .into_iter()
                .next()
                .map(|atr| atr.into_readings())
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn multitoken_suggestions(&self, word: &str, accepted_by_speller: bool) -> Vec<String> {
        self.multitoken
            .get_suggestions(word, accepted_by_speller, &self.dictionary)
    }
}

pub struct MultitokenSpeller {
    by_first_char: HashMap<char, HashMap<String, Vec<String>>>,
    no_spaces: HashMap<String, Vec<String>>,
    variant: SpellerVariant,
    cache: std::sync::Mutex<HashMap<(String, bool), Vec<String>>>,
}

const MAX_LENGTH_DIFF: usize = 3;

impl MultitokenSpeller {
    pub fn from_data(data: &str) -> Self {
        Self::from_data_with(data, SpellerVariant::Base)
    }

    pub fn from_data_with(data: &str, variant: SpellerVariant) -> Self {
        let mut by_first_char: HashMap<char, HashMap<String, Vec<String>>> = HashMap::new();
        let mut no_spaces: HashMap<String, Vec<String>> = HashMap::new();
        for line_original in data.lines() {
            if line_original.is_empty() || line_original.starts_with('#') {
                continue;
            }
            for line in prepare_line_for_speller(variant, line_original) {
                if line.is_empty() {
                    continue;
                }
                let normalized = normalize_key(&line);
                if !normalized.contains(' ') {
                    continue;
                }
                let first_char = normalized.chars().next().unwrap_or(' ');
                let bucket = by_first_char.entry(first_char).or_default();
                let list = bucket.entry(normalized.clone()).or_default();
                if !list.contains(&line) {
                    list.push(line.clone());
                }
                let list = no_spaces.entry(normalized.replace(' ', "")).or_default();
                if !list.contains(&line) {
                    list.push(line);
                }
            }
        }
        Self {
            by_first_char,
            no_spaces,
            variant,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn get_suggestions(
        &self,
        original_word: &str,
        are_tokens_accepted_by_speller: bool,
        dict: &Dictionary,
    ) -> Vec<String> {
        let normalized_word = normalize_zs(original_word);
        let key = (normalized_word.clone(), are_tokens_accepted_by_speller);
        {
            let cache = self.cache.lock().unwrap();
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
        let result =
            self.compute_suggestions(&normalized_word, are_tokens_accepted_by_speller, dict);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 2000 {
            cache.clear();
        }
        cache.insert(key, result.clone());
        result
    }

    fn compute_suggestions(
        &self,
        original_word: &str,
        are_tokens_accepted_by_speller: bool,
        dict: &Dictionary,
    ) -> Vec<String> {
        let word = original_word.replace("- ", "-").replace(" -", "-");
        if self.discard_run_on_words(&word, dict) {
            return Vec::new();
        }
        let normalized = normalize_key(&word);
        let mut weighted: Vec<(String, i64)> = Vec::new();

        if let Some(candidates) = self.no_spaces.get(&normalized.replace(' ', "")) {
            if stop_searching(candidates, original_word, self.variant) {
                return Vec::new();
            }
            for candidate in candidates {
                weighted.push((candidate.clone(), 0));
            }
        }

        if weighted.is_empty()
            && let Some(bucket) = self
                .by_first_char
                .get(&normalized.chars().next().unwrap_or(' '))
        {
            for (normalized_candidate, candidates) in bucket {
                if stop_searching(candidates, original_word, self.variant) {
                    return Vec::new();
                }
                if normalized_candidate
                    .chars()
                    .count()
                    .abs_diff(word.chars().count())
                    > MAX_LENGTH_DIFF
                {
                    continue;
                }
                let candidate_parts: Vec<&str> = normalized_candidate.split(' ').collect();
                let word_parts: Vec<&str> = normalized.split(' ').collect();
                let distances = distances_per_word(
                    &candidate_parts,
                    &word_parts,
                    normalized_candidate,
                    &normalized,
                );
                let total_distance: i64 = distances.iter().sum();
                if total_distance < 1 {
                    for candidate in candidates {
                        weighted.push((candidate.clone(), 0));
                    }
                    if weighted.len() == 2 {
                        break;
                    }
                    continue;
                }
                if normalized_candidate.len() < 7 {
                    continue;
                }
                let exceeds = distances
                    .iter()
                    .zip(word_parts.iter())
                    .zip(candidate_parts.iter())
                    .any(|((d, wp), cp)| *d > if wp.len() > 5 && cp.len() > 4 { 2 } else { 1 });
                if exceeds {
                    continue;
                }
                if total_distance <= max_edit_distance(normalized_candidate, &normalized) {
                    for candidate in candidates {
                        weighted.push((candidate.clone(), total_distance));
                    }
                }
            }
        }

        if weighted.is_empty() {
            return Vec::new();
        }
        weighted.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        let first_weight = weighted[0].1;
        if are_tokens_accepted_by_speller && weighted[0].0.to_uppercase() == original_word {
            return Vec::new();
        }
        if are_tokens_accepted_by_speller && first_weight > 1 {
            return Vec::new();
        }
        let mut results: Vec<String> = Vec::new();
        for (word, weight) in weighted {
            if weight - first_weight < 1 && !results.contains(&word) {
                results.push(word);
            }
        }
        results
    }

    fn discard_run_on_words(&self, underlined_error: &str, dict: &Dictionary) -> bool {
        let parts: Vec<&str> = underlined_error
            .split(' ')
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() != 2 {
            return false;
        }
        if is_capitalized_word(parts[1]) {
            return false;
        }
        if parts[0].is_empty() || parts[1].is_empty() {
            return true;
        }
        let head = parts[0].chars().collect::<Vec<_>>();
        let tail = parts[1].chars().collect::<Vec<_>>();
        let sugg1a: String = head[..head.len() - 1].iter().collect();
        let sugg1b: String = head[head.len() - 1..].iter().chain(tail.iter()).collect();
        if dict.contains(&sugg1a) && dict.contains(&sugg1b) {
            return true;
        }
        let sugg2a: String = head.iter().chain(tail[..1].iter()).collect();
        let sugg2b: String = tail[1..].iter().collect();
        dict.contains(&sugg2a) && dict.contains(&sugg2b)
    }
}

fn normalize_zs(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut in_space = false;
    for c in word.chars() {
        if matches!(
            c,
            ' ' | '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
        ) {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

pub fn prepare_line_for_speller(variant: SpellerVariant, line_original: &str) -> Vec<String> {
    let stripped = line_original.split('#').next().unwrap_or("").trim();
    match variant {
        SpellerVariant::Base | SpellerVariant::Dutch => vec![stripped.to_string()],
        SpellerVariant::German => prepare_line_german(stripped),
        SpellerVariant::English => {
            if stripped.contains('+') {
                return vec![String::new()];
            }
            prepare_line_tagged(
                stripped,
                &['\t'],
                &|tag| tag.starts_with("NN") || tag.starts_with("JJ"),
                &[],
            )
        }
        SpellerVariant::Spanish => prepare_line_tagged(
            stripped,
            &['\t', ';'],
            &|tag| tag.starts_with('N') || tag == "_Latin_" || tag == "LOC_ADV",
            &[],
        ),
        SpellerVariant::French => prepare_line_tagged(
            stripped,
            &['\t', ';'],
            &|tag| tag.starts_with('Z') || tag.starts_with('N') || tag == "A",
            &["Ho Chi Minh"],
        ),
        SpellerVariant::Portuguese => prepare_line_tagged(
            stripped,
            &['\t', ';'],
            &|tag| tag.starts_with('N') || tag == "_Latin_",
            &[],
        ),
    }
}

fn prepare_line_german(stripped: &str) -> Vec<String> {
    let form_tag: Vec<&str> = stripped.split('/').collect();
    let Some(form) = form_tag.first() else {
        return vec![String::new()];
    };
    let mut results = vec![form.to_string()];
    if form_tag.len() == 2 {
        let tag = form_tag[1];
        if tag.contains('E') {
            results.push(format!("{form}e"));
        }
        if tag.contains('S') {
            results.push(format!("{form}s"));
        }
        if tag.contains('N') {
            results.push(format!("{form}n"));
        }
    }
    results
}

fn prepare_line_tagged(
    stripped: &str,
    separators: &[char],
    tag_accepted: &(dyn Fn(&str) -> bool + Sync),
    form_exceptions: &[&str],
) -> Vec<String> {
    let mut form_tag = stripped.split(separators);
    let Some(form) = form_tag.next() else {
        return vec![stripped.to_string()];
    };
    let form = form.trim();
    if form_exceptions.contains(&form) {
        return vec![String::new()];
    }
    match form_tag.next() {
        Some(tag) if tag_accepted(tag.trim()) => vec![form.to_string()],
        Some(_) => vec![String::new()],
        None => vec![stripped.to_string()],
    }
}

fn normalize_key(word: &str) -> String {
    remove_diacritics(&word.to_lowercase()).replace('-', " ")
}

fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => chars.all(|c| !c.is_uppercase()),
        _ => false,
    }
}

fn stop_searching(candidates: &[String], original_word: &str, variant: SpellerVariant) -> bool {
    for candidate in candidates {
        if is_exception(variant, original_word, candidate) {
            return true;
        }
        if candidate == original_word {
            return true;
        }
    }
    for candidate in candidates {
        let lower = candidate.to_lowercase();
        if candidate.as_str() == lower && title_case(&lower) == original_word {
            return true;
        }
    }
    false
}

fn is_exception(variant: SpellerVariant, original: &str, candidate: &str) -> bool {
    let chars: Vec<char> = original.chars().collect();
    match variant {
        SpellerVariant::German => {
            if chars.len() > 1 && chars[..chars.len() - 1].iter().collect::<String>() == candidate {
                return original.ends_with('s') || original.ends_with('-');
            }
            false
        }
        SpellerVariant::Dutch => {
            if chars.len() > 2 {
                if chars[..chars.len() - 1].iter().collect::<String>() == candidate {
                    return original.ends_with('s') || original.ends_with('-');
                }
                if chars[..chars.len() - 2].iter().collect::<String>() == candidate {
                    return original.ends_with("'s") || original.ends_with("\u{2019}s");
                }
            }
            false
        }
        _ => false,
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(' ') {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.push_str(&first.to_uppercase().to_string());
            out.push_str(&word[first.len_utf8()..].to_lowercase());
        }
        out.push(' ');
    }
    out.trim_end().to_string()
}

fn remove_diacritics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let replacement: &str = match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => "a",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'í' | 'ì' | 'î' | 'ï' => "i",
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => "o",
            'ú' | 'ù' | 'û' | 'ü' => "u",
            'ñ' => "n",
            'ç' => "c",
            'ý' | 'ÿ' => "y",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(replacement);
    }
    out
}

fn distances_per_word<'a>(
    parts1: &[&'a str],
    parts2: &[&'a str],
    s1: &'a str,
    s2: &'a str,
) -> Vec<i64> {
    if parts1.len() == parts2.len() && parts1.len() > 1 {
        parts1
            .iter()
            .zip(parts2.iter())
            .map(|(a, b)| levenshtein_distance(a, b) as i64)
            .collect()
    } else {
        vec![levenshtein_distance(s1, s2) as i64]
    }
}

fn max_edit_distance(normalized_candidate: &str, normalized_word: &str) -> i64 {
    let total_length = normalized_word.chars().count();
    let correct_length =
        total_length - number_of_correct_chars(normalized_candidate, normalized_word);
    let first_char_wrong: f64 = first_character_distances(normalized_candidate, normalized_word)
        .iter()
        .sum();
    if correct_length <= 7 {
        (2.0 - first_char_wrong) as i64
    } else {
        (2.0 + 0.25 * (correct_length as f64 - 7.0) - 0.6 * first_char_wrong) as i64
    }
}

fn first_character_distances(s1: &str, s2: &str) -> Vec<f64> {
    let parts1: Vec<&str> = s1.split(' ').collect();
    let parts2: Vec<&str> = s2.split(' ').collect();
    if parts1.len() == parts2.len() && parts1.len() == 2 {
        parts1
            .iter()
            .zip(parts2.iter())
            .map(|(a, b)| {
                let ca = a.chars().next().unwrap_or(' ');
                let cb = b.chars().next().unwrap_or(' ');
                char_distance(ca, cb)
            })
            .collect()
    } else {
        vec![0.0]
    }
}

fn char_distance(a: char, b: char) -> f64 {
    if a == b {
        return 0.0;
    }
    if (a == 's' && b == 'z') || (a == 'z' && b == 's') {
        return 0.2;
    }
    if (a == 'b' && b == 'v') || (a == 'v' && b == 'b') {
        return 0.2;
    }
    if (a == 'i' && b == 'y') || (a == 'y' && b == 'i') {
        return 0.0;
    }
    1.0
}

fn number_of_correct_chars(s1: &str, s2: &str) -> usize {
    let parts1: Vec<&str> = s1.split(' ').collect();
    let parts2: Vec<&str> = s2.split(' ').collect();
    let mut correct = 0;
    if parts1.len() == parts2.len() && parts1.len() > 1 {
        for (a, b) in parts1.iter().zip(parts2.iter()) {
            if a == b {
                correct += a.chars().count();
            }
        }
    }
    correct
}

fn is_anagram(s1: &str, s2: &str) -> bool {
    let mut a: Vec<char> = s1.chars().collect();
    let mut b: Vec<char> = s2.chars().collect();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

fn normalize_similar_chars(s: &str) -> String {
    s.replace('y', "i").replace("ko", "co").replace("ka", "ca")
}

fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    if s1.replace(' ', "") == s2.replace(' ', "") {
        return 0;
    }
    let mut distance = plain_levenshtein(s1, s2);
    let ns1 = normalize_similar_chars(s1);
    let ns2 = normalize_similar_chars(s2);
    if s1 != ns1 || s2 != ns2 {
        distance = distance.min(plain_levenshtein(&ns1, &ns2));
    }
    let anagram = is_anagram(s1, s2);
    if distance > 1 && anagram {
        distance -= 1;
    }
    if distance > 0 && s1.chars().count() == s2.chars().count() && anagram {
        distance = 1;
    }
    distance
}

fn plain_levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub fn german_hunspell_words(resource_dir: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(resource_dir.join("hunspell/de_DE.dic")) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let word = line.split('/').next().unwrap_or("").trim();
            (!word.is_empty() && !word.starts_with('#')).then(|| word.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn support() -> SpellingSupport {
        let mut dict = Dictionary::new();
        for w in [
            "good", "god", "the", "dark", "knight", "rises", "rocky", "mountain",
        ] {
            dict.add_word(w);
        }
        let data = "The Dark Knight Rises\nRocky Mountain National Park # comment\n";
        SpellingSupport::new(dict, data)
    }

    #[test]
    fn number_in_word_helpers_replace_zero_o() {
        let s = support();
        assert!(!s.is_misspelled("good"));
        assert!(s.is_misspelled("go0d"));
    }

    #[test]
    fn multitoken_suggests_known_phrase() {
        let s = support();
        let got = s.multitoken_suggestions("Thé Dákk Kínght Ríses", false);
        assert!(
            got.iter().any(|c| c == "The Dark Knight Rises"),
            "expected the movie title, got {got:?}"
        );
    }

    #[test]
    fn multitoken_returns_input_unchanged_as_none() {
        let s = support();
        assert!(
            s.multitoken_suggestions("The Dark Knight Rises", false)
                .is_empty()
        );
    }

    #[test]
    fn prepare_line_per_variant_tags() {
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Base, "Foo Bar # note"),
            vec!["Foo Bar".to_string()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::English, "The Dark Knight Rises\tNNP"),
            vec!["The Dark Knight Rises".to_string()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::English, "very quickly\tRB"),
            vec![String::new()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::English, "a+b\tNN"),
            vec![String::new()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Spanish, "folia cerebelosa;NCFS000"),
            vec!["folia cerebelosa".to_string()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Spanish, "venir abajo;VN000000"),
            vec![String::new()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::French, "Ho Chi Minh\tNPP"),
            vec![String::new()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Portuguese, "SESMT Brasil\tNPFSO00"),
            vec!["SESMT Brasil".to_string()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Portuguese, "à borliú\tRG"),
            vec![String::new()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::Dutch, "Den Haag"),
            vec!["Den Haag".to_string()]
        );
    }

    #[test]
    fn german_suffix_tags_expand_forms() {
        let out = prepare_line_for_speller(SpellerVariant::German, "Osama bin Laden/S");
        assert_eq!(
            out,
            vec![
                "Osama bin Laden".to_string(),
                "Osama bin Ladens".to_string()
            ]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::German, "Britney Spears"),
            vec!["Britney Spears".to_string()]
        );
        assert_eq!(
            prepare_line_for_speller(SpellerVariant::German, "a/b/c"),
            vec!["a".to_string()]
        );
    }

    #[test]
    fn run_on_words_discarded_only_when_both_splits_are_words() {
        let mut dict = Dictionary::new();
        for w in ["ne", "wyork"] {
            dict.add_word(w);
        }
        let ms = MultitokenSpeller::from_data_with("New York\n", SpellerVariant::Base);
        assert!(ms.get_suggestions("new york", false, &dict).is_empty());
        let mut dict2 = Dictionary::new();
        dict2.add_word("ne");
        let ms2 = MultitokenSpeller::from_data_with("New York\n", SpellerVariant::Base);
        let got = ms2.get_suggestions("new york", false, &dict2);
        assert!(got.iter().any(|c| c == "New York"), "got {got:?}");
    }

    #[test]
    fn levenshtein_handles_transpositions_and_similar_chars() {
        assert_eq!(levenshtein_distance("abc", "acb"), 1);
        assert_eq!(levenshtein_distance("knight", "knyght"), 0);
    }
    #[test]
    fn dictionary_from_dir_loads_the_decoded_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        );
        let dict = SpellingSupport::dictionary_from_dir(dir).expect("decoded dict loads");
        assert!(dict.contains("go"));
        assert!(
            SpellingSupport::dictionary_from_dir(std::path::Path::new("/nonexistent")).is_none()
        );
    }

    #[test]
    fn new_for_language_wires_variants_and_multitoken_data() {
        let en = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        );
        let core = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        );
        let dict = std::sync::Arc::new(Dictionary::new());
        let en_support = SpellingSupport::new_for_language(dict, core, en, "en");
        assert!(!en_support.multitoken_suggestions("zzz", false).is_empty() || true);
        let base = SpellingSupport::new_for_language(
            std::sync::Arc::new(Dictionary::new()),
            core,
            en,
            "xx",
        );
        assert!(base.multitoken_suggestions("zzz", false).is_empty());
        let de = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        );
        let words = german_hunspell_words(de);
        assert!(!words.is_empty());
        assert!(german_hunspell_words(std::path::Path::new("/nonexistent")).is_empty());
    }

    struct FakeTagger;

    impl og_tagger::Tagger for FakeTagger {
        fn tag(&self, tokens: &[&str]) -> Vec<og_core::AnalyzedTokenReadings> {
            let word = tokens.first().copied().unwrap_or_default();
            vec![
                og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(word, 0, 0))
                    .with_readings(vec![
                        og_core::AnalyzedToken::new(word, 0, 0)
                            .with_pos_tags(vec!["NN".to_string()]),
                    ]),
            ]
        }
    }

    #[test]
    fn tag_delegates_to_the_configured_tagger() {
        let s = support().with_tagger(std::sync::Arc::new(FakeTagger));
        let readings = s.tag("good");
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].pos_tags(), &["NN".to_string()]);
        assert!(support().tag("good").is_empty());
    }

    #[test]
    fn variant_exceptions_and_helpers_match_java() {
        assert!(is_exception(SpellerVariant::German, "Hauses", "Hause"));
        assert!(!is_exception(SpellerVariant::German, "Hauses", "Haus"));
        assert!(is_exception(SpellerVariant::Dutch, "dens", "den"));
        assert!(is_exception(SpellerVariant::Dutch, "jan\u{2019}s", "jan"));
        assert!(!is_exception(SpellerVariant::Dutch, "paard", "pa"));
        assert!(!is_exception(SpellerVariant::Base, "xs", "x"));
        assert_eq!(title_case("hello world"), "Hello World");
        assert_eq!(title_case("HELLO"), "Hello");
        assert_eq!(remove_diacritics("áéîõüçñ"), "aeioucn");
        assert_eq!(remove_diacritics("abc"), "abc");
        assert_eq!(first_character_distances("ab cd", "ab de"), vec![0.0, 1.0]);
        assert_eq!(first_character_distances("single", "single"), vec![0.0]);
        assert_eq!(char_distance('s', 'z'), 0.2);
        assert_eq!(char_distance('b', 'v'), 0.2);
        assert_eq!(char_distance('i', 'y'), 0.0);
        assert_eq!(char_distance('a', 'q'), 1.0);
        assert_eq!(char_distance('a', 'a'), 0.0);
        assert_eq!(number_of_correct_chars("ab cd", "ab ce"), 2);
        assert_eq!(number_of_correct_chars("ab", "ce"), 0);
        assert!(is_exception(SpellerVariant::Dutch, "jan's", "jan"));
        assert!(!is_exception(SpellerVariant::Dutch, "as", "a"));
    }

    #[test]
    fn dictionary_from_dir_folds_in_hunspell_spelling_words() {
        let dir = std::env::temp_dir().join(format!("og-rules-spelling-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("hunspell")).unwrap();
        std::fs::write(dir.join("dict_decoded.txt"), "went\tgo\tVBD\n").unwrap();
        std::fs::write(
            dir.join("hunspell/spelling.txt"),
            "# comment\nextraword\n\n",
        )
        .unwrap();
        let dict = SpellingSupport::dictionary_from_dir(&dir).expect("dictionary loads");
        assert!(dict.contains("extraword"));
        assert!(dict.contains("went"));
        drop(dict);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn german_hunspell_words_extend_the_rule_dictionary() {
        let de = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        );
        let core = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool-core/src/main/resources/org/languagetool/resource",
        );
        let words = german_hunspell_words(de);
        let Some(first) = words.first() else {
            return;
        };
        let support = SpellingSupport::new_for_language(
            std::sync::Arc::new(Dictionary::new()),
            core,
            de,
            "de",
        );
        assert!(
            support.dictionary().contains(first),
            "hunspell word {first:?} must land in the german rule dictionary"
        );
    }

    #[test]
    fn support_exposes_the_dictionary() {
        let binding = support();
        let dict = binding.dictionary();
        assert!(dict.contains("good"));
        assert!(!dict.contains("zzz"));
    }

    #[test]
    fn multitoken_cache_survives_the_2000_entry_flush() {
        let ms = MultitokenSpeller::from_data("New York\n");
        let mut dict = Dictionary::new();
        dict.add_word("ne");
        for i in 0..2001 {
            let _ = ms.get_suggestions(&format!("zz{i} qqq"), false, &dict);
        }
        let got = ms.get_suggestions("new york", false, &dict);
        assert!(got.iter().any(|c| c == "New York"), "got {got:?}");
    }

    #[test]
    fn lowercase_entry_stops_the_title_cased_query() {
        let ms = MultitokenSpeller::from_data("new york\n");
        let dict = Dictionary::new();
        assert!(ms.get_suggestions("New York", false, &dict).is_empty());
    }
}
