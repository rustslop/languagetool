use crate::WordTokenizer;
use og_core::Token;
use std::sync::Arc;
use std::sync::OnceLock;

fn split_patterns() -> &'static [regex::Regex; 7] {
    static P: OnceLock<[regex::Regex; 7]> = OnceLock::new();
    P.get_or_init(|| {
        let clitics =
            "-ce|-elle|-t-elle|-elles|-t-elles|-en|-il|-t-il|-ils|-t-ils|-je|-la|-le|-les|-leur|-lui|-moi|-nous|-on|-t-on|-toi|-tu|-vous|-vs|-y";
        let elisions = "[cç]['’]|j['’]|n['’]|m['’]|t['’]|s['’]|l['’]|d['’]|qu['’]|jusqu['’]|lorsqu['’]|puisqu['’]|quoiqu['’]";
        [
            regex::Regex::new(
                r"(?i)^(c['’]te?|m['’]as-tu-vu|c['’]est-à-dire|add-on|add-ons|rendez-vous|garde-à-vous|chez-eux|chez-moi|chez-nous|chez-soi|chez-toi|chez-vous)$",
            )
            .unwrap(),
            regex::Regex::new(&format!(r"(?i)^({elisions})([^\-]*)({clitics})$")).unwrap(),
            regex::Regex::new(&format!(r"(?i)^({elisions})([^'’\-].*)$")).unwrap(),
            regex::Regex::new(&format!(r"(?i)^([^\-\d]+)({clitics})({clitics})$")).unwrap(),
            regex::Regex::new(r"(?i)^(.*)(-t|-m)(['’]en|['’]y)$").unwrap(),
            regex::Regex::new(r"(?i)^(.*)(-t-elle|-t-elles|-t-il|-t-ils|-t-on)$").unwrap(),
            regex::Regex::new(&format!(r"(?i)^(.*)({clitics})$")).unwrap(),
        ]
    })
}

const DO_NOT_SPLIT: [&str; 22] = [
    "mers-cov",
    "mcgraw-hill",
    "sars-cov-2",
    "sars-cov",
    "ph-metre",
    "ph-metres",
    "anti-ivg",
    "anti-uv",
    "anti-vih",
    "al-qaïda",
    "c'est-à-dire",
    "add-on",
    "add-ons",
    "rendez-vous",
    "garde-à-vous",
    "chez-eux",
    "chez-moi",
    "chez-nous",
    "chez-soi",
    "chez-toi",
    "chez-vous",
    "m'as-tu-vu",
];

fn is_fr_word_char(c: char) -> bool {
    matches!(c,
        '§' | '©' | '@' | '€' | '£' | '$' | '_' | '\u{300}'..='\u{36F}' | '\u{A8}'
        | '\u{2070}'..='\u{209F}' | '°' | '‰' | '‱' | '&' | '\u{FFFD}' | '\u{AD}' | '\u{AC}')
        || c.is_alphanumeric()
        || c == '-'
}

fn apostrophe_joins(previous: char, next: char) -> bool {
    previous.is_alphabetic()
        && (next.is_alphabetic()
            || next == '1'
            || next == '"'
            || next == '‘'
            || next == '“'
            || next == '«')
}

fn exactly_three_digits(chars: &[char], start: usize) -> bool {
    (0..3).all(|k| chars.get(start + k).is_some_and(|c| c.is_numeric()))
        && !chars.get(start + 3).is_some_and(|c| c.is_numeric())
}

fn boundary_at(chars: &[char], idx: usize) -> bool {
    chars
        .get(idx)
        .is_none_or(|c| !(c.is_alphanumeric() || *c == '_'))
}

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

fn is_whitespace_token(s: &str) -> bool {
    if s == "\u{2}" || s == "\u{1}" {
        return false;
    }
    if s == "\u{FEFF}" {
        return true;
    }
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.chars().count() == 1 {
        let c = trimmed.chars().next().unwrap();
        return s == "\u{200B}" || s == "\u{A0}" || s == "\u{202F}" || c.is_whitespace();
    }
    false
}

fn is_url_chars(token: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-zA-ZÄÖÜäöü0-9/%$-_.+!*'(),?#~]+$").unwrap())
        .is_match(token)
}

fn is_domain_chars(token: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9-]+$").unwrap())
        .is_match(token)
}

