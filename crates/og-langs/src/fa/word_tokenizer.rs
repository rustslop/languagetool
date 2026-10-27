use og_core::Token;

fn is_fa_delimiter(c: char) -> bool {
    matches!(c,
        '\u{0020}' | '\u{00A0}' | '\u{115F}' | '\u{1160}' | '\u{1680}'
        | '\u{2000}'..='\u{200F}'
        | '\u{2028}'..='\u{202F}'
        | '\u{205F}'..='\u{206F}'
        | '\u{3000}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}'
        | '\u{FFF9}'..='\u{FFFB}'
        | '\u{00A6}' | '\u{2016}' | '\u{2223}' | '|' | ',' | '.' | ';' | '(' | ')' | '['
        | ']' | '{' | '}' | '=' | '*' | '#' | '\u{2217}' | '+' | '\u{00D7}' | '\u{00B7}'
        | '\u{00F7}' | '<' | '>' | '!' | '?' | ':' | '~' | '/' | '\\' | '"' | '\u{00AB}'
        | '\u{00BB}' | '\u{201E}' | '\u{201C}' | '\u{201D}' | '\u{201B}' | '\u{2032}'
        | '\u{203A}' | '\u{2039}' | '\u{2026}' | '\u{00BF}' | '\u{00A1}' | '\u{203C}'
        | '\u{2047}' | '\u{2048}' | '\u{2049}' | '\u{2122}' | '\u{00AE}' | '\u{203D}'
        | '\u{00B6}' | '\u{FFEB}' | '\u{2E2E}'
        | '\'' | '`' | '\u{2019}' | '\u{2018}' | '\u{00B4}'
        | '\u{2012}'..='\u{2015}'
        | '\u{2500}' | '\u{3161}' | '\u{2713}'
        | '\u{25CF}' | '\u{25CB}' | '\u{25C6}' | '\u{27A2}' | '\u{25A0}' | '\u{25A1}'
        | '\u{2605}' | '\u{274F}' | '\u{2794}' | '\u{21B5}' | '\u{2756}' | '\u{25AA}'
        | '\u{2751}' | '\u{2022}'
        | '\u{2B9A}' | '\u{2265}' | '\u{2192}' | '\u{21FE}' | '\u{21C9}' | '\u{21D2}'
        | '\u{21E8}' | '\u{21DB}'
        | '\u{00B9}' | '\u{00B2}' | '\u{00B3}' | '\u{2070}' | '\u{2071}'
        | '\u{2074}'..='\u{2079}'
        | '\t' | '\n' | '\r' | '\u{000B}'
        | '\u{060C}' | '\u{061F}' | '\u{061B}'
    )
}

fn consume_url_or_email(remaining: &str, byte_start: usize) -> Option<usize> {
    if remaining.starts_with("http://")
        || remaining.starts_with("https://")
        || remaining.starts_with("ftp://")
        || remaining.starts_with("www.")
    {
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        return Some(byte_start + end);
    }
    if let Some(at_pos) = remaining.find('@')
        && at_pos > 0
        && remaining[..at_pos]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    {
        let domain = &remaining[at_pos + 1..];
        if let Some(ws) = domain.find(char::is_whitespace) {
            let part = &domain[..ws];
            if part.contains('.')
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            {
                return Some(byte_start + at_pos + 1 + ws);
            }
        } else if domain.contains('.')
            && domain
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        {
            return Some(byte_start + remaining.len());
        }
    }
    None
}

pub struct PersianWordTokenizer;

impl PersianWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PersianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_tokenizer::WordTokenizer for PersianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut i = 0usize;
        while i < text.len() {
            if let Some(end) = consume_url_or_email(&text[i..], i) {
                tokens.push(Token::new(&text[i..end], i, end));
                i = end;
                continue;
            }
            let c = text[i..].chars().next().unwrap();
            if is_fa_delimiter(c) {
                let end = i + c.len_utf8();
                tokens.push(Token::new(&text[i..end], i, end));
                i = end;
            } else {
                let start = i;
                i += c.len_utf8();
                while let Some(next) = text[i..].chars().next() {
                    if is_fa_delimiter(next) {
                        break;
                    }
                    i += next.len_utf8();
                }
                tokens.push(Token::new(&text[start..i], start, i));
            }
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        PersianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_persian_punctuation_is_tokenized() {
        assert_eq!(
            texts("سلام، دنیا؟"),
            vec![
                "سلام".to_string(),
                "،".to_string(),
                " ".to_string(),
                "دنیا".to_string(),
                "؟".to_string(),
            ],
            "PersianWordTokenizer.getTokenizingCharacters adds ،؟؛ to the base set \
             (PersianWordTokenizer.java:24-27)"
        );
        assert_eq!(
            texts("سلام؛دنیا"),
            vec!["سلام".to_string(), "؛".to_string(), "دنیا".to_string()]
        );
    }

    #[test]
    fn java_pin_zwnj_is_its_own_token() {
        assert_eq!(
            texts("آزمایش‌ها"),
            vec![
                "آزمایش".to_string(),
                "\u{200C}".to_string(),
                "ها".to_string()
            ],
            "U+200C ZWNJ sits in the base \\u2000-\\u200f tokenizing range, so compound \
             words split around it exactly as in Java"
        );
    }

    #[test]
    fn java_pin_base_punctuation_still_splits() {
        assert_eq!(
            texts("این است."),
            vec![
                "این".to_string(),
                " ".to_string(),
                "است".to_string(),
                ".".to_string()
            ]
        );
        assert_eq!(
            texts("گفت: «بله»"),
            vec![
                "گفت".to_string(),
                ":".to_string(),
                " ".to_string(),
                "«".to_string(),
                "بله".to_string(),
                "»".to_string()
            ]
        );
        assert_eq!(
            texts("کلمه،مورد"),
            vec!["کلمه".to_string(), "،".to_string(), "مورد".to_string()],
            "the ASCII comma stays a delimiter next to the Persian one"
        );
    }

    #[test]
    fn java_pin_each_whitespace_char_is_its_own_token() {
        assert_eq!(
            texts("یک  جمله"),
            vec![
                "یک".to_string(),
                " ".to_string(),
                " ".to_string(),
                "جمله".to_string(),
            ]
        );
    }

    #[test]
    fn java_pin_emails_and_urls_are_joined() {
        assert_eq!(
            texts("بفرست به user@example.com الان"),
            vec![
                "بفرست".to_string(),
                " ".to_string(),
                "به".to_string(),
                " ".to_string(),
                "user@example.com".to_string(),
                " ".to_string(),
                "الان".to_string(),
            ]
        );
        assert_eq!(
            texts("ببین https://www.example.com/topic الان"),
            vec![
                "ببین".to_string(),
                " ".to_string(),
                "https://www.example.com/topic".to_string(),
                " ".to_string(),
                "الان".to_string(),
            ]
        );
    }

    #[test]
    fn token_offsets_reconstruct_the_original_text() {
        for text in [
            "سلام، دنیا؟ این است.",
            "آزمایش‌ها را می‌بینم.",
            "user@example.com و https://example.com/الف",
            "جملهٔ اول... خوب نیست؟ جملهٔ دوم.",
        ] {
            let tokens = PersianWordTokenizer::new().tokenize(text);
            let mut rebuilt = String::new();
            let mut last_end = 0usize;
            for t in &tokens {
                assert_eq!(t.start(), last_end, "gap/overlap in '{text}' at {last_end}");
                rebuilt.push_str(t.text());
                last_end = t.end();
            }
            assert_eq!(rebuilt, text);
        }
    }
}
