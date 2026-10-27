use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub mod speller;

#[cfg(test)]
mod example_suite;

pub use speller::{DanishSpellerRule, HUNSPELL_RULE_DA};

pub struct DanishNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn danish_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "”"],
        vec!["]", ")", "}", "\"", "”"],
    )
}

pub fn danish_native_rules() -> DanishNativeRules {
    DanishNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(DanishSpellerRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(danish_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speller_matches(text: &str) -> Vec<(String, String)> {
        let rule = DanishSpellerRule::new();
        let sentence = example_suite::danish_pipeline().analyze_range(text, 0, text.len());
        rule.match_sentence(&sentence)
            .into_iter()
            .map(|m| {
                let span = &sentence.text()[m.offset()..m.offset() + m.length()];
                (span.to_string(), m.message().to_string())
            })
            .collect()
    }

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = danish_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "HUNSPELL_RULE",
                "UPPERCASE_SENTENCE_START",
            ]
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_level_ids, vec!["UNPAIRED_BRACKETS", "WHITESPACE_RULE"]);
    }

    #[test]
    fn speller_rule_keeps_the_java_dash_run_clean() {
        assert!(
            speller_matches("De står under ----").is_empty(),
            "DanishSpellerRuleTest.java:35 expects zero matches on 'De står under ----'"
        );
    }

    #[test]
    fn speller_rule_accepts_everyday_words_the_morfologik_dict_lacks() {
        for text in [
            "Indsæt din egen tekst her.",
            "Denne tekst kan man se nogle fejl i.",
            "De står under ----",
        ] {
            assert!(
                speller_matches(text).is_empty(),
                "'{text}' must stay unflagged by the hunspell-backed rule"
            );
        }
    }

    #[test]
    fn speller_rule_fires_on_a_danish_typo() {
        let matches = speller_matches("Dette er en perfect sætning.");
        assert_eq!(
            matches,
            vec![(
                "perfect".to_string(),
                "Possible spelling mistake found: 'perfect'".to_string()
            )],
            "the DanishTest demo word 'perfect' is unknown to both da dictionaries"
        );
    }

    #[test]
    fn speller_residue_stems_from_unparsed_numeric_hunspell_flags() {
        let matches = speller_matches(" vær opmærksom på at få standartfejl frem i lyset. ");
        let flagged: Vec<&str> = matches.iter().map(|(w, _)| w.as_str()).collect();
        assert_eq!(
            flagged,
            vec!["opmærksom", "standartfejl"],
            "da_DK.aff declares FLAG num; og-spell hunspell.rs reads flags as raw chars, so \
             flagged stems (opmærksom/1025,366,976) and flag-gated compounds (standartfejl) \
             stay unknown to the hunspell stage and to danish.dict"
        );
    }
}
