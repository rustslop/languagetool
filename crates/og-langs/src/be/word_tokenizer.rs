use og_core::Token;

pub struct BelarusianWordTokenizer {
    inner: og_tokenizer::DefaultWordTokenizer,
}

impl BelarusianWordTokenizer {
    pub fn new() -> Self {
        Self {
            inner: og_tokenizer::DefaultWordTokenizer::new_generic(),
        }
    }
}

impl Default for BelarusianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

fn is_be_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '\'' | '\u{2019}' | '\u{02BC}')
}

fn is_wordish(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_be_word_char)
}

fn finish(token: Token) -> Token {
    let text = token.text();
    if text.chars().count() > 1 && text.contains('\u{2019}') {
        return Token::new(text.replace('\u{2019}', "'"), token.start(), token.end());
    }
    token
}

// Java BelarusianWordTokenizer drops ' ’ ʼ from the tokenizing characters and
// rewrites '’' to '\'' only in tokens with length() > 1; the generic splitter
// emits each apostrophe as its own token, so contiguous word runs merge first.
impl og_tokenizer::WordTokenizer for BelarusianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let raw = self.inner.tokenize(text);
        let mut out: Vec<Token> = Vec::with_capacity(raw.len());
        let mut pending: Option<Token> = None;
        for token in raw {
            let extends_chain = pending
                .as_ref()
                .is_some_and(|prev| prev.end() == token.start())
                && is_wordish(token.text());
            if extends_chain && let Some(prev) = pending.take() {
                let mut merged = String::with_capacity(prev.text().len() + token.text().len());
                merged.push_str(prev.text());
                merged.push_str(token.text());
                pending = Some(Token::new(merged, prev.start(), token.end()));
                continue;
            }
            if let Some(prev) = pending.take() {
                out.push(finish(prev));
            }
            if is_wordish(token.text()) {
                pending = Some(token);
            } else {
                out.push(token);
            }
        }
        if let Some(prev) = pending {
            out.push(finish(prev));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        BelarusianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_apostrophe_stays_inside_the_word() {
        assert_eq!(texts("камп'ютар"), vec!["камп'ютар".to_string()]);
    }

    #[test]
    fn java_pin_carriage_return_splits_as_its_own_token() {
        assert_eq!(
            texts("Яно\rразбіваецца"),
            vec![
                "Яно".to_string(),
                "\r".to_string(),
                "разбіваецца".to_string()
            ]
        );
    }

    #[test]
    fn java_pin_email_is_joined_and_surrounding_text_split() {
        assert_eq!(
            texts("Мой адрас — address@email.com"),
            vec![
                "Мой".to_string(),
                " ".to_string(),
                "адрас".to_string(),
                " ".to_string(),
                "—".to_string(),
                " ".to_string(),
                "address@email.com".to_string(),
            ]
        );
    }

    #[test]
    fn right_single_quote_is_normalized_to_apostrophe_in_long_tokens() {
        assert_eq!(texts("з’ява"), vec!["з'ява".to_string()]);
        assert_eq!(
            texts("п’яны З’яўляцца"),
            vec![
                "п'яны".to_string(),
                " ".to_string(),
                "З'яўляцца".to_string()
            ]
        );
    }

    #[test]
    fn rewritten_token_offsets_still_index_the_original_text() {
        let text = "з’ява вялікая";
        let tokens = BelarusianWordTokenizer::new().tokenize(text);
        let zyava = &tokens[0];
        assert_eq!(zyava.text(), "з'ява");
        assert_eq!(zyava.start(), 0);
        assert_eq!(zyava.end(), "з’ява".len());
        assert_eq!(&text[zyava.start()..zyava.end()], "з’ява");
    }

    #[test]
    fn modifier_letter_apostrophe_stays_in_the_word_unrewritten() {
        assert_eq!(texts("зʼява"), vec!["зʼява".to_string()]);
        assert_eq!(
            texts("З'ява, зʼява, з’ява"),
            vec![
                "З'ява".to_string(),
                ",".to_string(),
                " ".to_string(),
                "зʼява".to_string(),
                ",".to_string(),
                " ".to_string(),
                "з'ява".to_string(),
            ]
        );
    }

    #[test]
    fn single_quote_character_token_is_left_alone() {
        assert_eq!(texts("’"), vec!["’".to_string()]);
        assert_eq!(texts("’ "), vec!["’".to_string(), " ".to_string()]);
    }

    #[test]
    fn leading_and_trailing_apostrophes_join_the_adjacent_word() {
        assert_eq!(texts("'ясна"), vec!["'ясна".to_string()]);
        assert_eq!(texts("ясна'"), vec!["ясна'".to_string()]);
        assert_eq!(
            texts("а ' б"),
            vec![
                "а".to_string(),
                " ".to_string(),
                "'".to_string(),
                " ".to_string(),
                "б".to_string()
            ]
        );
    }

    #[test]
    fn apostrophe_does_not_merge_across_punctuation_or_whitespace() {
        assert_eq!(
            texts("а,'б"),
            vec!["а".to_string(), ",".to_string(), "'б".to_string()]
        );
        assert_eq!(
            texts("з’ ява"),
            vec!["з'".to_string(), " ".to_string(), "ява".to_string()]
        );
    }
}
