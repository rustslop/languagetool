use og_rules::native_rules::DoublePunctuationRule;

pub type GermanDoublePunctuationRule = DoublePunctuationRule;

pub fn german_double_punctuation_rule() -> DoublePunctuationRule {
    DoublePunctuationRule::new()
        .with_id("DE_DOUBLE_PUNCTUATION")
        .with_description("Zwei aufeinanderfolgende Kommas oder Punkte")
        .with_messages(
            "Zwei aufeinanderfolgende Punkte. Auch wenn ein Satz mit einer Abkürzung endet, \
             endet er nur mit einem Punkt (§103 Regelwerk).",
            "Zwei aufeinanderfolgende Punkte",
            "Zwei aufeinanderfolgende Kommas",
            "Zwei aufeinanderfolgende Kommas",
        )
}

#[cfg(test)]
mod tests {

    #[test]
    fn german_rule_flags_double_dots_after_abbreviation() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let result = engine.check("Sein Vater ist Regierungsrat a. D..");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_DOUBLE_PUNCTUATION"),
            "expected DE_DOUBLE_PUNCTUATION on 'a. D..', got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ellipsis_is_not_flagged() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let result = engine.check("Und dann\u{2026}");
        assert!(
            !result
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_DOUBLE_PUNCTUATION")
        );
    }

    #[test]
    fn domains_after_dot_is_not_flagged() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let result = engine.check("Die .com-Domains sind teuer.");
        assert!(
            !result
                .matches
                .iter()
                .any(|m| m.rule().id() == "COMMA_PARENTHESIS_WHITESPACE"),
            "German DOMAINS exception must apply, got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }
}
