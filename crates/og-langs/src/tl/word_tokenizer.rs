use og_core::Token;
use og_tokenizer::WordTokenizer;
use regex::Regex;
use std::sync::OnceLock;

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

fn is_tl_delimiter(c: char) -> bool {
    matches!(c,
        '\u{0020}' | '\u{00A0}' | '\u{115F}' | '\u{1160}' | '\u{1680}'
        | '\u{2000}'..='\u{200F}'
        | '\u{2028}'..='\u{202F}'
        | '\u{205F}' | '\u{2060}' | '\u{2061}' | '\u{2062}' | '\u{2063}'
        | '\u{206A}'..='\u{206F}'
        | '\u{3000}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF9}'..='\u{FFFB}'
        | '\u{00A6}' | '\u{2016}' | '\u{2223}' | '|' | ',' | '.' | ';' | '(' | ')' | '['
        | ']' | '{' | '}' | '=' | '*' | '#' | '\u{2217}' | '+' | '\u{00D7}' | '\u{00B7}'
        | '\u{00F7}' | '<' | '>' | '!' | '?' | ':' | '~' | '/' | '\\' | '"' | '\u{00AB}'
        | '\u{00BB}' | '\u{201E}' | '\u{201C}' | '\u{201D}' | '\u{201B}' | '\u{2032}'
        | '\u{203A}' | '\u{2039}' | '\u{2026}' | '\u{00BF}' | '\u{00A1}' | '\u{203C}'
        | '\u{2047}' | '\u{2048}' | '\u{2049}' | '\u{2122}' | '\u{00AE}' | '\u{203D}'
        | '\u{00B6}' | '\u{FFEB}' | '\u{2E2E}'
        | '\'' | '`' | '\u{2019}' | '\u{2018}' | '\u{00B4}'
        | '-'
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
    )
}

#[derive(Clone)]
struct Piece {
    text: String,
    start: usize,
    end: usize,
}

pub struct TagalogWordTokenizer;

impl TagalogWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TagalogWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WordTokenizer for TagalogWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((byte, c)) = chars.next() {
            if is_tl_delimiter(c) {
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
                if is_tl_delimiter(next_char) {
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
        TagalogWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn hyphen_joins_the_base_delimiter_set() {
        assert_eq!(
            tokenized("araw-araw"),
            vec!["araw", "-", "araw"],
            "TagalogWordTokenizer.java:29-31 appends '-' to the core WordTokenizer's \
             TOKENIZING_CHARACTERS, so hyphenated Tagalog reduplications split"
        );
        assert_eq!(
            tokenized("pinaka-prominenteng"),
            vec!["pinaka", "-", "prominenteng"],
            "the SRX test text's compound splits at the hyphen (TagalogSRXSentenceTokenizerTest)"
        );
        assert_eq!(tokenized("e-mail"), vec!["e", "-", "mail"]);
    }

    #[test]
    fn underscores_are_not_delimiters() {
        assert_eq!(
            tokenized("a_b"),
            vec!["a_b"],
            "'_' is absent from TOKENIZING_CHARACTERS (WordTokenizer.java:75) and Tagalog adds \
             only '-', so it stays inside the word"
        );
    }

    #[test]
    fn western_punctuation_and_quotes_are_single_token_delimiters() {
        assert_eq!(tokenized("a,b.c"), vec!["a", ",", "b", ".", "c"]);
        assert_eq!(tokenized("a(b)c"), vec!["a", "(", "b", ")", "c"]);
        assert_eq!(
            tokenized("x!y?z:q"),
            vec!["x", "!", "y", "?", "z", ":", "q"]
        );
        assert_eq!(tokenized("a'b"), vec!["a", "'", "b"]);
    }

    #[test]
    fn consecutive_delimiters_each_keep_their_own_token() {
        assert_eq!(tokenized("a,,b"), vec!["a", ",", ",", "b"]);
        assert_eq!(tokenized("a  b"), vec!["a", " ", " ", "b"]);
        assert_eq!(tokenized("a\t\nb"), vec!["a", "\t", "\n", "b"]);
    }

    #[test]
    fn zwsp_and_bom_are_delimiters_with_their_own_tokens() {
        assert_eq!(
            tokenized("a\u{200B}b"),
            vec!["a", "\u{200B}", "b"],
            "U+200B sits in the \\u2000-\\u200F run of TOKENIZING_CHARACTERS"
        );
        assert_eq!(tokenized("a\u{FEFF}b"), vec!["a", "\u{FEFF}", "b"]);
    }

    #[test]
    fn emails_and_urls_join() {
        assert_eq!(
            tokenized("Sumulat kay juan.delacruz@example.com ngayon."),
            vec![
                "Sumulat",
                " ",
                "kay",
                " ",
                "juan.delacruz@example.com",
                " ",
                "ngayon",
                "."
            ]
        );
        assert_eq!(
            tokenized("Tingnan ang www.example.com ngayon."),
            vec![
                "Tingnan",
                " ",
                "ang",
                " ",
                "www.example.com",
                " ",
                "ngayon",
                "."
            ]
        );
    }

    #[test]
    fn byte_offsets_partition_the_original_text() {
        let text = "Ang magandang babae sa malayong bayan?";
        let tokens = TagalogWordTokenizer::new().tokenize(text);
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
