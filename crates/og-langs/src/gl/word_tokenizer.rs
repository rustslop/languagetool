use og_core::Token;
use og_tokenizer::WordTokenizer;
use regex::Regex;
use std::sync::OnceLock;

const DECIMAL_COMMA_SUBST: char = '\u{E001}';
const NON_BREAKING_SPACE_SUBST: char = '\u{E002}';
const NON_BREAKING_DOT_SUBST: char = '\u{E003}';
const NON_BREAKING_COLON_SUBST: char = '\u{E004}';

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

fn is_gl_delimiter(c: char) -> bool {
    matches!(c,
        '\u{0020}' | '\u{002D}' | '\u{00A0}'
        | '\u{115F}' | '\u{1160}' | '\u{1680}'
        | '\u{2000}'..='\u{200F}'
        | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2028}'..='\u{202F}'
        | '\u{205F}'..='\u{2063}'
        | '\u{206A}'..='\u{206F}'
        | '\u{3000}' | '\u{3164}' | '\u{FEFF}' | '\u{FFA0}'
        | '\u{FFF9}'..='\u{FFFB}'
        | '*' | '+' | '\u{00D7}' | '\u{2217}' | '\u{00B7}' | '\u{00F7}'
        | ':' | '=' | '\u{2260}' | '\u{2242}' | '\u{2243}' | '\u{2244}' | '\u{2245}'
        | '\u{2246}' | '\u{2247}' | '\u{2248}' | '\u{2249}' | '\u{2264}' | '\u{2265}'
        | '\u{226A}' | '\u{226B}' | '\u{2227}' | '\u{2228}' | '\u{2229}' | '\u{222A}'
        | '\u{2208}' | '\u{2209}' | '\u{220A}' | '\u{220B}' | '\u{220C}' | '\u{220D}'
        | ',' | '.' | ';' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}'
        | '\u{00BF}' | '\u{00A1}' | '!' | '?' | '"' | '\u{00AB}' | '\u{00BB}' | '`'
        | '\'' | '\u{2019}' | '\u{2018}' | '\u{201E}' | '\u{201C}' | '\u{201D}'
        | '\u{2026}' | '\\' | '/' | '\t' | '\r' | '\n'
    )
}

fn is_java_regex_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r')
}

#[derive(Clone)]
struct Piece {
    text: String,
    start: usize,
    end: usize,
}

pub struct GalicianWordTokenizer;

impl GalicianWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GalicianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

struct Prepared {
    chars: Vec<char>,
    spans: Vec<(usize, usize)>,
}

fn prepare(text: &str) -> Prepared {
    let original: Vec<(usize, char)> = text.char_indices().collect();
    let mut chars: Vec<char> = Vec::with_capacity(original.len());
    let mut spans: Vec<(usize, usize)> = Vec::with_capacity(original.len());
    let mut i = 0usize;
    while i < original.len() {
        let (byte, c) = original[i];
        let span = (byte, byte + c.len_utf8());
        if c == ','
            && i > 0
            && i + 1 < original.len()
            && original[i - 1].1.is_ascii_digit()
            && original[i + 1].1.is_ascii_digit()
        {
            chars.push(DECIMAL_COMMA_SUBST);
            spans.push(span);
            i += 1;
        } else if c == '.' {
            let protected = i > 0
                && i + 1 < original.len()
                && original[i - 1].1.is_ascii_digit()
                && (original[i + 1].1.is_ascii_digit() || ordinal_suffix_at(&original, i + 1));
            if protected {
                chars.push(NON_BREAKING_DOT_SUBST);
            } else {
                chars.push(c);
            }
            spans.push(span);
            i += 1;
        } else if c == ':'
            && i > 0
            && i + 1 < original.len()
            && original[i - 1].1.is_ascii_digit()
            && original[i + 1].1.is_ascii_digit()
        {
            chars.push(NON_BREAKING_COLON_SUBST);
            spans.push(span);
            i += 1;
        } else if c.is_ascii_digit() && try_consume_date(&original[i..], &mut chars, &mut spans) {
            i += 10;
        } else if c.is_ascii_digit()
            && let Some(end_index) = spaced_decimal_end(&original, i)
        {
            for &(k_byte, k_char) in &original[i..end_index] {
                chars.push(if k_char == ' ' || k_char == '\u{00A0}' {
                    NON_BREAKING_SPACE_SUBST
                } else {
                    k_char
                });
                spans.push((k_byte, k_byte + k_char.len_utf8()));
            }
            i = end_index;
        } else {
            chars.push(c);
            spans.push(span);
            i += 1;
        }
    }
    Prepared { chars, spans }
}

