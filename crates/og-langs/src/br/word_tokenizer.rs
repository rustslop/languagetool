use og_core::Token;

pub struct BretonWordTokenizer {
    inner: og_tokenizer::DefaultWordTokenizer,
}

impl BretonWordTokenizer {
    pub fn new() -> Self {
        Self {
            inner: og_tokenizer::DefaultWordTokenizer::new_generic(),
        }
    }
}

impl Default for BretonWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{02BC}')
}

fn is_single_apostrophe(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(c) if is_apostrophe(c)) && chars.next().is_none()
}

// Java BretonWordTokenizer sentinelizes [Cc]apos[Hh] (the c'h trigraph) before the
// generic split and cuts any other letter+apostrophe, restoring U+2019 in both.
impl og_tokenizer::WordTokenizer for BretonWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let raw = self.inner.tokenize(text);
        let mut out: Vec<Token> = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            let token = &raw[i];
            if is_single_apostrophe(token.text()) {
                let adjacent_prev = out.last().filter(|p| p.end() == token.start());
                let prev_char = adjacent_prev.and_then(|p| p.text().chars().next_back());
                let next_char = raw
                    .get(i + 1)
                    .filter(|n| n.start() == token.end())
                    .and_then(|n| n.text().chars().next());
                if matches!(prev_char, Some('c' | 'C')) && matches!(next_char, Some('h' | 'H')) {
                    let next = &raw[i + 1];
                    if let Some(prev) = out.pop() {
                        let mut merged =
                            String::with_capacity(prev.text().len() + 3 + next.text().len());
                        merged.push_str(prev.text());
                        merged.push('\u{2019}');
                        merged.push_str(next.text());
                        out.push(Token::new(merged, prev.start(), next.end()));
                        i += 2;
                        continue;
                    }
                }
                if prev_char.is_some_and(char::is_alphabetic)
                    && let Some(prev) = out.pop()
                {
                    let mut merged = String::with_capacity(prev.text().len() + 3);
                    merged.push_str(prev.text());
                    merged.push('\u{2019}');
                    out.push(Token::new(merged, prev.start(), token.end()));
                    i += 1;
                    continue;
                }
                out.push(token.clone());
                i += 1;
                continue;
            }
            push_word_pieces(token, &mut out);
            i += 1;
        }
        out
    }
}

fn push_word_pieces(token: &Token, out: &mut Vec<Token>) {
    let text = token.text();
    if !text.contains('\u{02BC}') {
        out.push(token.clone());
        return;
    }
    let base = token.start();
    let mut segment = String::with_capacity(text.len());
    let mut segment_start = 0usize;
    let mut byte_idx = 0usize;
    for ch in text.chars() {
        let len = ch.len_utf8();
        if is_apostrophe(ch) {
            let before = text[..byte_idx].chars().next_back();
            let after = text[byte_idx + len..].chars().next();
            if matches!(before, Some('c' | 'C')) && matches!(after, Some('h' | 'H')) {
                segment.push('\u{2019}');
            } else if before.is_some_and(char::is_alphabetic) {
                segment.push('\u{2019}');
                out.push(Token::new(
                    std::mem::take(&mut segment),
                    base + segment_start,
                    base + byte_idx + len,
                ));
                segment_start = byte_idx + len;
            } else {
                segment.push(ch);
            }
        } else {
            segment.push(ch);
        }
        byte_idx += len;
    }
    if segment_start < text.len() || !segment.is_empty() {
        out.push(Token::new(segment, base + segment_start, base + text.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        BretonWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_test_c_h_with_typewriter_apostrophe() {
        assert_eq!(
            texts("Test c'h"),
            vec!["Test".to_string(), " ".to_string(), "c’h".to_string()]
        );
    }

    #[test]
    fn java_pin_test_c_h_with_typographic_apostrophe() {
        assert_eq!(
            texts("Test c’h"),
            vec!["Test".to_string(), " ".to_string(), "c’h".to_string()]
        );
    }

    #[test]
    fn java_pin_c_hwerc_h_sentence_stays_thirteen_tokens() {
        assert_eq!(
            texts("C'hwerc'h merc'h gwerc'h war c'hwerc'h marc'h kalloc'h"),
            vec![
                "C’hwerc’h".to_string(),
                " ".to_string(),
                "merc’h".to_string(),
                " ".to_string(),
                "gwerc’h".to_string(),
                " ".to_string(),
                "war".to_string(),
                " ".to_string(),
                "c’hwerc’h".to_string(),
                " ".to_string(),
                "marc’h".to_string(),
                " ".to_string(),
                "kalloc’h".to_string(),
            ]
        );
    }

    #[test]
    fn java_pin_test_n_eo_splits_in_two_tokens_only() {
        assert_eq!(
            texts("Test n’eo"),
            vec![
                "Test".to_string(),
                " ".to_string(),
                "n’".to_string(),
                "eo".to_string()
            ]
        );
    }

    #[test]
    fn leading_apostrophe_keeps_its_own_token() {
        assert_eq!(texts("’test"), vec!["’".to_string(), "test".to_string()]);
        assert_eq!(
            texts("x ’ y"),
            vec![
                "x".to_string(),
                " ".to_string(),
                "’".to_string(),
                " ".to_string(),
                "y".to_string()
            ]
        );
    }

    #[test]
    fn apostrophe_after_digit_is_not_merged() {
        assert_eq!(
            texts("25’30"),
            vec!["25".to_string(), "’".to_string(), "30".to_string()]
        );
    }

    #[test]
    fn left_quote_apostrophe_behaves_like_the_others() {
        assert_eq!(
            texts("n‘eo c‘h"),
            vec![
                "n’".to_string(),
                "eo".to_string(),
                " ".to_string(),
                "c’h".to_string()
            ]
        );
    }

    #[test]
    fn modifier_letter_apostrophe_splits_like_java_pattern_two() {
        assert_eq!(texts("nʼeo"), vec!["n’".to_string(), "eo".to_string()]);
        assert_eq!(texts("cʼhwi"), vec!["c’hwi".to_string()]);
        assert_eq!(texts("25ʼ30"), vec!["25ʼ30".to_string()]);
    }

    #[test]
    fn double_apostrophe_mirrors_the_java_token_stream() {
        assert_eq!(
            texts("c’’h"),
            vec!["c’".to_string(), "’".to_string(), "h".to_string()]
        );
    }

    #[test]
    fn rewritten_tokens_span_the_original_text_bytes() {
        let text = "C'hwerc'h merc'h";
        let tokens = BretonWordTokenizer::new().tokenize(text);
        assert_eq!(tokens[0].text(), "C’hwerc’h");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), "C'hwerc'h".len());
        assert_eq!(&text[tokens[0].start()..tokens[0].end()], "C'hwerc'h");
        assert_eq!(tokens[2].text(), "merc’h");
        assert_eq!(tokens[2].start(), "C'hwerc'h ".len());
        assert_eq!(tokens[2].end(), text.len());
    }
}
