use og_core::{AnalyzedSentence, AnalyzedTokenReadings};

pub fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn lowercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

pub fn is_all_uppercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| !c.is_lowercase())
}

pub fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_uppercase())
        && chars.all(|c| !c.is_uppercase())
        && s.chars().any(|c| c.is_alphabetic())
}

pub fn has_pos_tag_starting_with(token: &AnalyzedTokenReadings, prefix: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

pub fn matches_pos_tag_regex(token: &AnalyzedTokenReadings, pattern: &str) -> bool {
    token.has_pos_tag_matching(pattern)
}

pub fn is_tagged(token: &AnalyzedTokenReadings) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| !t.is_empty()))
}

pub fn is_pos_tag_unknown(token: &AnalyzedTokenReadings) -> bool {
    !is_tagged(token)
}

pub fn has_same_lemmas(token: &AnalyzedTokenReadings) -> bool {
    let mut lemmas = token.readings().iter().map(|r| r.lemma());
    let first = lemmas.next().flatten();
    first.is_some_and(|first| lemmas.all(|l| l == Some(first)))
}

pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    let mut a_chars = a.chars().flat_map(char::to_lowercase);
    let mut b_chars = b.chars().flat_map(char::to_lowercase);
    loop {
        match (a_chars.next(), b_chars.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => continue,
            _ => return false,
        }
    }
}

pub fn levenshtein_at_most_one(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let mut i = 0;
    let mut j = 0;
    let mut diff = 0;
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else {
            diff += 1;
            if diff > 1 {
                return false;
            }
            if a.len() < b.len() {
                j += 1;
            } else if a.len() > b.len() {
                i += 1;
            } else {
                i += 1;
                j += 1;
            }
        }
    }
    diff + (a.len() - i) + (b.len() - j) <= 1
}

pub fn is_paragraph_end(sentences: &[AnalyzedSentence], n: usize) -> bool {
    if sentences.is_empty() || n >= sentences.len() - 1 {
        return true;
    }
    let text = sentences[n].text();
    if text.ends_with("\n\n")
        || text.ends_with("\n\r\n\r")
        || text.ends_with("\r\n\r\n")
        || sentences[n + 1].text().starts_with('\n')
        || sentences[n + 1].text().starts_with("\r\n")
    {
        return true;
    }
    false
}