fn url_starts_at(i: usize, l: &[Token]) -> bool {
    let token = l[i].text();
    if PROTOCOLS.contains(&token)
        && l.len() > i + 3
        && l[i + 1].text() == ":"
        && l[i + 2].text() == "/"
        && l[i + 3].text() == "/"
    {
        return true;
    }
    if l.len() > i + 1 && token == "www" && l[i + 1].text() == "." {
        return true;
    }
    if l.len() > i + 3
        && l[i + 1].text() == "."
        && l[i + 3].text() == "/"
        && is_domain_chars(token)
        && is_domain_chars(l[i + 2].text())
    {
        return true;
    }
    l.len() > i + 5
        && l[i + 1].text() == "."
        && l[i + 3].text() == "."
        && l[i + 5].text() == "/"
        && is_domain_chars(token)
        && is_domain_chars(l[i + 2].text())
        && is_domain_chars(l[i + 4].text())
}

fn url_ends_at(i: usize, l: &[Token], url_quote: Option<&str>) -> bool {
    let token = l[i].text();
    if is_whitespace_token(token) || token == ")" || token == "]" {
        return true;
    }
    if l.len() > i + 1 {
        let next = l[i + 1].text();
        let next_closes = is_whitespace_token(next)
            || matches!(
                next,
                "\"" | "»" | "«" | "‘" | "’" | "\u{201C}" | "\u{201D}" | "'" | "."
            );
        let token_breaks =
            matches!(token, "." | "," | ";" | ":" | "!" | "?") || Some(token) == url_quote;
        if (next_closes && token_breaks) || !is_url_chars(token) {
            return true;
        }
    } else if !is_url_chars(token) || token == "." || Some(token) == url_quote {
        return true;
    }
    false
}

fn join_emails(tokens: Vec<Token>) -> Vec<Token> {
    let text: String = tokens.iter().map(|t| t.text()).collect();
    if !text.contains('@') {
        return tokens;
    }
    static E_MAIL: OnceLock<fancy_regex::Regex> = OnceLock::new();
    let re = E_MAIL.get_or_init(|| {
        fancy_regex::Regex::new(
            r"@?\b[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@(\[[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\]|(([a-zA-Z\-0-9]+\.)+[a-zA-Z]{2,}))\b",
        )
        .unwrap()
    });
    let mut matches: Vec<(usize, usize)> = Vec::new();
    for m in re.find_iter(&text).flatten() {
        let start = m.start();
        if start > 0 && text.as_bytes().get(start - 1) == Some(&b':') {
            continue;
        }
        matches.push((start, m.end()));
    }
    if matches.is_empty() {
        return tokens;
    }
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut current = 0usize;
    let mut idx = 0usize;
    for (start, end) in matches {
        let mut group_first = usize::MAX;
        while current < end {
            if current < start {
                out.push(tokens[idx].clone());
            } else if current == start {
                group_first = idx;
            }
            current += tokens[idx].text().len();
            idx += 1;
        }
        if group_first != usize::MAX {
            let last = idx - 1;
            out.push(Token::new(
                &text[start..end],
                tokens[group_first].start(),
                tokens[last].end(),
            ));
        }
    }
    if current < text.len() {
        out.extend(tokens[idx..].iter().cloned());
    }
    out
}

fn join_urls(tokens: Vec<Token>) -> Vec<Token> {
    let l = &tokens;
    let mut new: Vec<Token> = Vec::with_capacity(l.len());
    let mut in_url = false;
    let mut url_text = String::new();
    let mut url_quote: Option<&str> = None;
    let mut url_first = 0usize;
    let mut url_last = 0usize;
    for i in 0..l.len() {
        if url_starts_at(i, l) && !in_url {
            in_url = true;
            url_quote = if i > 0 { Some(l[i - 1].text()) } else { None };
            url_first = i;
            url_last = i;
            url_text.push_str(l[i].text());
        } else if in_url && url_ends_at(i, l, url_quote) {
            in_url = false;
            url_quote = None;
            new.push(Token::new(
                &url_text,
                l[url_first].start(),
                l[url_last].end(),
            ));
            url_text.clear();
            new.push(l[i].clone());
        } else if in_url {
            url_text.push_str(l[i].text());
            url_last = i;
        } else {
            new.push(l[i].clone());
        }
    }
    if !url_text.is_empty() {
        new.push(Token::new(
            &url_text,
            l[url_first].start(),
            l[url_last].end(),
        ));
    }
    new
}

