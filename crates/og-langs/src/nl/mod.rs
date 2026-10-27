pub mod compound_acceptor;
pub mod preferred_word;
pub mod simple_replace;
pub mod space_in_compound;
pub mod speller;
pub mod tagger;
pub mod unpaired_brackets;

#[cfg(test)]
pub(crate) mod example_suite;
#[cfg(test)]
pub(crate) mod test_support;

use crate::en::{CompoundRule, WordCoherencyRule, WrongWordInContextRule};
use og_core::rule::{Rule, TextLevelRule};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

pub(crate) fn nl_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("nl/src/main/resources/org/languagetool/rules/nl"))
}

pub fn variant_grammar_path() -> Option<PathBuf> {
    nl_rules_dir().map(|dir| dir.join("nl-NL/grammar.xml"))
}

pub fn dutch_compound_rule() -> CompoundRule {
    CompoundRule::for_language(
        "NL_COMPOUNDS",
        "Woorden die aaneengeschreven horen, bijvoorbeeld 'zee-egel' i.p.v. 'zee egel': $match",
        "Dit woord hoort waarschijnlijk aaneengeschreven met een koppelteken.",
        "Dit woord hoort waarschijnlijk aaneengeschreven.",
        "Deze uitdrukking hoort mogelijk aan elkaar, eventueel met een koppelteken.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compounds.txt"
        ),
    )
}

pub fn dutch_wrong_word_in_context_rule() -> WrongWordInContextRule {
    WrongWordInContextRule::for_language(
        "DUTCH_WRONG_WORD_IN_CONTEXT",
        "Woordverwarring: $match",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/rules/nl/wrongWordInContext.txt"
        ),
        "Mogelijk verwarring",
        "Mogelijk verwarring: Bedoelde u <suggestion>$SUGGESTION</suggestion> (= $EXPLANATION_SUGGESTION) i.p.v. '$WRONGWORD' (= $EXPLANATION_WRONGWORD)?",
    )
}

pub fn dutch_word_coherency_rule() -> WordCoherencyRule {
    WordCoherencyRule::for_language(
        "NL_WORD_COHERENCY",
        "Consistente spelling van woorden met meerdere correcte vormen.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/rules/nl/coherency.txt"
        ),
    )
    .with_message_template(
        "Gebruik liever niet '{word1}' en '{word2}' door elkaar in een tekst.",
    )
}

pub use unpaired_brackets::dutch_unpaired_brackets_rule;

pub struct DutchNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn dutch_native_rules() -> DutchNativeRules {
    DutchNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(speller::DutchSpellerRule::new()),
            Arc::new(dutch_compound_rule()),
            Arc::new(dutch_wrong_word_in_context_rule()),
            Arc::new(simple_replace::dutch_simple_replace_rule()),
            Arc::new(preferred_word::preferred_word_rule()),
            Arc::new(space_in_compound::space_in_compound_rule()),
            Arc::new(simple_replace::dutch_check_case_rule()),
        ],
        text_level_rules: vec![
            Arc::new(dutch_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(40)),
            Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
            Arc::new(dutch_word_coherency_rule()),
        ],
    }
}

pub fn id_to_priority() -> &'static HashMap<&'static str, i32> {
    static MAP: OnceLock<HashMap<&'static str, i32>> = OnceLock::new();
    MAP.get_or_init(|| {
        HashMap::from([
            ("TOO_LONG_SENTENCE", -1),
            ("SINT_X", 3),
            ("ET_AL", 1),
            ("N_PERSOONS", 1),
            ("HOOFDLETTERS_OVERBODIG_A", 1),
            ("VERSCHILLENDE_AANHALINGSTEKENS", 1),
            ("STAM_ZONDER_IK", -1),
            ("KOMMA_ONTBR", -1),
            ("KOMMA_KOMMA", -1),
            ("HET_FIETS", -2),
            ("JIJ_JOU_JOUW", -2),
            ("JOU_JOUW", -3),
            ("BE", -3),
            ("DOUBLE_PUNCTUATION", -3),
            ("EINDE_ZIN_ONVERWACHT", -5),
            ("TOO_LONG_PARAGRAPH", -15),
            ("ERG_LANG_WOORD", -20),
            ("DE_ONVERWACHT", -20),
        ])
    })
}

