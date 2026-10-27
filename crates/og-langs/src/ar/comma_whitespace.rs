use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use regex::Regex;
use std::sync::LazyLock;

const MSG_NO_SPACE_AFTER: &str = "لا تضع فراغا بعد القوس المفتوح.";
const MSG_NO_SPACE_AROUND_QUOTES: &str = "لا تضع مسافة على كلا جانبي رمز التنصيص.";
const MSG_MISSING_SPACE_AFTER_COMMA: &str = "ضع فراغا بعد الفاصلة.";
const MSG_SPACE_AFTER_COMMA: &str = "ضع فراغا بعد الفاصلة، وليس قبلها.";
const MSG_NO_SPACE_BEFORE: &str = "لا تضع فراغا قبل القوس المغلق.";
const MSG_NO_SPACE_BEFORE_DOT: &str = "لا تضع فراغا قبل النقطة.";

static DOMAIN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)(com|org|net|int|edu|gov|mil|[a-z]{2})").unwrap());
static FILE_EXTENSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("([a-z]{3,4}|[A-Z]{3,4}|ai|mp[34]|MP[34])(-.+)?").unwrap());
static ALPHA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new("[a-zA-Z]+").unwrap());

pub struct ArabicCommaWhitespaceRule {
    id: String,
    comma: char,
}

pub fn arabic_comma_whitespace_rule() -> ArabicCommaWhitespaceRule {
    ArabicCommaWhitespaceRule {
        id: "ARABIC_COMMA_PARENTHESIS_WHITESPACE".to_string(),
        comma: '،',
    }
}

pub fn arabic_question_mark_whitespace_rule() -> ArabicCommaWhitespaceRule {
    ArabicCommaWhitespaceRule {
        id: "ARABIC_QM_WHITESPACE".to_string(),
        comma: '؟',
    }
}

pub fn arabic_semicolon_whitespace_rule() -> ArabicCommaWhitespaceRule {
    ArabicCommaWhitespaceRule {
        id: "ARABIC_SC_WHITESPACE".to_string(),
        comma: '؛',
    }
}

pub fn arabic_base_comma_whitespace_rule() -> ArabicCommaWhitespaceRule {
    ArabicCommaWhitespaceRule {
        id: "COMMA_PARENTHESIS_WHITESPACE".to_string(),
        comma: ',',
    }
}

fn is_whitespace_token(token: &AnalyzedTokenReadings) -> bool {
    let text = token.token().token();
    if text == "\u{200B}" {
        return false;
    }
    token.is_whitespace()
        || text.is_empty()
        || text.chars().all(|c| c == '\u{00A0}')
        || token.has_pos_tag("SENT_START")
}

fn is_quote(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('\'' | '"' | '’' | '”' | '“' | '«' | '»'), None)
    )
}

fn is_hyphen_or_comma(s: &str) -> bool {
    let mut chars = s.chars();
    matches!((chars.next(), chars.next()), (Some('-' | ','), None))
}

fn is_digit_or_dot(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c == '.' || c.is_numeric())
}

fn contains_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_numeric())
}

fn starts_with_open_paren(s: &str) -> bool {
    s.starts_with('(')
}

fn starts_with_close_paren(s: &str) -> bool {
    s.starts_with(')')
}

fn is_proper_noun(token: &AnalyzedTokenReadings) -> bool {
    token.readings().iter().any(|r| {
        r.pos_tags()
            .iter()
            .any(|t| t == "NNP" || t == "NNPS" || t.starts_with("NP"))
    })
}

fn token_text(tokens: &[AnalyzedTokenReadings], i: usize) -> &str {
    tokens.get(i).map(|t| t.token().token()).unwrap_or_default()
}