fn ordinal_suffix_at(original: &[(usize, char)], index: usize) -> bool {
    let Some((_, next)) = original.get(index) else {
        return false;
    };
    if !matches!(next, 'a' | 'o' | 'A' | 'O' | 'ª' | 'º') {
        return false;
    }
    match original.get(index + 1) {
        None => true,
        Some((_, after)) => matches!(after, 's' | 'S' | 'ˢ'),
    }
}

fn try_consume_date(
    slice: &[(usize, char)],
    chars: &mut Vec<char>,
    spans: &mut Vec<(usize, usize)>,
) -> bool {
    if slice.len() < 10 {
        return false;
    }
    let digit = |k: usize| slice[k].1.is_ascii_digit();
    let dot = |k: usize| slice[k].1 == '.';
    let dash = |k: usize| slice[k].1 == '-';
    let alt1 = (0..2).all(digit) && dot(2) && (3..5).all(digit) && dot(5) && (6..10).all(digit);
    let alt2 = (0..4).all(digit) && dot(4) && (5..7).all(digit) && dot(7) && (8..10).all(digit);
    let alt3 = (0..4).all(digit) && dash(4) && (5..7).all(digit) && dash(7) && (8..10).all(digit);
    if alt1 {
        for (byte, c) in slice.iter().take(10) {
            chars.push(if *c == '.' {
                NON_BREAKING_DOT_SUBST
            } else {
                *c
            });
            spans.push((*byte, byte + c.len_utf8()));
        }
        true
    } else if alt2 || alt3 {
        let full = (slice[0].0, slice[9].0 + slice[9].1.len_utf8());
        chars.push(NON_BREAKING_DOT_SUBST);
        spans.push(full);
        chars.push(NON_BREAKING_DOT_SUBST);
        spans.push(full);
        true
    } else {
        false
    }
}

fn spaced_decimal_end(original: &[(usize, char)], number_start: usize) -> Option<usize> {
    let len = original.len();
    let boundary = number_start == 0
        || is_java_regex_space(original[number_start - 1].1)
        || original[number_start - 1].1 == '(';
    if !boundary {
        return None;
    }
    let mut digits = 0usize;
    while digits < 3
        && number_start + digits < len
        && original[number_start + digits].1.is_ascii_digit()
    {
        digits += 1;
    }
    if digits == 0 {
        return None;
    }
    for lead in (1..=digits).rev() {
        let mut end = number_start + lead;
        let mut groups = 0usize;
        while end + 4 <= len
            && original[end].1 == ' '
            && original[end + 1].1.is_ascii_digit()
            && original[end + 2].1.is_ascii_digit()
            && original[end + 3].1.is_ascii_digit()
        {
            end += 4;
            groups += 1;
        }
        while groups > 0 {
            if end == len || is_java_regex_space(original[end].1) || original[end].1 == '(' {
                return Some(end);
            }
            groups -= 1;
            end -= 4;
        }
    }
    None
}