fn join_emails_and_urls(tokens: Vec<Token>) -> Vec<Token> {
    join_urls(join_emails(tokens))
}

pub struct FrenchWordTokenizer {
    is_tagged: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl FrenchWordTokenizer {
    pub fn new(is_tagged: Arc<dyn Fn(&str) -> bool + Send + Sync>) -> Self {
        Self { is_tagged }
    }

    fn scan_word_end(chars: &[char], start: usize) -> usize {
        let mut i = start;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            if (c == '\'' || c == '\u{2019}')
                && next.is_some_and(|n| apostrophe_joins(chars[i - 1], n))
            {
                i += if next.is_some_and(|n| n.is_alphanumeric()) {
                    2
                } else {
                    1
                };
            } else if (c == '-' && i > start && next.is_some_and(|n| n.is_alphanumeric()))
                || ((c == '.' || c == ',')
                    && i > start
                    && chars[i - 1].is_numeric()
                    && next.is_some_and(|n| n.is_numeric()))
            {
                i += 2;
            } else if c == ' '
                && i > start
                && chars[i - 1].is_numeric()
                && exactly_three_digits(chars, i + 1)
                && boundary_at(chars, i + 4)
            {
                i += 4;
            } else if is_fr_word_char(c) {
                i += 1;
            } else {
                break;
            }
        }
        i
    }

    fn words_to_add(&self, s: &str, base: usize, out: &mut Vec<(usize, usize)>) {
        if s.is_empty() {
            return;
        }
        if !s.contains('-') {
            out.push((base, base + s.len()));
            return;
        }
        let normalized = s.replace('\u{AD}', "").replace('\u{2019}', "'");
        let lower = s.to_lowercase();
        if (self.is_tagged)(&normalized) || DO_NOT_SPLIT.contains(&lower.as_str()) {
            out.push((base, base + s.len()));
            return;
        }
        let mut offset = 0usize;
        for (idx, part) in s.split('-').enumerate() {
            if idx > 0 {
                let hyphen_at = offset - 1;
                out.push((base + hyphen_at, base + hyphen_at + 1));
            }
            if !part.is_empty() {
                out.push((base + offset, base + offset + part.len()));
                offset += part.len();
            }
            offset += 1;
        }
    }

    fn split_word_token(&self, text: &str, start: usize, end: usize, out: &mut Vec<Token>) {
        let s = &text[start..end];
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut matched = false;
        for p in split_patterns() {
            if let Some(caps) = p.captures(s) {
                matched = true;
                let mut offset = 0usize;
                for i in 1..caps.len() {
                    if let Some(m) = caps.get(i) {
                        let group_text = m.as_str();
                        self.words_to_add(group_text, start + offset, &mut spans);
                        offset += group_text.len();
                    }
                }
                break;
            }
        }
        if !matched {
            self.words_to_add(s, start, &mut spans);
        }
        for (s_start, s_end) in spans {
            if s_end > s_start {
                out.push(Token::new(&text[s_start..s_end], s_start, s_end));
            }
        }
    }
}

