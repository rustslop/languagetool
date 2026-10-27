pub mod advanced_word_repeat;
pub mod dash;
pub mod filler_words;
pub mod filters;
pub mod paragraph_repeat_beginning;
pub mod specific_case;
pub mod verb_conjugation;
pub mod word_tokenizer;
pub mod yo_speller;

pub use advanced_word_repeat::RussianWordRepeatRule;
pub use dash::RussianDashRule;
pub use filler_words::RussianFillerWordsRule;
pub use paragraph_repeat_beginning::russian_paragraph_repeat_beginning_rule;
pub use specific_case::russian_specific_case_rule;
pub use verb_conjugation::RussianVerbConjugationRule;
pub use yo_speller::RussianYoSpellerRule;

use crate::en::{CompoundRule, SimpleReplaceRule, WordCoherencyRule};
use og_rules::native_rules::WordRepeatRule;

pub fn russian_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new()
        .with_id("RU_UNPAIRED_BRACKETS")
        .with_symbols(
            vec!["(", "{", "„", "\"", "'", "“"],
            vec![")", "}", "“", "\"", "'", "”"],
        )
}

pub fn russian_word_coherency_rule() -> WordCoherencyRule {
    WordCoherencyRule::for_language(
        "RU_WORD_COHERENCY",
        "Единообразное написание слов с более чем одним допустимым написанием",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/rules/ru/coherency.txt"
        ),
    )
    .with_message_template("«{word1}» и «{word2}» не следует использовать одновременно")
}

pub fn russian_word_root_repeat_rule() -> WordCoherencyRule {
    WordCoherencyRule::for_language(
        "RU_WORD_ROOT_REPEAT",
        "Повтор однокоренных слов",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/rules/ru/wordrootrep.txt"
        ),
    )
    .with_message_template("«{word1}» и «{word2}» – однокоренные слова, их не стоит использовать одновременно")
    .with_default_off()
}

pub fn russian_simple_replace_rule() -> SimpleReplaceRule {
    let mut rule = SimpleReplaceRule::new(
        "RU_SIMPLE_REPLACE",
        "Поиск просторечий и ошибочных фраз",
        "MISC",
        "Общие правила",
    );
    rule = rule.with_message_template("«$match» — просторечие, исправление: $suggestions", ", ");
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/rules/ru/replace.txt"
    ));
    rule
}

pub fn russian_compound_rule() -> CompoundRule {
    CompoundRule::for_language(
        "RU_COMPOUNDS",
        "Правописание через дефис",
        "Эти слова должны быть написаны через дефис.",
        "Эти слова должны быть написаны слитно.",
        "Эти слова могут быть написаны через дефис или слитно.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru/compounds.txt"
        ),
    )
}

pub fn russian_simple_word_repeat_rule() -> WordRepeatRule {
    WordRepeatRule::new()
        .with_ignored_words(&["и", "по", "что"])
        .with_ignore_single_letter_repeats()
}

