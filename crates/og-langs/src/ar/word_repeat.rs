use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};

const IGNORED_REPETITIONS: &[&str] = &["خطوة", "رويدا"];

fn is_emoji(token: &str) -> bool {
    token.chars().any(|c| c as u32 > 0xFFFF)
        && !token.chars().any(|c| c.is_alphabetic() || c.is_numeric())
}

fn is_numeric_space(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c == ' ' || c.is_numeric())
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

pub struct ArabicWordRepeatRule {
    id: String,
    category: Category,
}

impl ArabicWordRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "ARABIC_WORD_REPEAT_RULE".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
        }
    }

    fn word_repetition_of(word: &str, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        position > 0
            && tokens[position - 1].token().token() == word
            && tokens[position].token().token() == word
    }

    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        IGNORED_REPETITIONS
            .iter()
            .any(|word| Self::word_repetition_of(word, tokens, position))
    }
}

impl Default for ArabicWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for ArabicWordRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "كلمة مكررة ( مثال 'جميل جميل')"
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
            if tokens[i].is_immunized() {
                prev_token = "";
                continue;
            }
            if !prev_token.is_empty()
                && is_word(token)
                && eq_ignore_case(prev_token, token)
                && !self.ignore(&tokens, i)
            {
                let start = tokens[i - 1].token().start();
                let end = tokens[i].token().start() + prev_token.len();
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
                        "خطأ محتمل: كلمة مكررة",
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_short_message("كلمة مكررة")
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
    fn span_covers_both_repeated_tokens_like_java() {
        let rule = ArabicWordRepeatRule::new();
        let text = "هذا فقط فقط مثال.";
        let matches = rule.match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.offset(), "هذا ".len());
        assert_eq!(m.length(), "فقط فقط".len());
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "فقط فقط");
        assert_eq!(m.replacements()[0].value(), "فقط");
        assert_eq!(m.message(), "خطأ محتمل: كلمة مكررة");
    }

    #[test]
    fn ignores_java_word_repetition_whitelist() {
        let rule = ArabicWordRepeatRule::new();
        assert!(
            rule.match_sentence(&make_sentence("نفذت التعليمات خطوة خطوة."))
                .is_empty()
        );
        assert!(
            rule.match_sentence(&make_sentence("امش رويدا رويدا"))
                .is_empty()
        );
    }

    #[test]
    fn single_non_letter_and_numeric_tokens_are_not_words() {
        let rule = ArabicWordRepeatRule::new();
        assert!(rule.match_sentence(&make_sentence("1 1")).is_empty());
        assert!(rule.match_sentence(&make_sentence("؟ ؟")).is_empty());
    }
}