impl WordTokenizer for FrenchWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        if text.is_empty() {
            return Vec::new();
        }
        let normalized = text.replace(['\u{2010}', '\u{2011}'], "-");
        let chars: Vec<char> = normalized.chars().collect();
        let mut byte_at: Vec<usize> = Vec::with_capacity(chars.len() + 1);
        let mut b = 0usize;
        for c in &chars {
            byte_at.push(b);
            b += c.len_utf8();
        }
        byte_at.push(b);

        let mut tokens: Vec<Token> = Vec::new();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            let start_byte = byte_at[i];
            if c.is_whitespace() {
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                tokens.push(Token::new(
                    &normalized[start_byte..byte_at[i]],
                    start_byte,
                    byte_at[i],
                ));
            } else if is_fr_word_char(c) {
                let end = Self::scan_word_end(&chars, i);
                let word = &normalized[start_byte..byte_at[end]];
                if word.chars().count() == 1 {
                    tokens.push(Token::new(word, start_byte, byte_at[end]));
                } else {
                    self.split_word_token(&normalized, start_byte, byte_at[end], &mut tokens);
                }
                i = end;
            } else {
                tokens.push(Token::new(
                    &normalized[start_byte..byte_at[i + 1]],
                    start_byte,
                    byte_at[i + 1],
                ));
                i += 1;
            }
        }

        let mut joined: Vec<Token> = Vec::with_capacity(tokens.len());
        for token in tokens {
            let piece = token.text();
            if piece.chars().count() == 1 {
                let cp = piece.chars().next().unwrap() as u32;
                if (0xFE00..=0xFE0F).contains(&cp)
                    && let Some(last) = joined.last_mut()
                    && !last.text().chars().all(|c| c.is_whitespace())
                {
                    let combined = format!("{}{}", last.text(), piece);
                    *last = Token::new(&combined, last.start(), token.end());
                    continue;
                }
            }
            joined.push(token);
        }
        join_emails_and_urls(joined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok() -> FrenchWordTokenizer {
        FrenchWordTokenizer::new(Arc::new(|w: &str| {
            matches!(w, "rendez-vous" | "peut-être" | "aujourd'hui")
        }))
    }

    fn texts(text: &str) -> Vec<String> {
        tok()
            .tokenize(text)
            .iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn elision_splits_with_apostrophe_attached() {
        assert_eq!(
            texts("L'enfant dort."),
            vec!["L'", "enfant", " ", "dort", "."]
        );
    }

    #[test]
    fn elision_and_clitic_split() {
        assert_eq!(
            texts("Est-tu là ?"),
            vec!["Est", "-", "tu", " ", "là", " ", "?"]
        );
    }

    #[test]
    fn hyphenated_word_in_dictionary_stays_whole() {
        assert_eq!(
            texts("un rendez-vous."),
            vec!["un", " ", "rendez-vous", "."]
        );
    }

    #[test]
    fn hyphenated_word_not_in_dictionary_is_split() {
        assert_eq!(texts("mot-chose"), vec!["mot", "-", "chose"]);
    }

    #[test]
    fn decimal_comma_stays_one_token() {
        assert_eq!(
            texts("Il coûte 1,25 euros."),
            vec!["Il", " ", "coûte", " ", "1,25", " ", "euros", "."]
        );
    }

    #[test]
    fn digit_groups_stay_joined_and_years_do_not() {
        assert_eq!(texts("1 000 000 livres"), vec!["1 000 000", " ", "livres"]);
        assert_eq!(texts("en 2024 il"), vec!["en", " ", "2024", " ", "il"]);
    }

    #[test]
    fn typographic_apostrophe_elision() {
        assert_eq!(
            texts("L’enfant dis merci."),
            vec!["L’", "enfant", " ", "dis", " ", "merci", "."]
        );
    }

    #[test]
    fn token_offsets_stay_in_sync() {
        let text = "L'enfant mange, ça coûte 1,25 €.";
        for t in tok().tokenize(text) {
            assert_eq!(&text[t.start()..t.end()], t.text());
        }
    }

    #[test]
    fn email_and_url_are_rejoined() {
        assert_eq!(
            texts("Contactez jean.dupont@exemple.com."),
            vec!["Contactez", " ", "jean.dupont@exemple.com", "."]
        );
        assert_eq!(
            texts("Voir https://exemple.com/page maintenant"),
            vec!["Voir", " ", "https://exemple.com/page", " ", "maintenant"]
        );
        assert_eq!(
            texts("www.exemple.fr/x fonctionne"),
            vec!["www.exemple.fr/x", " ", "fonctionne"]
        );
    }

    #[test]
    fn m_as_tu_vu_is_not_split() {
        assert_eq!(texts("un m'as-tu-vu."), vec!["un", " ", "m'as-tu-vu", "."]);
    }

    #[test]
    fn apostrophe_before_opening_quote_ends_the_word() {
        assert_eq!(texts("l'«mot»"), vec!["l'", "«", "mot", "»"]);
        assert_eq!(texts("l'“mot”"), vec!["l'", "\u{201C}", "mot", "\u{201D}"]);
    }

    #[test]
    fn double_clitic_and_reentrant_clitic() {
        assert_eq!(
            texts("Donnez-le-moi."),
            vec!["Donnez", "-", "le", "-", "moi", "."]
        );
        assert_eq!(texts("Va-t-en."), vec!["Va", "-", "t", "-", "en", "."]);
    }
}
