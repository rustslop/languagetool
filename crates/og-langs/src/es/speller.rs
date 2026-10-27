use std::sync::Arc;

struct EsSpellerOracle {
    surfaces: Vec<String>,
    char_offsets: Vec<u32>,
    chars: Vec<char>,
    dictionary: og_spell::Dictionary,
}

impl EsSpellerOracle {
    fn candidate_chars(&self, i: usize) -> &[char] {
        &self.chars[self.char_offsets[i] as usize..self.char_offsets[i + 1] as usize]
    }
}

fn es_speller_oracle() -> Option<Arc<EsSpellerOracle>> {
    static CACHE: og_core::releasable::Releasable<EsSpellerOracle> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = crate::es::es_resource_dir()?;
        let dictionary = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "spanish")?;
        let mut surfaces: Vec<String> = match dictionary.fsa_word_list() {
            Some(words) => words,
            None => dictionary.keys().map(str::to_string).collect(),
        };
        surfaces.sort_unstable();
        let mut char_offsets: Vec<u32> = Vec::with_capacity(surfaces.len() + 1);
        let mut chars: Vec<char> = Vec::new();
        for surface in &surfaces {
            char_offsets.push(chars.len() as u32);
            chars.extend(surface.chars());
        }
        char_offsets.push(chars.len() as u32);
        let mut dict = og_spell::Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(Arc::clone(&dictionary)),
        ));
        for name in ["hunspell/spelling.txt", "multiwords.txt"] {
            if let Ok(text) = std::fs::read_to_string(resource_dir.join(name)) {
                for line in text.lines() {
                    let word = line.split(['\t', ';']).next().unwrap_or("").trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }
        }
        if let Some(core_dir) = crate::engine::find_lt_core_resource_dir()
            && let Ok(text) = std::fs::read_to_string(core_dir.join("spelling_global.txt"))
        {
            for line in text.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        Some(Arc::new(EsSpellerOracle {
            surfaces,
            char_offsets,
            chars,
            dictionary: dict,
        }))
    })
}

pub(crate) fn nfd_base(c: char) -> char {
    match c {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'Ç' => 'C',
        'ç' => 'c',
        'Ñ' => 'N',
        'ñ' => 'n',
        'Ý' | 'Ÿ' => 'Y',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

fn are_equal(x: char, y: char) -> bool {
    if x == y {
        return true;
    }
    let xb = nfd_base(x);
    let yb = nfd_base(y);
    if xb == yb {
        return true;
    }
    if xb.is_alphabetic() && xb.is_lowercase() != yb.is_lowercase() {
        let (Some(xl), Some(yl)) = (xb.to_lowercase().next(), yb.to_lowercase().next()) else {
            return false;
        };
        return xl == yl;
    }
    false
}

fn quasi_one_edit(shorter: &[char], longer: &[char]) -> bool {
    let mut i = 0;
    while i < shorter.len() && are_equal(shorter[i], longer[i]) {
        i += 1;
    }
    for j in i..shorter.len() {
        if !are_equal(shorter[j], longer[j + 1]) {
            return false;
        }
    }
    true
}

fn quasi_distance_at_most_one(a: &[char], b: &[char]) -> Option<usize> {
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => {
            let mut first_diff = None;
            let mut diff_count = 0usize;
            for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                if !are_equal(*x, *y) {
                    if first_diff.is_none() {
                        first_diff = Some(i);
                    }
                    diff_count += 1;
                    if diff_count > 2 {
                        return None;
                    }
                }
            }
            match diff_count {
                0 => Some(0),
                1 => Some(1),
                _ => {
                    let i = first_diff?;
                    if i + 1 < a.len() && a[i] == b[i + 1] && a[i + 1] == b[i] {
                        Some(1)
                    } else {
                        None
                    }
                }
            }
        }
        std::cmp::Ordering::Less => {
            if b.len() - a.len() == 1 && quasi_one_edit(a, b) {
                Some(1)
            } else {
                None
            }
        }
        std::cmp::Ordering::Greater => {
            if a.len() - b.len() == 1 && quasi_one_edit(b, a) {
                Some(1)
            } else {
                None
            }
        }
    }
}

pub fn find_similar_words(word: &str) -> Vec<String> {
    let Some(oracle) = es_speller_oracle() else {
        return Vec::new();
    };
    let word_chars: Vec<char> = word.chars().collect();
    let len = word_chars.len();
    if len == 0 || len >= 120 {
        return Vec::new();
    }
    let max_distance = usize::from(len > 1);
    let mut exact: Vec<&str> = Vec::new();
    let mut near: Vec<&str> = Vec::new();
    for (i, candidate) in oracle.surfaces.iter().enumerate() {
        let candidate_chars = oracle.candidate_chars(i);
        if candidate_chars.len() + 1 < len {
            continue;
        }
        if candidate.as_str() == word {
            continue;
        }
        if len + 1 < candidate_chars.len() {
            continue;
        }
        match quasi_distance_at_most_one(&word_chars, candidate_chars) {
            Some(0) => exact.push(candidate),
            Some(1) if max_distance == 1 => near.push(candidate),
            _ => {}
        }
    }
    exact.extend(near);
    exact.into_iter().map(str::to_string).collect()
}

