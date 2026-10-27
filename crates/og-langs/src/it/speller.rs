pub fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

pub fn order_suggestions(suggestions: &[String], word: &str) -> Vec<String> {
    if is_capitalized_word(word) {
        return suggestions.to_vec();
    }
    suggestions
        .iter()
        .filter(|s| !(is_capitalized_word(s) && suggestions.contains(&s.to_lowercase())))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_capitalized_duplicate_when_word_is_lowercase() {
        let suggestions = vec![
            "empio".to_string(),
            "Esempio".to_string(),
            "esempio".to_string(),
            "espi\u{f2}".to_string(),
        ];
        assert_eq!(
            order_suggestions(&suggestions, "esmpio"),
            vec![
                "empio".to_string(),
                "esempio".to_string(),
                "espi\u{f2}".to_string()
            ]
        );
    }

    #[test]
    fn keeps_everything_for_capitalized_word() {
        let suggestions = vec!["Esempio".to_string(), "esempio".to_string()];
        assert_eq!(
            order_suggestions(&suggestions, "Esmpio"),
            vec!["Esempio".to_string(), "esempio".to_string()]
        );
    }

    #[test]
    fn keeps_capitalized_suggestion_without_lowercase_sibling() {
        let suggestions = vec!["Empio".to_string()];
        assert_eq!(order_suggestions(&suggestions, "esmpio"), suggestions);
    }

    #[test]
    fn keeps_uppercase_acronym_like_suggestions() {
        let suggestions = vec!["CBA".to_string(), "cba".to_string()];
        assert_eq!(
            order_suggestions(&suggestions, "esmpio"),
            vec!["CBA".to_string(), "cba".to_string()]
        );
    }

    #[test]
    fn capitalized_word_semantics_match_java() {
        assert!(is_capitalized_word("Esempio"));
        assert!(is_capitalized_word("\u{c8}"));
        assert!(!is_capitalized_word("esempio"));
        assert!(!is_capitalized_word("ESempio"));
        assert!(!is_capitalized_word("ESEMPIO"));
        assert!(!is_capitalized_word(""));
        assert!(is_capitalized_word("E1"));
    }
}
