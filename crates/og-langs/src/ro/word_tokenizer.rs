use og_core::Token;
use og_tokenizer::WordTokenizer;
use regex::Regex;
use std::sync::OnceLock;

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

fn is_ro_delimiter(c: char) -> bool {
    matches!(c,
        '\u{0020}' | '\u{00A0}' | '\u{115F}' | '\u{1160}' | '\u{1680}'
        | '\u{2000}'..='\u{200F}'
        | '\u{2028}'..='\u{202F}'
        | '\u{205F}' | '\u{2060}'..='\u{2063}' | '\u{206A}'..='\u{206F}'
        | '\u{3000}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF9}'..='\u{FFFB}'
        | ',' | '.' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
        | '!' | '?' | ':' | '"' | '\'' | '\u{2019}' | '\u{2018}' | '\u{201E}' | '\u{201C}'
        | '\u{201D}' | '\u{2026}' | '\\' | '/' | '\t' | '\n' | '\r'
        | '\u{00AB}' | '\u{00BB}' | '<' | '>' | '%' | '\u{00B0}' | '-' | '|' | '='
    )
}

// Java's StringTools.isWhitespace counts U+200B and U+FEFF as whitespace (StringTools.java:558-569)
// but Rust's char::is_whitespace does not, so these delimiters emit no token at all.
fn is_java_only_whitespace_delimiter(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{FEFF}')
}

#[derive(Clone)]
struct Piece {
    text: String,
    start: usize,
    end: usize,
}

pub struct RomanianWordTokenizer;

impl RomanianWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RomanianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WordTokenizer for RomanianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((byte, c)) = chars.next() {
            if is_ro_delimiter(c) {
                if is_java_only_whitespace_delimiter(c) {
                    continue;
                }
                pieces.push(Piece {
                    text: c.to_string(),
                    start: byte,
                    end: byte + c.len_utf8(),
                });
                continue;
            }
            let start = byte;
            let mut end = byte + c.len_utf8();
            while let Some(&(next_byte, next_char)) = chars.peek() {
                if is_ro_delimiter(next_char) {
                    break;
                }
                end = next_byte + next_char.len_utf8();
                chars.next();
            }
            pieces.push(Piece {
                text: text[start..end].to_string(),
                start,
                end,
            });
        }
        join_emails_and_urls(pieces)
            .into_iter()
            .map(|p| Token::new(&p.text, p.start, p.end))
            .collect()
    }
}

fn e_mail_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@((\[[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}])|(([a-zA-Z\-0-9]+\.)+[a-zA-Z]{2,}))",
        )
        .unwrap()
    })
}

fn domain_chars_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9-]+$").unwrap())
}

fn url_chars_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-ZÄÖÜäöü0-9/%$\-_.+!*'(),?#~]+$").unwrap())
}

fn is_java_whitespace_token(token: &str) -> bool {
    token
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '\u{A0}' | '\u{2028}' | '\u{2029}'))
}

fn join_emails(list: Vec<Piece>) -> Vec<Piece> {
    let text: String = list.iter().map(|p| p.text.as_str()).collect();
    if !text.contains('@') {
        return list;
    }
    let mut out: Vec<Piece> = Vec::new();
    let mut current_position = 0usize;
    let mut idx = 0usize;
    for mat in e_mail_re().find_iter(&text) {
        let (start, end) = (mat.start(), mat.end());
        let mut email_slot: Option<usize> = None;
        while current_position < end {
            if current_position < start {
                out.push(list[idx].clone());
            } else if current_position == start {
                email_slot = Some(out.len());
                out.push(Piece {
                    text: mat.as_str().to_string(),
                    start: list[idx].start,
                    end: list[idx].end,
                });
            }
            current_position += list[idx].text.len();
            if let Some(slot) = email_slot {
                out[slot].end = list[idx].end;
            }
            idx += 1;
        }
    }
    if current_position < text.len() {
        out.extend(list[idx..].iter().cloned());
    }
    if out.is_empty() { list } else { out }
}

fn url_starts_at(i: usize, list: &[Piece]) -> bool {
    let token = list[i].text.as_str();
    if PROTOCOLS.contains(&token)
        && list.len() > i + 3
        && list[i + 1].text == ":"
        && list[i + 2].text == "/"
        && list[i + 3].text == "/"
    {
        return true;
    }
    if list.len() > i + 1 && token == "www" && list[i + 1].text == "." {
        return true;
    }
    if list.len() > i + 3
        && list[i + 1].text == "."
        && list[i + 3].text == "/"
        && domain_chars_re().is_match(token)
        && domain_chars_re().is_match(&list[i + 2].text)
    {
        return true;
    }
    list.len() > i + 5
        && list[i + 1].text == "."
        && list[i + 3].text == "."
        && list[i + 5].text == "/"
        && domain_chars_re().is_match(token)
        && domain_chars_re().is_match(&list[i + 2].text)
        && domain_chars_re().is_match(&list[i + 4].text)
}