impl Rule for ArabicCommaWhitespaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "استعمال فراغات قبل الفاصلة أو قبل الأقواس أو بعدها"
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.tokens();
        let text = sentence.text();
        let comma = self.comma.to_string();
        let mut rule_matches = Vec::new();
        let mut prev_token = "";
        let mut prev_prev_token = "";
        let mut prev_white = false;

        for i in 0..tokens.len() {
            let token_reading = &tokens[i];
            let token = token_reading.token().token();
            let is_whitespace = is_whitespace_token(token_reading);
            let mut msg: Option<&str> = None;
            let mut suggestion: Option<String> = None;
            let mut two_suggestions = false;

            if is_whitespace && starts_with_open_paren(prev_token) {
                let is_exception =
                    prev_token == "[" && token == " " && token_text(tokens, i + 1) == "]";
                if !is_exception {
                    msg = Some(MSG_NO_SPACE_AFTER);
                    suggestion = Some(prev_token.to_string());
                }
            } else if is_whitespace && is_quote(prev_token) && prev_prev_token == " " {
                msg = Some(MSG_NO_SPACE_AROUND_QUOTES);
                suggestion = Some(prev_token.to_string());
                two_suggestions = true;
            } else if !is_whitespace
                && prev_token == comma
                && !is_quote(token)
                && !is_hyphen_or_comma(token)
                && !contains_digit(prev_prev_token)
                && !contains_digit(token)
                && prev_prev_token != ","
            {
                msg = Some(MSG_MISSING_SPACE_AFTER_COMMA);
                suggestion = Some(format!("{comma} {token}"));
            } else if prev_white {
                if starts_with_close_paren(token) {
                    let is_exception = token == "]" && prev_token == " " && prev_prev_token == "[";
                    if !is_exception {
                        msg = Some(MSG_NO_SPACE_BEFORE);
                        suggestion = Some(token.to_string());
                    }
                } else if token == comma {
                    if token_text(tokens, i + 1) != comma {
                        msg = Some(MSG_SPACE_AFTER_COMMA);
                        let next_space =
                            if tokens.get(i + 1).is_some_and(|t| !is_whitespace_token(t)) {
                                format!("{comma} ")
                            } else {
                                comma.clone()
                            };
                        suggestion = Some(next_space);
                    }
                } else if token == "." {
                    let domain = tokens
                        .get(i + 1)
                        .is_some_and(|t| DOMAIN_RE.is_match(t.token().token()));
                    let file_extension = tokens
                        .get(i + 1)
                        .is_some_and(|t| FILE_EXTENSION_RE.is_match(t.token().token()));
                    if !domain && !file_extension && !is_proper_noun(token_reading) {
                        let next_digit_or_dot = tokens
                            .get(i + 1)
                            .is_some_and(|t| is_digit_or_dot(t.token().token()));
                        let next_slash_alpha = token_text(tokens, i + 1) == "/"
                            && tokens
                                .get(i + 2)
                                .is_some_and(|t| ALPHA_RE.is_match(t.token().token()));
                        if !next_digit_or_dot && !next_slash_alpha {
                            msg = Some(MSG_NO_SPACE_BEFORE_DOT);
                            suggestion = Some(".".to_string());
                        }
                    }
                }
            }

            if let Some(m) = msg
                && !token_reading.is_immunized()
            {
                let from_pos = tokens[i - 1].token().start();
                let from_pos = if two_suggestions {
                    tokens[i - 2].token().start()
                } else {
                    from_pos
                };
                let to_pos = token_reading.token().end();
                let Some(suggestion_text) = suggestion.as_deref() else {
                    prev_prev_token = prev_token;
                    prev_token = token;
                    prev_white = is_whitespace;
                    continue;
                };
                let rel_from = from_pos - sentence.start();
                let rel_to = to_pos - sentence.start();
                if rel_to < text.len()
                    && &text[rel_from..rel_to] == suggestion_text
                    && !two_suggestions
                {
                    prev_prev_token = prev_token;
                    prev_token = token;
                    prev_white = is_whitespace;
                    continue;
                }

                let text_len = text.len();
                let mut context_start = rel_from.saturating_sub(30);
                while context_start > 0 && !text.is_char_boundary(context_start) {
                    context_start -= 1;
                }
                let mut context_end = std::cmp::min(rel_to + 30, text_len);
                while context_end < text_len && !text.is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = text[context_start..context_end].to_string();

                let rm_rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category())
                    .with_issue_type(IssueType::Whitespace.as_str());

                let mut rule_match = RuleMatch::new(
                    m,
                    from_pos,
                    to_pos - from_pos,
                    rm_rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(text.to_string());

                if two_suggestions {
                    rule_match = rule_match.with_replacements(vec![
                        SuggestedReplacement::new(format!("{suggestion_text} ")),
                        SuggestedReplacement::new(format!(" {suggestion_text}")),
                    ]);
                } else {
                    rule_match = rule_match
                        .with_replacements(vec![SuggestedReplacement::new(suggestion_text)]);
                }
                rule_matches.push(rule_match);
            }

            prev_prev_token = prev_token;
            prev_token = token;
            prev_white = is_whitespace;
        }

        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_java() {
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
        assert_eq!(
            arabic_base_comma_whitespace_rule().id(),
            "COMMA_PARENTHESIS_WHITESPACE"
        );
    }
}
