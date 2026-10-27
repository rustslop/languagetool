pub mod comma_whitespace;
pub mod inflected_one_word;
pub mod replace_rules;
pub mod trans_verb;
pub mod word_coherency;
pub mod word_repeat;

#[cfg(test)]
mod example_suite;

pub use comma_whitespace::{
    ArabicCommaWhitespaceRule, arabic_base_comma_whitespace_rule, arabic_comma_whitespace_rule,
    arabic_question_mark_whitespace_rule, arabic_semicolon_whitespace_rule,
};
pub use inflected_one_word::ArabicInflectedOneWordReplaceRule;
pub use replace_rules::ArabicSimpleReplaceRule2;
pub use trans_verb::ArabicTransVerbRule;
pub use word_coherency::ArabicWordCoherencyRule;
pub use word_repeat::ArabicWordRepeatRule;

use og_core::{Category, IssueType};
use og_rules::native_rules::DoublePunctuationRule;

pub fn arabic_double_punctuation_rule() -> DoublePunctuationRule {
    DoublePunctuationRule::new()
        .with_id("ARABIC_DOUBLE_PUNCTUATION")
        .with_comma_character('،')
}

struct ReplaceRuleSpec<'a> {
    id: &'a str,
    name: &'a str,
    short: &'a str,
    message: &'a str,
    separator: &'a str,
    data: &'a str,
    category: (&'a str, &'a str),
    issue_type: IssueType,
}

fn replace_rule(spec: ReplaceRuleSpec<'_>) -> ArabicSimpleReplaceRule2 {
    let ReplaceRuleSpec {
        id,
        name,
        short,
        message,
        separator,
        data,
        category,
        issue_type,
    } = spec;
    let mut rule = ArabicSimpleReplaceRule2::new(
        id,
        name,
        short,
        message,
        separator,
        Category::new(category.0, category.1),
        issue_type,
    );
    rule.load_data(data);
    rule
}

pub fn arabic_darja_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_DARJA_REPLACE",
        name: "كلمات بديلة للكلمات العامية أو الأجنبية",
        short: "كلمات بديلة للكلمات العامية أو الأجنبية",
        message: "الكلمة عامية  أو أجنبية يفضل أن يقال $suggestions",
        separator: " أو  ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/darja.txt"
        ),
        category: ("STYLE", "Style"),
        issue_type: IssueType::LocaleViolation,
    })
}

pub fn arabic_diacritics_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_DIACRITICS_REPLACE",
        name: "كلمات مشكولة للتوضيح",
        short: "كلمات يستحسن أن تشكّل لتصحيح نطقها",
        message: "'$match' كلمة يشيع نطقها نطقا خاطئا لذا نقترح تشكيلها كالآتي: $suggestions",
        separator: " أو ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/diacritics.txt"
        ),
        category: ("STYLE", "Style"),
        issue_type: IssueType::Style,
    })
}

pub fn arabic_simple_replace_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_SIMPLE_REPLACE",
        name: "قاعدة تطابق الكلمات التي يجب تجنبها وتقترح تصويبا لها",
        short: "خطأ، يفضل أن  يقال:",
        message: "قل $suggestions",
        separator: " أو  ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/replaces.txt"
        ),
        category: ("CONFUSED_WORDS", "Confused Words"),
        issue_type: IssueType::Uncategorized,
    })
}

pub fn arabic_homophones_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_HOMOPHONES_REPLACE",
        name: "كلمات متشابهة لفظا للتوضيح، يرجى التحقق منها مثل تشابه الظاء والضاد.",
        short: "كلمات متشابهة لفظا يرجى التحقق منها",
        message: "قل $suggestions",
        separator: " أو  ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/homophones.txt"
        ),
        category: ("CONFUSED_WORDS", "Confused Words"),
        issue_type: IssueType::Uncategorized,
    })
}

pub fn arabic_redundancy_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_REDUNDANCY_REPLACE",
        name: "1. تكرار (عام)",
        short: "تكرار",
        message: "'$match' تعبير فيه تكرار.في بعض الحالات، يستحسن استعمال $suggestions",
        separator: " أو ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/redundancies.txt"
        ),
        category: ("REDUNDANCY", "Redundancy"),
        issue_type: IssueType::Style,
    })
}