fn url_ends_at(i: usize, list: &[Piece], url_quote: Option<&str>) -> bool {
    let token = list[i].text.as_str();
    if is_java_whitespace_token(token) || token == ")" || token == "]" {
        return true;
    }
    if list.len() > i + 1 {
        let next_token = list[i + 1].text.as_str();
        let next_boundary = is_java_whitespace_token(next_token)
            || matches!(
                next_token,
                "\"" | "»" | "«" | "‘" | "’" | "“" | "”" | "'" | "."
            );
        let token_punct = matches!(token, "." | "," | ";" | ":" | "!" | "?");
        if (next_boundary && (token_punct || Some(token) == url_quote))
            || !url_chars_re().is_match(token)
        {
            return true;
        }
    } else if !url_chars_re().is_match(token) || token == "." || Some(token) == url_quote {
        return true;
    }
    false
}

fn join_urls(list: Vec<Piece>) -> Vec<Piece> {
    let mut new_list: Vec<Piece> = Vec::new();
    let mut in_url = false;
    let mut url_start = 0usize;
    let mut url_end = 0usize;
    let mut url = String::new();
    let mut url_quote: Option<String> = None;
    for (i, piece) in list.iter().enumerate() {
        if url_starts_at(i, &list) && !in_url {
            in_url = true;
            if i > 0 {
                url_quote = Some(list[i - 1].text.clone());
            }
            url_start = piece.start;
            url_end = piece.end;
            url.push_str(&piece.text);
        } else if in_url && url_ends_at(i, &list, url_quote.as_deref()) {
            in_url = false;
            url_quote = None;
            let joined = std::mem::take(&mut url);
            new_list.push(Piece {
                text: joined,
                start: url_start,
                end: url_end,
            });
            new_list.push(piece.clone());
        } else if in_url {
            url.push_str(&piece.text);
            url_end = piece.end;
        } else {
            new_list.push(piece.clone());
        }
    }
    if !url.is_empty() {
        new_list.push(Piece {
            text: url,
            start: url_start,
            end: url_end,
        });
    }
    new_list
}

fn join_emails_and_urls(list: Vec<Piece>) -> Vec<Piece> {
    join_urls(join_emails(list))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenized(text: &str) -> Vec<String> {
        RomanianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_tokenizer_test_corpus() {
        assert_eq!(
            tokenized("Aceaste mese sunt bune"),
            vec!["Aceaste", " ", "mese", " ", "sunt", " ", "bune"]
        );
        assert_eq!(
            tokenized("Această carte este frumoasă"),
            vec!["Această", " ", "carte", " ", "este", " ", "frumoasă"]
        );
        assert_eq!(
            tokenized("nu-ți doresc"),
            vec!["nu", "-", "ți", " ", "doresc"]
        );
        assert_eq!(tokenized("zicea „merge"), vec!["zicea", " ", "„", "merge"]);
        assert_eq!(
            tokenized("zicea „ merge"),
            vec!["zicea", " ", "„", " ", "merge"]
        );
        assert_eq!(tokenized("zicea merge”"), vec!["zicea", " ", "merge", "”"]);
        assert_eq!(
            tokenized("zicea „merge bine”"),
            vec!["zicea", " ", "„", "merge", " ", "bine", "”"]
        );
        assert_eq!(tokenized("ți-am"), vec!["ți", "-", "am"]);
        assert_eq!(
            tokenized("zicea «merge bine»"),
            vec!["zicea", " ", "«", "merge", " ", "bine", "»"]
        );
    }

    #[test]
    fn romanian_only_delimiters_split() {
        assert_eq!(tokenized("10% din"), vec!["10", "%", " ", "din"]);
        assert_eq!(tokenized("a=b"), vec!["a", "=", "b"]);
        assert_eq!(tokenized("a|b"), vec!["a", "|", "b"]);
        assert_eq!(tokenized("10°C"), vec!["10", "°", "C"]);
        assert_eq!(tokenized("mai<decât"), vec!["mai", "<", "decât"]);
        assert_eq!(tokenized("fără\ttab"), vec!["fără", "\t", "tab"]);
        assert_eq!(tokenized("rând\r\nnou"), vec!["rând", "\r", "\n", "nou"]);
    }

    #[test]
    fn emails_and_urls_join() {
        assert_eq!(
            tokenized("Scrie la ion.popescu@example.com acum."),
            vec![
                "Scrie",
                " ",
                "la",
                " ",
                "ion.popescu@example.com",
                " ",
                "acum",
                "."
            ]
        );
        assert_eq!(
            tokenized("Vezi www.example.com acum."),
            vec!["Vezi", " ", "www.example.com", " ", "acum", "."]
        );
    }

    #[test]
    fn byte_offsets_partition_the_original_text() {
        let text = "Zicea «câte și trei» la 10% din 3=4.";
        let tokens = RomanianWordTokenizer::new().tokenize(text);
        let joined: String = tokens.iter().map(|t| t.text().to_string()).collect();
        assert_eq!(joined, text);
    }
}
