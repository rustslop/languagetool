pub fn russian_specific_case_rule() -> crate::specific_case::SpecificCaseRule {
    crate::specific_case::SpecificCaseRule::from_data(
        "RU_SPECIFIC_CASE",
        "Написание специальных наименований в верхнем или нижнем регистре",
        "Capitalization",
        "Для специальных наименований используйте начальную заглавную букву.",
        "Для специальных наименований используйте предложенное написание заглавных и строчных букв.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru/specific_case.txt"
        ),
    )
}

#[cfg(test)]
mod tests {
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, rule::Rule as _};

    fn sentence(words: &[&str]) -> AnalyzedSentence {
        let text = words.join(" ");
        let mut s = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for word in words {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *word, start, end,
            )));
        }
        s.set_tokens(tokens);
        s
    }

    fn rule(data: &str) -> crate::specific_case::SpecificCaseRule {
        crate::specific_case::SpecificCaseRule::from_data(
            "RU_SPECIFIC_CASE",
            "d",
            "Capitalization",
            "initial",
            "other",
            data,
        )
    }

    #[test]
    fn loads_java_phrases_and_suggests_proper_spelling() {
        let rule = rule("Рытый Банк\nвитамин C\n");
        let matches = rule.match_sentence(&sentence(&["Река", "рытый", "банк"]));
        assert_eq!(matches.len(), 1, "lowercase phrase must be flagged");
        assert_eq!(matches[0].replacements()[0].value(), "Рытый Банк");
    }

    #[test]
    fn skips_sentence_start_lowercase_suggestion() {
        let rule = rule("витамин C\n");
        let matches = rule.match_sentence(&sentence(&["витамин", "c"]));
        assert!(
            matches.is_empty(),
            "lowercase-start suggestion at sentence start must be skipped"
        );
    }
}
