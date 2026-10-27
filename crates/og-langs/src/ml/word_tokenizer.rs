use og_core::Token;
use og_tokenizer::WordTokenizer;

fn is_ml_delimiter(c: char) -> bool {
    matches!(
        c,
        '\u{0020}'
            | '\u{00A0}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{1680}'
            | ','
            | '.'
            | ';'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '!'
            | '?'
            | ':'
            | '"'
            | '\''
            | '\u{2019}'
            | '\u{2018}'
            | '\u{201E}'
            | '\u{201C}'
            | '\u{201D}'
            | '\u{2026}'
            | '\\'
            | '/'
            | '\t'
            | '\n'
    )
}

pub struct MalayalamWordTokenizer;

impl MalayalamWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MalayalamWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WordTokenizer for MalayalamWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((byte, c)) = chars.next() {
            if is_ml_delimiter(c) {
                tokens.push(Token::new(c.to_string(), byte, byte + c.len_utf8()));
                continue;
            }
            let start = byte;
            let mut end = byte + c.len_utf8();
            while let Some(&(next_byte, next_char)) = chars.peek() {
                if is_ml_delimiter(next_char) {
                    break;
                }
                end = next_byte + next_char.len_utf8();
                chars.next();
            }
            tokens.push(Token::new(&text[start..end], start, end));
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenized(text: &str) -> Vec<String> {
        MalayalamWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn splits_words_and_keeps_delimiters_like_the_java_string_tokenizer() {
        assert_eq!(
            tokenized("ഞാന്‍ അവളെ ഒരു പുസ്തകം നല്‍കി."),
            vec!["ഞാന്‍", " ", "അവളെ", " ", "ഒരു", " ", "പുസ്തകം", " ", "നല്‍കി", "."]
        );
    }

    #[test]
    fn western_punctuation_and_quotes_are_single_token_delimiters() {
        assert_eq!(tokenized("a,b.c"), vec!["a", ",", "b", ".", "c"]);
        assert_eq!(tokenized("a(b)c"), vec!["a", "(", "b", ")", "c"]);
        assert_eq!(
            tokenized("a[b]{c}"),
            vec!["a", "[", "b", "]", "{", "c", "}"]
        );
        assert_eq!(
            tokenized("x!y?z:q"),
            vec!["x", "!", "y", "?", "z", ":", "q"]
        );
        assert_eq!(
            tokenized("a“b”c‘d’e„f”…g"),
            vec![
                "a", "\u{201C}", "b", "\u{201D}", "c", "\u{2018}", "d", "\u{2019}", "e",
                "\u{201E}", "f", "\u{201D}", "\u{2026}", "g"
            ]
        );
    }

    #[test]
    fn consecutive_delimiters_each_keep_their_own_token() {
        assert_eq!(tokenized("a,,b"), vec!["a", ",", ",", "b"]);
        assert_eq!(tokenized("a  b"), vec!["a", " ", " ", "b"]);
        assert_eq!(tokenized("a\t\nb"), vec!["a", "\t", "\n", "b"]);
    }

    #[test]
    fn malayalam_script_punctuation_and_zwj_zwnj_stay_inside_words() {
        assert_eq!(
            tokenized("അച്ഛന്‍റെ കൂടെ അവളും കൂടി പോയി."),
            vec![
                "അച്ഛന്‍റെ",
                " ",
                "കൂടെ",
                " ",
                "അവളും",
                " ",
                "കൂടി",
                " ",
                "പോയി",
                "."
            ],
            "ZWJ (U+200D) inside അച്ഛന്‍റെ and the Malayalam signs are absent from the Java \
             delimiter set (MalayalamWordTokenizer.java:40-42), so the words stay whole"
        );
    }

    #[test]
    fn whitespace_exotic_and_ordinary_digits_are_not_delimited() {
        assert_eq!(tokenized("500 പേര്‍"), vec!["500", " ", "പേര്‍"]);
        assert_eq!(tokenized("a\u{115F}b"), vec!["a", "\u{115F}", "b"]);
        assert_eq!(tokenized("a\u{1680}b"), vec!["a", "\u{1680}", "b"]);
        assert_eq!(
            tokenized("a\u{00A0}b"),
            vec!["a", "\u{00A0}", "b"],
            "NBSP is in the Java delimiter set"
        );
        assert_eq!(
            tokenized("a\rb"),
            vec!["a\rb"],
            "\\r is NOT in the Java delimiter set, so it stays inside the token"
        );
    }

    #[test]
    fn byte_offsets_partition_the_original_text() {
        let text = "ഞാന്‍ അവളെ, ഒരു പുസ്തകം?";
        let tokens = MalayalamWordTokenizer::new().tokenize(text);
        let joined: String = tokens.iter().map(|t| t.text().to_string()).collect();
        assert_eq!(joined, text);
        for pair in tokens.windows(2) {
            assert_eq!(pair[0].end(), pair[1].start());
        }
        if let Some(last) = tokens.last() {
            assert_eq!(last.end(), text.len());
        }
    }
}