pub fn has_no_whitespace_before(
    sentence: &AnalyzedSentence,
    token: &AnalyzedTokenReadings,
) -> bool {
    let rel = token.token().start().saturating_sub(sentence.start());
    match sentence
        .text()
        .get(..rel)
        .and_then(|s| s.chars().next_back())
    {
        Some(c) => !c.is_whitespace(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{sentence, token_with_pos};
    use og_core::AnalyzedToken;

    #[test]
    fn case_conversion_handles_first_char_only() {
        assert_eq!(uppercase_first_char("straße"), "Straße");
        assert_eq!(uppercase_first_char("über"), "Über");
        assert_eq!(uppercase_first_char("ß"), "SS");
        assert_eq!(uppercase_first_char(""), "");
        assert_eq!(lowercase_first_char("ÜBER"), "üBER");
        assert_eq!(lowercase_first_char("İstanbul"), "i̇stanbul");
        assert_eq!(lowercase_first_char(""), "");
    }

    #[test]
    fn uppercase_predicates_split_the_four_java_cases() {
        assert!(starts_with_uppercase("Über"));
        assert!(!starts_with_uppercase("über"));
        assert!(!starts_with_uppercase(""));
        assert!(is_all_uppercase("ABC"));
        assert!(is_all_uppercase("ABC 123"));
        assert!(!is_all_uppercase(""));
        assert!(!is_all_uppercase("AbC"));
        assert!(is_capitalized_word("Word"));
        assert!(is_capitalized_word("Ä"));
        assert!(!is_capitalized_word("WORD"));
        assert!(!is_capitalized_word("word"));
        assert!(!is_capitalized_word("123"));
    }

    #[test]
    fn pos_tag_helpers_read_all_readings() {
        let tagged = token_with_pos("der", 0, 3, &["ART", "DT:DEF"]);
        assert!(has_pos_tag_starting_with(&tagged, "DT"));
        assert!(!has_pos_tag_starting_with(&tagged, "NN"));
        assert!(matches_pos_tag_regex(&tagged, "DT.*"));
        assert!(is_tagged(&tagged));
        assert!(!is_pos_tag_unknown(&tagged));
        let untagged = token_with_pos("der", 0, 3, &[]);
        assert!(!is_tagged(&untagged));
        assert!(is_pos_tag_unknown(&untagged));
    }

    #[test]
    fn has_same_lemmas_requires_a_shared_lemma_across_readings() {
        let token =
            AnalyzedTokenReadings::new(AnalyzedToken::new("saw", 0, 3)).with_readings(vec![
                AnalyzedToken::new("see", 0, 3).with_lemma("see"),
                AnalyzedToken::new("saw", 0, 3).with_lemma("see"),
            ]);
        assert!(has_same_lemmas(&token));
        let differing =
            AnalyzedTokenReadings::new(AnalyzedToken::new("saw", 0, 3)).with_readings(vec![
                AnalyzedToken::new("see", 0, 3).with_lemma("see"),
                AnalyzedToken::new("saw", 0, 3).with_lemma("saw"),
            ]);
        assert!(!has_same_lemmas(&differing));
        let lemmaless = AnalyzedTokenReadings::new(AnalyzedToken::new("saw", 0, 3))
            .with_readings(vec![AnalyzedToken::new("see", 0, 3)]);
        assert!(!has_same_lemmas(&lemmaless));
    }

    #[test]
    fn eq_ignore_case_folds_per_char_like_java() {
        assert!(eq_ignore_case("Straße", "STRASSE") == false);
        assert!(eq_ignore_case("Hallo", "halLO"));
        assert!(eq_ignore_case("", ""));
        assert!(!eq_ignore_case("hallo", "hall"));
    }

    #[test]
    fn levenshtein_at_most_one_covers_substitution_insert_delete() {
        assert!(levenshtein_at_most_one("abc", "abc"));
        assert!(levenshtein_at_most_one("abc", "abd"));
        assert!(levenshtein_at_most_one("abc", "abdc"));
        assert!(levenshtein_at_most_one("abc", "ac"));
        assert!(levenshtein_at_most_one("", "a"));
        assert!(levenshtein_at_most_one("a", ""));
        assert!(!levenshtein_at_most_one("abc", "abcde"));
        assert!(!levenshtein_at_most_one("ab", "ba"));
        assert!(!levenshtein_at_most_one("abc", "axy"));
    }

    #[test]
    fn is_paragraph_end_follows_the_java_line_break_matrix() {
        let empty: Vec<AnalyzedSentence> = Vec::new();
        assert!(is_paragraph_end(&empty, 0));
        let single = vec![sentence("Ein Satz.")];
        assert!(is_paragraph_end(&single, 0));
        let pair = vec![sentence("A."), sentence("B.")];
        assert!(!is_paragraph_end(&pair, 0));
        assert!(is_paragraph_end(&pair, 1));
        let double_nl = vec![sentence("A.\n\n"), sentence("B.")];
        assert!(is_paragraph_end(&double_nl, 0));
        let nl_crnl_cr = vec![sentence("A.\n\r\n\r"), sentence("B.")];
        assert!(is_paragraph_end(&nl_crnl_cr, 0));
        let crlf_crlf = vec![sentence("A.\r\n\r\n"), sentence("B.")];
        assert!(is_paragraph_end(&crlf_crlf, 0));
        let leading_nl = vec![sentence("A."), sentence("\nB.")];
        assert!(is_paragraph_end(&leading_nl, 0));
        let leading_crlf = vec![sentence("A."), sentence("\r\nB.")];
        assert!(is_paragraph_end(&leading_crlf, 0));
    }

    #[test]
    fn has_no_whitespace_before_looks_at_the_char_before_the_token() {
        let s = sentence("hallo Welt");
        let tokens: Vec<_> = s.non_whitespace_tokens().to_vec();
        assert!(!has_no_whitespace_before(&s, &tokens[1]));
        assert!(!has_no_whitespace_before(&s, &tokens[0]));
        let tight = sentence("A:B");
        let tight_tokens: Vec<_> = tight.non_whitespace_tokens().to_vec();
        assert!(has_no_whitespace_before(&tight, &tight_tokens[2]));
    }
}