impl WordTokenizer for GalicianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let prepared = prepare(text);
        let mut pieces: Vec<Piece> = Vec::new();
        let mut i = 0usize;
        while i < prepared.chars.len() {
            let c = prepared.chars[i];
            if is_gl_delimiter(c) {
                let piece = match c {
                    DECIMAL_COMMA_SUBST => ',',
                    NON_BREAKING_COLON_SUBST => ':',
                    NON_BREAKING_SPACE_SUBST => ' ',
                    NON_BREAKING_DOT_SUBST => '.',
                    other => other,
                };
                let span = prepared.spans[i];
                pieces.push(Piece {
                    text: piece.to_string(),
                    start: span.0,
                    end: span.1,
                });
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            while i < prepared.chars.len() && !is_gl_delimiter(prepared.chars[i]) {
                i += 1;
            }
            let mut piece = String::new();
            for c in &prepared.chars[start..i] {
                piece.push(match *c {
                    DECIMAL_COMMA_SUBST => ',',
                    NON_BREAKING_COLON_SUBST => ':',
                    NON_BREAKING_SPACE_SUBST => ' ',
                    NON_BREAKING_DOT_SUBST => '.',
                    other => other,
                });
            }
            let span_start = prepared.spans[start].0;
            let span_end = prepared.spans[i - 1].1;
            pieces.push(Piece {
                text: piece,
                start: span_start,
                end: span_end,
            });
        }
        let joined = join_emails_and_urls(pieces);
        joined
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
        GalicianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn splits_words_and_punctuation_per_character() {
        assert_eq!(
            tokenized("O gato, que é galego."),
            vec![
                "O", " ", "gato", ",", " ", "que", " ", "é", " ", "galego", "."
            ],
            "StringTokenizer(returnDelims=true) emits each delimiter character as its own \
             token and maximal non-delimiter runs as one token"
        );
    }

    #[test]
    fn hyphen_and_math_symbols_split() {
        assert_eq!(tokenized("porta-abelas"), vec!["porta", "-", "abelas"]);
        assert_eq!(tokenized("3×4"), vec!["3", "×", "4"]);
        assert_eq!(tokenized("a=b"), vec!["a", "=", "b"]);
    }

    #[test]
    fn decimal_comma_and_dotted_numbers_stay_whole() {
        assert_eq!(tokenized("3,5"), vec!["3,5"]);
        assert_eq!(tokenized("1.000.000"), vec!["1.000.000"]);
    }

    #[test]
    fn ordinals_keep_their_dot() {
        assert_eq!(tokenized("1.º"), vec!["1.º"]);
        assert_eq!(tokenized("21.as"), vec!["21.as"]);
    }

    #[test]
    fn dotted_numbers_at_sentence_end_are_not_protected() {
        assert_eq!(
            tokenized("O custo é 3."),
            vec!["O", " ", "custo", " ", "é", " ", "3", "."],
            "the dot is the last character so dotInsideSentence is false and the dot splits"
        );
    }

    #[test]
    fn colon_times_stay_whole() {
        assert_eq!(tokenized("12:25"), vec!["12:25"]);
    }

    #[test]
    fn spaced_decimals_join_into_one_token() {
        assert_eq!(tokenized("2 000 000"), vec!["2 000 000"]);
        assert_eq!(
            tokenized("Custa 15 000 euros."),
            vec!["Custa", " ", "15 000", " ", "euros", "."]
        );
        assert_eq!(tokenized("1234 567"), vec!["1234", " ", "567"]);
    }

    #[test]
    fn dd_mm_yyyy_dates_stay_whole() {
        assert_eq!(tokenized("31.01.2020"), vec!["31.01.2020"]);
    }

    #[test]
    fn yyyy_mm_dd_dates_collapse_to_two_dots_like_java() {
        assert_eq!(
            tokenized("2020.01.31"),
            vec![".."],
            "Java's DATE_PATTERN_REPL only references $1-$3, so the yyyy.mm.dd and \
             yyyy-mm-dd alternatives replace the whole date with two placeholder dots \
             (GalicianWordTokenizer.java:88-91)"
        );
        assert_eq!(tokenized("2020-01-31"), vec![".."]);
    }

    #[test]
    fn urls_and_emails_join() {
        assert_eq!(
            tokenized("Visita https://example.com/x?a=1 agora."),
            vec![
                "Visita",
                " ",
                "https",
                ":",
                "/",
                "/",
                "example.com/x?a",
                "=",
                "1",
                " ",
                "agora",
                "."
            ],
            "the scheme stays split: urlEndsAt closes the url at ':' because ':' is not in \
             URL_CHARS (WordTokenizer.java:44), and '=' ends the joined domain part; the \
             domain tokens join via the mydomain.org/ branch of urlStartsAt"
        );
        assert_eq!(
            tokenized("Escribe a xPersona@example.com."),
            vec!["Escribe", " ", "a", " ", "xPersona@example.com", "."]
        );
        assert_eq!(
            tokenized("Visita www.example.com agora."),
            vec!["Visita", " ", "www.example.com", " ", "agora", "."],
            "www domains join whole"
        );
    }

    #[test]
    fn byte_offsets_partition_the_original_text() {
        let text = "O gato coma 3,5 kg de peixe o 31.01.2020.";
        let tokens = GalicianWordTokenizer::new().tokenize(text);
        let joined: String = tokens.iter().map(|t| t.text().to_string()).collect();
        assert_eq!(joined, text);
        assert_eq!(tokens.last().unwrap().end(), text.len());
    }
}