fn is_all_uppercase(chars: &[char]) -> bool {
    chars
        .iter()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn is_not_all_lowercase(chars: &[char]) -> bool {
    chars.iter().any(|c| c.is_alphabetic() && !c.is_lowercase())
}

fn is_not_capitalized_word(chars: &[char]) -> bool {
    match chars.first() {
        Some(&first) if first.is_uppercase() => chars[1..]
            .iter()
            .any(|c| c.is_alphabetic() && !c.is_lowercase()),
        _ => true,
    }
}

fn is_mixed_case(chars: &[char]) -> bool {
    !is_all_uppercase(chars) && is_not_capitalized_word(chars) && is_not_all_lowercase(chars)
}

fn is_camel_case(chars: &[char]) -> bool {
    !chars.is_empty()
        && !is_all_uppercase(chars)
        && is_not_capitalized_word(chars)
        && chars[0].is_uppercase()
        && (chars.len() <= 1 || chars[1].is_lowercase())
        && is_not_all_lowercase(chars)
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn is_misspelled(word: &str) -> bool {
    let Some(oracle) = es_speller_oracle() else {
        return false;
    };
    let chars: Vec<char> = word.chars().collect();
    if chars.is_empty() {
        return false;
    }
    if chars.len() == 1 && !chars[0].is_alphabetic() {
        return false;
    }
    if chars.iter().any(|c| c.is_numeric()) {
        return false;
    }
    if is_camel_case(&chars) || is_all_uppercase(&chars) {
        return false;
    }
    if oracle.dictionary.contains_exact(word) {
        return false;
    }
    if !is_mixed_case(&chars) {
        let lower = word.to_lowercase();
        if oracle.dictionary.contains_exact(&lower) {
            return false;
        }
        if is_all_uppercase(&chars)
            && oracle
                .dictionary
                .contains_exact(&uppercase_first_char(&lower))
        {
            return false;
        }
    }
    true
}

pub fn replacement_is_misspelled(value: &str) -> bool {
    let tokenizer = shared_es_tokenizer();
    let tokens: Vec<String> = match tokenizer {
        Some(t) => t.tokenize_strings(value),
        None => value.split_whitespace().map(str::to_string).collect(),
    };
    tokens.iter().any(|t| is_misspelled(t))
}

fn shared_es_tokenizer() -> Option<Arc<crate::es::word_tokenizer::SpanishWordTokenizer>> {
    static CACHE: og_core::releasable::Releasable<crate::es::word_tokenizer::SpanishWordTokenizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        crate::es::es_resource_dir()
            .map(|_| crate::es::word_tokenizer::SpanishWordTokenizer::new())
            .map(Arc::new)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_similar_words_resolves_the_apostrophe_examples() {
        let mut words = find_similar_words("Est`n");
        assert!(
            words.contains(&"están".to_string()) && words.contains(&"estén".to_string()),
            "quasi-distance must fold case and diacritics: {words:?}"
        );
        assert!(
            words.iter().all(|w| w != "Est`n"),
            "the query itself is never returned"
        );
        words = find_similar_words("A");
        assert!(
            words.iter().all(|w| w.chars().count() == 1),
            "one-char queries only accept distance zero: {words:?}"
        );
    }

    #[test]
    fn find_similar_words_orders_by_distance_then_lexicographically() {
        let words = find_similar_words("casá");
        assert!(
            words.first().is_some_and(|w| w == "casa"),
            "quasi-equal words come first: {words:?}"
        );
    }

    #[test]
    fn is_misspelled_ports_morfologik_default_flags() {
        assert!(!is_misspelled("maquina"));
        assert!(
            !is_misspelled("Buenos"),
            "convert-case accepts the lowercase form"
        );
        assert!(
            !is_misspelled("DÁ"),
            "all-uppercase words are never misspelled"
        );
        assert!(
            !is_misspelled("McDonald"),
            "camel case words are never misspelled"
        );
        assert!(
            !is_misspelled("a1b2"),
            "words with digits are never misspelled"
        );
        assert!(!is_misspelled("!"));
        assert!(!is_misspelled(""));
        assert!(is_misspelled("imporntate"));
        assert!(is_misspelled("asldkfg"));
        assert!(
            !is_misspelled("a fin de cuentas"),
            "multiwords.txt is in the plain-text dicts"
        );
    }

    #[test]
    fn replacements_are_misspelled_per_spanish_token() {
        assert!(!replacement_is_misspelled("cambió"));
        assert!(!replacement_is_misspelled("importante"));
        assert!(replacement_is_misspelled("imporntate"));
        assert!(replacement_is_misspelled("cambió imporntate"));
    }
}