// Java's switch matches the constant names RUSSIAN_SIMPLE_REPLACE_RULE and
// RUSSIAN_SPECIFIC_CASE, never their values RU_SIMPLE_REPLACE/RU_SPECIFIC_CASE.
pub fn russian_priority_for_id(id: &str) -> Option<i32> {
    match id {
        "RU_DASH_RULE" => Some(12),
        "RU_COMPOUNDS" => Some(11),
        "RUSSIAN_SIMPLE_REPLACE_RULE" => Some(10),
        "RUSSIAN_SPECIFIC_CASE" => Some(9),
        "MORFOLOGIC_RULE_RU_RU_YO" => Some(2),
        "MORFOLOGIC_RULE_RU_RU" => Some(1),
        "Word_root_repeat" => Some(-1),
        "PUNCT_DPT_2" => Some(-2),
        "TOO_LONG_PARAGRAPH" => Some(-15),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::{Rule as _, TextLevelRule as _};

    fn sentence_from_words(words: &[&str]) -> og_core::AnalyzedSentence {
        let text = words.join(" ");
        let mut sentence = og_core::AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![og_core::AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for word in words {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new(*word, start, end),
            ));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn russian_priority_table_matches_java() {
        assert_eq!(russian_priority_for_id("RU_DASH_RULE"), Some(12));
        assert_eq!(russian_priority_for_id("RU_COMPOUNDS"), Some(11));
        assert_eq!(russian_priority_for_id("MORFOLOGIC_RULE_RU_RU_YO"), Some(2));
        assert_eq!(russian_priority_for_id("MORFOLOGIC_RULE_RU_RU"), Some(1));
        assert_eq!(russian_priority_for_id("Word_root_repeat"), Some(-1));
        assert_eq!(russian_priority_for_id("PUNCT_DPT_2"), Some(-2));
        assert_eq!(russian_priority_for_id("TOO_LONG_PARAGRAPH"), Some(-15));
        assert_eq!(russian_priority_for_id("RU_SIMPLE_REPLACE"), None);
        assert_eq!(russian_priority_for_id("RU_SPECIFIC_CASE"), None);
        assert_eq!(russian_priority_for_id("SOMETHING_ELSE"), None);
    }

    #[test]
    fn unpaired_brackets_fires_on_the_java_embedded_example() {
        let rule = russian_unpaired_brackets_rule();
        let text = "Самоотверженный поступок Оленина (подарок Лукашке коня вызывает лишь удивление и усиливает недоверие к нему станичников.";
        let matches = rule.match_text(text, &[]);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "("),
            "expected the unpaired ( to be flagged"
        );
    }

    #[test]
    fn simple_replace_suggests_on_the_java_embedded_example() {
        let rule = russian_simple_replace_rule();
        let sentence = sentence_from_words(&["Экспрессо", "–", "крепкий", "кофе."]);
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "эспрессо")),
            "expected the экспрессо=эспрессо suggestion from replace.txt:309"
        );
    }

    #[test]
    fn compound_rule_fires_on_the_java_embedded_example() {
        let rule = russian_compound_rule();
        let sentence =
            sentence_from_words(&["Собрание", "состоится", "в", "конференц", "зале", "."]);
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.iter().any(|m| m
                .replacements()
                .iter()
                .any(|r| r.value().contains("конференц-зал"))),
            "expected the конференц-зале suggestion"
        );
    }

    #[test]
    fn russian_parameterized_natives_use_java_ids() {
        assert_eq!(
            russian_unpaired_brackets_rule().id(),
            "RU_UNPAIRED_BRACKETS"
        );
        assert_eq!(russian_word_coherency_rule().id(), "RU_WORD_COHERENCY");
        assert_eq!(russian_word_root_repeat_rule().id(), "RU_WORD_ROOT_REPEAT");
        assert_eq!(russian_simple_replace_rule().id(), "RU_SIMPLE_REPLACE");
        assert_eq!(russian_compound_rule().id(), "RU_COMPOUNDS");
        assert_eq!(russian_simple_word_repeat_rule().id(), "WORD_REPEAT_RULE");
        assert_eq!(RussianWordRepeatRule::new().id(), "RU_WORD_REPEAT");
        assert_eq!(
            RussianVerbConjugationRule::new().id(),
            "RU_VERB_CONJUGATION"
        );
        assert_eq!(russian_specific_case_rule().id(), "RU_SPECIFIC_CASE");
        assert_eq!(RussianDashRule::new().id(), "RU_DASH_RULE");
        assert_eq!(RussianFillerWordsRule::new().id(), "FILLER_WORDS_RU");
    }

    #[test]
    fn russian_word_coherency_flags_mixed_spelling() {
        let rule = russian_word_coherency_rule();
        let mk = |text: &str, start: usize| {
            let mut sentence = og_core::AnalyzedSentence::new(text, start, start + text.len());
            let mut tokens = Vec::new();
            let mut offset = 0;
            for part in text.split(' ') {
                if part.is_empty() {
                    continue;
                }
                let s = offset + text[offset..].find(part).unwrap_or(0);
                let e = s + part.len();
                offset = e;
                tokens.push(og_core::AnalyzedTokenReadings::new(
                    og_core::AnalyzedToken::new(part, start + s, start + e),
                ));
            }
            sentence.set_tokens(tokens);
            sentence
        };
        let data = filters::ru_resource_dir()
            .and_then(|dir| {
                let path = dir
                    .join("../../rules/ru/coherency.txt")
                    .canonicalize()
                    .ok()?;
                std::fs::read_to_string(path).ok()
            })
            .unwrap_or_default();
        let pair = data
            .lines()
            .find(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
            .and_then(|l| l.trim().split_once(';'));
        let Some((a, b)) = pair else {
            panic!("coherency.txt must resolve with at least one data pair");
        };
        let s1 = mk(a.trim(), 0);
        let s2 = mk(b, a.len() + 1);
        let text = format!("{} {}", a.trim(), b);
        let matches = rule.match_text(&text, &[s1, s2]);
        assert!(!matches.is_empty(), "second variant must be flagged");
    }

    #[test]
    fn engine_check_survives_russian_text_with_brackets() {
        let _release_guard = crate::release_guard();
        let engine = crate::engine::LanguageEngine::for_code("ru").unwrap();
        let result = engine.check("Привет мир (тест).");
        assert!(!result.matches.is_empty());
    }
}
