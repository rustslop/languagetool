pub fn russian_paragraph_repeat_beginning_rule()
-> crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule {
    crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule::new(
        "Повтор начала абзаца",
        "Повтор начала последнего абзаца",
        "Стиль",
    )
}

#[cfg(test)]
mod tests {
    use og_core::rule::TextLevelRule as _;

    #[test]
    fn flags_repeated_paragraph_beginning_with_java_id_and_ru_messages() {
        let _release_guard = crate::release_guard();
        let rule = super::russian_paragraph_repeat_beginning_rule();
        assert_eq!(rule.id(), "PARAGRAPH_REPEAT_BEGINNING_RULE");
        assert!(!rule.is_default_on());
        assert_eq!(rule.description(), "Повтор начала абзаца");

        let engine = crate::engine::LanguageEngine::shared_for_code("ru").expect("ru engine");
        let text = "Собака бежит быстро.\n\nСобака лает громко.";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["PARAGRAPH_REPEAT_BEGINNING_RULE".to_string()];
        let matches = engine.check_with_request(&request);
        let hits: Vec<&og_core::RuleMatch> = matches
            .matches
            .iter()
            .filter(|m| m.rule().id() == "PARAGRAPH_REPEAT_BEGINNING_RULE")
            .collect();
        assert_eq!(hits.len(), 2, "one match per paragraph, got {hits:?}");
        assert!(
            hits.iter()
                .all(|m| m.message() == "Повтор начала последнего абзаца")
        );

        let distinct = "Собака бежит быстро.\n\nКошка спит долго.";
        let mut request = og_core::CheckRequest::new(distinct, engine.language().clone());
        request.enabled_rules = vec!["PARAGRAPH_REPEAT_BEGINNING_RULE".to_string()];
        assert!(
            engine
                .check_with_request(&request)
                .matches
                .iter()
                .all(|m| m.rule().id() != "PARAGRAPH_REPEAT_BEGINNING_RULE")
        );
    }
}
