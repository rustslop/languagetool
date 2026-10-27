use og_core::Token;

const O_TI: &str = "\u{3CC},\u{3C4}\u{3B9}";

fn is_el_delimiter(c: char) -> bool {
    matches!(c,
        '\u{0020}' | '\u{00A0}' | '\u{115F}' | '\u{1160}' | '\u{1680}'
        | '\u{2000}'..='\u{200F}'
        | '\u{2028}'..='\u{202F}'
        | '\u{205F}'..='\u{2063}'
        | '\u{206A}'..='\u{206F}'
        | '\u{3000}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}'
        | '\u{FFF9}'..='\u{FFFB}'
        | ',' | '.' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '!' | ':' | '"' | '\''
        | '\u{0387}'
        | '\u{2019}' | '\u{2018}' | '\u{201E}' | '\u{201C}' | '\u{201D}'
        | '\u{2026}' | '\u{00AB}' | '\u{00BB}' | '\\' | '/'
        | '\t' | '\n'
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

pub struct GreekWordTokenizer;

impl GreekWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GreekWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_tokenizer::WordTokenizer for GreekWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut i = 0usize;
        while i < text.len() {
            if let Some(end) = consume_url_or_email(&text[i..], i) {
                tokens.push(Token::new(&text[i..end], i, end));
                i = end;
                continue;
            }
            if text[i..].starts_with(O_TI) {
                let end = i + O_TI.len();
                tokens.push(Token::new(&text[i..end], i, end));
                i = end;
                continue;
            }
            let c = text[i..].chars().next().unwrap();
            if is_el_delimiter(c) {
                let end = i + c.len_utf8();
                tokens.push(Token::new(&text[i..end], i, end));
                i = end;
            } else {
                let start = i;
                i += c.len_utf8();
                while let Some(next) = text[i..].chars().next() {
                    if is_el_delimiter(next) {
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
        GreekWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_basic_sentence_with_final_dot() {
        assert_eq!(
            texts("Η τελεία είναι σημείο στίξης."),
            vec![
                "Η".to_string(),
                " ".to_string(),
                "τελεία".to_string(),
                " ".to_string(),
                "είναι".to_string(),
                " ".to_string(),
                "σημείο".to_string(),
                " ".to_string(),
                "στίξης".to_string(),
                ".".to_string(),
            ]
        );
    }

    #[test]
    fn java_pin_greek_semicolon_is_a_delimiter_but_ascii_question_mark_is_not() {
        assert_eq!(
            texts("Πώς είσαι;"),
            vec![
                "Πώς".to_string(),
                " ".to_string(),
                "είσαι".to_string(),
                ";".to_string()
            ]
        );
        assert_eq!(texts("OK?"), vec!["OK?".to_string()]);
    }

    #[test]
    fn java_pin_ano_teleia_and_guillemets_split() {
        assert_eq!(
            texts("μία\u{0387}δύο"),
            vec!["μία".to_string(), "\u{0387}".to_string(), "δύο".to_string()]
        );
        assert_eq!(
            texts("μία\u{00B7}δύο"),
            vec!["μία\u{00B7}δύο".to_string()],
            "U+00B7 middle dot is not in the jflex delimiter set"
        );
        assert_eq!(
            texts("«λέξη»"),
            vec!["«".to_string(), "λέξη".to_string(), "»".to_string()]
        );
    }

    #[test]
    fn java_pin_curly_apostrophe_splits_but_hyphen_stays_inside_runs() {
        assert_eq!(
            texts("Γιώργος’s"),
            vec!["Γιώργος".to_string(), "’".to_string(), "s".to_string()]
        );
        assert_eq!(texts("καλά-κακώς"), vec!["καλά-κακώς".to_string()]);
        assert_eq!(texts("2020-2021"), vec!["2020-2021".to_string()]);
    }

    #[test]
    fn java_pin_lower_case_o_ti_stays_whole_upper_case_splits() {
        assert_eq!(
            texts("ό,τι και να κάνεις"),
            vec![
                "ό,τι".to_string(),
                " ".to_string(),
                "και".to_string(),
                " ".to_string(),
                "να".to_string(),
                " ".to_string(),
                "κάνεις".to_string(),
            ]
        );
        assert_eq!(
            texts("Ό,τι και να κάνεις"),
            vec![
                "Ό".to_string(),
                ",".to_string(),
                "τι".to_string(),
                " ".to_string(),
                "και".to_string(),
                " ".to_string(),
                "να".to_string(),
                " ".to_string(),
                "κάνεις".to_string(),
            ],
            "the jflex literal is lower-case ό,τι only"
        );
    }

    #[test]
    fn java_pin_each_whitespace_char_is_its_own_token() {
        assert_eq!(
            texts("μία  πρόταση"),
            vec![
                "μία".to_string(),
                " ".to_string(),
                " ".to_string(),
                "πρόταση".to_string(),
            ]
        );
        assert_eq!(
            texts("α\tβ"),
            vec!["α".to_string(), "\t".to_string(), "β".to_string()]
        );
    }

    #[test]
    fn java_pin_emails_and_urls_are_joined() {
        assert_eq!(
            texts("στείλε στο user@example.com τώρα"),
            vec![
                "στείλε".to_string(),
                " ".to_string(),
                "στο".to_string(),
                " ".to_string(),
                "user@example.com".to_string(),
                " ".to_string(),
                "τώρα".to_string(),
            ]
        );
        assert_eq!(
            texts("δες https://www.example.com/topic τώρα"),
            vec![
                "δες".to_string(),
                " ".to_string(),
                "https://www.example.com/topic".to_string(),
                " ".to_string(),
                "τώρα".to_string(),
            ]
        );
        assert_eq!(
            texts("δες www.example.com τώρα"),
            vec![
                "δες".to_string(),
                " ".to_string(),
                "www.example.com".to_string(),
                " ".to_string(),
                "τώρα".to_string(),
            ]
        );
    }

    #[test]
    fn token_offsets_reconstruct_the_original_text() {
        for text in [
            "Η τελεία είναι σημείο στίξης.",
            "ό,τι Ό,τι OK; a  b user@example.com",
            "«μία·δύο» Γιώργος’s καλά-κακώς",
        ] {
            let tokens = GreekWordTokenizer::new().tokenize(text);
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