pub fn arabic_wordiness_rule() -> ArabicSimpleReplaceRule2 {
    replace_rule(ReplaceRuleSpec {
        id: "AR_WORDINESS_REPLACE",
        name: "2. حشو(تعبير فيه تكرار)",
        short: "حشو (تعبير فيه تكرار)",
        message: "'$match' تعبير فيه حشو يفضل أن يقال $suggestions",
        separator: " أو ",
        data: include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/wordiness.txt"
        ),
        category: ("REDUNDANCY", "Redundancy"),
        issue_type: IssueType::Style,
    })
}

pub fn arabic_word_coherency_rule() -> ArabicWordCoherencyRule {
    ArabicWordCoherencyRule::new()
}

pub fn arabic_word_repeat_rule() -> ArabicWordRepeatRule {
    ArabicWordRepeatRule::new()
}

pub fn arabic_wrong_word_in_context_rule() -> crate::en::WrongWordInContextRule {
    crate::en::WrongWordInContextRule::for_language(
        "ARABIC_WRONG_WORD_IN_CONTEXT",
        "كلمات شائعةمتشابهة (ظل/ضلَ, رؤيا/رؤية الخ.)",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/wrongWordInContext.txt"
        ),
        "احتمال كلمة متشابهة: هل تقصد <suggestion>$SUGGESTION</suggestion> بدلا من '$WRONGWORD'?",
        "احتمال كلمة متشابهة: هل تقصد <suggestion>$SUGGESTION</suggestion> (= $EXPLANATION_SUGGESTION) بدلا من '$WRONGWORD' (= $EXPLANATION_WRONGWORD)?",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::{Rule as _, TextLevelRule as _};

    #[test]
    fn arabic_natives_use_java_ids() {
        assert_eq!(
            arabic_double_punctuation_rule().id(),
            "ARABIC_DOUBLE_PUNCTUATION"
        );
        assert_eq!(
            arabic_comma_whitespace_rule().id(),
            "ARABIC_COMMA_PARENTHESIS_WHITESPACE"
        );
        assert_eq!(
            arabic_question_mark_whitespace_rule().id(),
            "ARABIC_QM_WHITESPACE"
        );
        assert_eq!(
            arabic_semicolon_whitespace_rule().id(),
            "ARABIC_SC_WHITESPACE"
        );
        assert_eq!(arabic_word_repeat_rule().id(), "ARABIC_WORD_REPEAT_RULE");
        assert_eq!(arabic_darja_rule().id(), "AR_DARJA_REPLACE");
        assert_eq!(arabic_diacritics_rule().id(), "AR_DIACRITICS_REPLACE");
        assert_eq!(arabic_simple_replace_rule().id(), "AR_SIMPLE_REPLACE");
        assert_eq!(arabic_homophones_rule().id(), "AR_HOMOPHONES_REPLACE");
        assert_eq!(arabic_redundancy_rule().id(), "AR_REDUNDANCY_REPLACE");
        assert_eq!(arabic_wordiness_rule().id(), "AR_WORDINESS_REPLACE");
        assert_eq!(arabic_word_coherency_rule().id(), "AR_WORD_COHERENCY");
        assert_eq!(
            arabic_wrong_word_in_context_rule().id(),
            "ARABIC_WRONG_WORD_IN_CONTEXT"
        );
    }

    #[test]
    fn arabic_word_coherency_flags_mixed_spelling() {
        use og_core::rule::TextLevelRule as _;
        let rule = arabic_word_coherency_rule();
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
                    og_core::AnalyzedToken::new(part, start + s, start + e)
                        .with_lemma(part.to_string())
                        .with_pos_tags(vec!["NJ-;---;---".to_string()]),
                ));
            }
            sentence.set_tokens(tokens);
            sentence
        };
        let s1 = mk("شؤون", 0);
        let s2 = mk("شئون", 6);
        let matches = rule.match_text("شؤون شئون", &[s1, s2]);
        assert_eq!(matches.len(), 1, "second variant should be flagged");
    }

    #[test]
    fn arabic_double_punctuation_flags_arabic_comma_pair() {
        use og_tokenizer::WordTokenizer as _;
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new_splitting_hyphen();
        let tokens = tokenizer.tokenize("نعم ،، لقد");
        let mut sentence = og_core::AnalyzedSentence::new("نعم ،، لقد", 0, "نعم ،، لقد".len());
        sentence.set_tokens(
            tokens
                .iter()
                .map(|t| {
                    og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                        t.text(),
                        t.start(),
                        t.end(),
                    ))
                })
                .collect(),
        );
        let matches = arabic_double_punctuation_rule().match_sentence(&sentence);
        assert!(!matches.is_empty(), "expected match on '،،'");
    }
}