pub fn dutch_priority_for_id(id: &str) -> i32 {
    if id.starts_with("NL_SIMPLE_REPLACE") || id.starts_with("NL_SPACE_IN_COMPOUND") {
        return 1;
    }
    if let Some(priority) = id_to_priority().get(id) {
        return *priority;
    }
    if id.starts_with("AI_NL_HYDRA_LEO") {
        if id.starts_with("AI_NL_HYDRA_LEO_MISSING_COMMA") {
            return -51;
        }
        return -5;
    }
    if id.eq_ignore_ascii_case("TOO_LONG_SENTENCE") {
        return -101;
    }
    if id == "REPETITIONS_STYLE" {
        return -55;
    }
    if id.contains("STYLE") {
        return -50;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rules_carry_java_ids() {
        let native = dutch_native_rules();
        let ids: Vec<&str> = native.rules.iter().map(|r| r.id()).collect();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
            "MORFOLOGIK_RULE_NL_NL",
            "NL_COMPOUNDS",
            "DUTCH_WRONG_WORD_IN_CONTEXT",
            "NL_SIMPLE_REPLACE",
            "NL_PREFERRED_WORD_RULE",
            "NL_SPACE_IN_COMPOUND",
            "NL_CHECKCASE",
        ] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
        let text_ids: Vec<&str> = native.text_level_rules.iter().map(|r| r.id()).collect();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "TOO_LONG_SENTENCE",
            "TOO_LONG_PARAGRAPH",
            "SENTENCE_WHITESPACE",
            "NL_WORD_COHERENCY",
        ] {
            assert!(text_ids.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn java_pins_priority_map() {
        assert_eq!(dutch_priority_for_id("SINT_X"), 3);
        assert_eq!(dutch_priority_for_id("ET_AL"), 1);
        assert_eq!(dutch_priority_for_id("JOU_JOUW"), -3);
        assert_eq!(dutch_priority_for_id("ERG_LANG_WOORD"), -20);
        assert_eq!(dutch_priority_for_id("DE_ONVERWACHT"), -20);
        assert_eq!(dutch_priority_for_id("TOO_LONG_PARAGRAPH"), -15);
        assert_eq!(dutch_priority_for_id("TOO_LONG_SENTENCE"), -1);
        assert_eq!(dutch_priority_for_id("NL_SIMPLE_REPLACE_KLAA"), 1);
        assert_eq!(dutch_priority_for_id("NL_SPACE_IN_COMPOUND_LANGE"), 1);
        assert_eq!(dutch_priority_for_id("AI_NL_HYDRA_LEO_X"), -5);
        assert_eq!(dutch_priority_for_id("AI_NL_HYDRA_LEO_MISSING_COMMA"), -51);
        assert_eq!(dutch_priority_for_id("SOMETHING_STYLE_CAT"), -50);
        assert_eq!(dutch_priority_for_id("UNKNOWN_RULE"), 0);
    }

    #[test]
    fn variant_grammar_file_resolves() {
        let path = variant_grammar_path().expect("nl rules dir");
        assert!(path.ends_with("nl-NL/grammar.xml"));
        assert!(path.exists(), "{path:?} must exist");
    }

    #[test]
    fn java_pins_compound_rule_examples() {
        let rule = dutch_compound_rule();
        let pipeline = crate::nl::test_support::DutchPipeline::shared();
        let sentence = |text: &str| pipeline.analyze_range(text, 0, text.len());
        assert!(
            rule.match_sentence(&sentence("Dit is een zee-egel."))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&sentence("Zee-egel is een woord."))
                .is_empty()
        );
        let matches = rule.match_sentence(&sentence("Dit is een zee egel."));
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "zee-egel")
        );
        let matches = rule.match_sentence(&sentence("Zee egel is een woord."));
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "Zee-egel")
        );
    }

    #[test]
    fn java_pins_word_coherency_example() {
        let rule = dutch_word_coherency_rule();
        let pipeline = crate::nl::test_support::DutchPipeline::shared();
        let sentences = pipeline.analyze_text("organogram, organigram");
        let text = "organogram, organigram";
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
    }
}
