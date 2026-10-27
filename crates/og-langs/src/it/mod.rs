pub mod speller;

#[cfg(test)]
mod example_suite;

#[cfg(test)]
mod trace;

use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};

const BASE_IGNORES: &[&str] = &[
    "Phi", "Li", "Xiao", "Duran", "Wagga", "Abdullah", "Nwe", "Pago", "Cao",
];

const ITALIAN_IGNORES: &[&str] = &["cos\u{ec}", "passo", "piano", "via"];

fn is_zs(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

fn word_for_speller(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| {
            c.is_alphabetic() || c.is_numeric() || c.is_ascii_punctuation() || c == ' ' || is_zs(c)
        })
}

fn is_emoji(token: &str) -> bool {
    token.chars().any(|c| c as u32 > 0xFFFF) && !word_for_speller(token)
}

fn is_numeric_space(token: &str) -> bool {
    token.chars().all(|c| c == ' ' || c.is_numeric())
}

fn is_word(token: &str) -> bool {
    if is_emoji(token) {
        return false;
    }
    if is_numeric_space(token) {
        return false;
    }
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && !c.is_alphabetic()
    {
        return false;
    }
    true
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

pub struct ItalianWordRepeatRule {
    id: String,
    category: Category,
}

impl ItalianWordRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "ITALIAN_WORD_REPEAT_RULE".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
        }
    }

    fn word_repetition_of(word: &str, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        position > 0
            && tokens[position - 1].token().token() == word
            && tokens[position].token().token() == word
    }

    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        if ITALIAN_IGNORES
            .iter()
            .chain(BASE_IGNORES.iter())
            .any(|word| Self::word_repetition_of(word, tokens, position))
        {
            return true;
        }
        false
    }
}

impl Default for ItalianWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ItalianWordRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Parola ripetuta (es. 'casa casa')"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let mut prev_token: &str = "";

        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if is_word(token)
                && !prev_token.is_empty()
                && eq_ignore_case(prev_token, token)
                && !self.ignore(&tokens, i)
            {
                let start = tokens[i - 1].token().start();
                let end = tokens[i].token().start() + prev_token.len();
                if end <= start {
                    prev_token = token;
                    continue;
                }
                let rel_start = start - sentence.start();
                let rel_end = end - sentence.start();
                let text_len = sentence.text().len();
                let mut context_start = rel_start.saturating_sub(20);
                while context_start > 0 && !sentence.text().is_char_boundary(context_start) {
                    context_start -= 1;
                }
                let mut context_end = std::cmp::min(rel_end + 20, text_len);
                while context_end < text_len && !sentence.text().is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = sentence.text()[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Redundancy.as_str());

                matches.push(
                    RuleMatch::new(
                        "Possibile errore di battitura: parola ripetuta",
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![SuggestedReplacement::new(prev_token)]),
                );
            }
            prev_token = token;
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() {
                continue;
            }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    part, start, end,
                )));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn detects_real_repetition() {
        let rule = ItalianWordRepeatRule::new();
        let sentence = make_sentence("Questa \u{e8} una una prova");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(
            &sentence.text()[m.offset()..m.offset() + m.length()],
            "una una"
        );
        assert_eq!(
            m.message(),
            "Possibile errore di battitura: parola ripetuta"
        );
        assert_eq!(m.replacements().len(), 1);
        assert_eq!(m.replacements()[0].value(), "una");
    }

    #[test]
    fn span_covers_both_tokens_like_java() {
        let rule = ItalianWordRepeatRule::new();
        let sentence = make_sentence("Mi mi \u{e8} sembrato cos\u{ec}");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &sentence.text()[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "Mi mi"
        );
        assert_eq!(matches[0].replacements()[0].value(), "Mi");
    }

    #[test]
    fn java_test_rule_expectations() {
        let rule = ItalianWordRepeatRule::new();
        assert!(
            rule.match_sentence(&make_sentence("Mi \u{e8} sembrato cos\u{ec} cos\u{ec}"))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&make_sentence("Duran Duran"))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&make_sentence("Devi mescolare piano piano"))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&make_sentence("Seguo passo passo"))
                .is_empty()
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Mi mi \u{e8} sembrato cos\u{ec}"))
                .len(),
            1
        );
    }

    #[test]
    fn non_words_never_repeat() {
        let rule = ItalianWordRepeatRule::new();
        assert!(
            rule.match_sentence(&make_sentence("Pag\u{f2} 1 000 000 euro"))
                .is_empty()
        );
        assert!(rule.match_sentence(&make_sentence("- -")).is_empty());
        assert!(
            rule.match_sentence(&make_sentence("Wagga Wagga Wagga"))
                .is_empty()
        );
    }

    #[test]
    fn ignores_italian_idioms() {
        let rule = ItalianWordRepeatRule::new();
        for text in [
            "Mi \u{e8} sembrato cos\u{ec} cos\u{ec}.",
            "Andiamo piano piano verso la meta",
            "Il lessico si andava via via modificando.",
            "che passo passo vanno verso la rovina",
        ] {
            let sentence = make_sentence(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should ignore repetition in '{text}'"
            );
        }
    }
}
