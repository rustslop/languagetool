use og_core::checker::WordTokenizer;
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use regex::Regex;
use std::sync::{Arc, LazyLock};

const DECIMAL_COMMA_SUBST: char = '\u{E001}';
const NON_BREAKING_SPACE_SUBST: char = '\u{E002}';
const NON_BREAKING_DOT_SUBST: char = '\u{E003}';
const NON_BREAKING_COLON_SUBST: char = '\u{E004}';
const HYPHEN_SUBST_TEXT: &str = "\u{0001}\u{0001}PT_HYPHEN\u{0001}\u{0001}";

static DECIMAL_COMMA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d),(\d)").expect("valid regex"));
static DOTTED_NUMBERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d)\.(\d)").expect("valid regex"));
static COLON_NUMBERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d):(\d)").expect("valid regex"));
static DATES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\d{2}\.\d{2}\.\d{4}|\d{4}\.\d{2}\.\d{2}|\d{4}-\d{2}-\d{2}").expect("valid regex")
});
static DOTTED_ORDINALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d)\.([aoªºᵃᵒ][sˢ]?)").expect("valid regex"));
static HYPHEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\p{L})-([\p{L}\d])").expect("valid regex"));
static NEARBY_HYPHENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\p{L})-(\p{L})-(\p{L})").expect("valid regex"));
static E_MAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"@?\b[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@((\[[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\])|(([a-zA-Z\-0-9]+\.)+[a-zA-Z]{2,}))\b",
    )
    .expect("valid regex")
});
static URL_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[a-zA-ZÄÖÜäöü0-9/%$\\-_.+!*'(),?#~]+").expect("valid regex"));
static DOMAIN_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[a-zA-Z0-9][a-zA-Z0-9-]+").expect("valid regex"));
static CURRENCY_SYMBOLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[A-Z]*[฿₿₵¢₡$₫֏€ƒ₲₴₭₾₺₼₦₱£៛₽₹₪৳₸₮₩¥¤]").expect("valid regex"));
static CURRENCY_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(?:[.,]\d+)*").expect("valid regex"));
static CURRENCY_EXPRESSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "(?:({})({})|({})({}))",
        CURRENCY_SYMBOLS.as_str(),
        CURRENCY_VALUE.as_str(),
        CURRENCY_VALUE.as_str(),
        CURRENCY_SYMBOLS.as_str()
    ))
    .expect("valid regex")
});

const PROTOCOLS: &[&str] = &[
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

const CAMEL_CASE_HYPHEN_WORDS: &[&str] = &[
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
];

static WORD_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    let word_chars = format!(
        "°^\\-\\p{{L}}\\d\\u{{0300}}-\\u{{036F}}\\u{{00A8}}\\u{{2070}}-\\u{{209F}}{DECIMAL_COMMA_SUBST}{NON_BREAKING_DOT_SUBST}{NON_BREAKING_COLON_SUBST}{NON_BREAKING_SPACE_SUBST}{HYPHEN_SUBST_TEXT}"
    );
    let left = "−@€£\\$¢¥¤";
    let right = "€£\\$%‰‱ºªᵃᵒˢ";
    Regex::new(&format!(
        "[{left}]?[{word_chars}]+[{right}]?|[^{word_chars}]"
    ))
    .expect("valid regex")
});

struct CookedChar {
    ch: char,
    orig_start: usize,
}

fn byte_to_char_index(s: &str, byte_index: usize) -> usize {
    s[..byte_index].chars().count()
}

#[derive(Clone)]
struct CookedToken {
    text: String,
    orig_start: usize,
}

pub struct PortugueseWordTokenizer {
    is_word_tagged: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl PortugueseWordTokenizer {
    pub fn new(is_word_tagged: Arc<dyn Fn(&str) -> bool + Send + Sync>) -> Self {
        Self { is_word_tagged }
    }

    fn cook(text: &str) -> Vec<CookedChar> {
        let mut cooked: Vec<CookedChar> = text
            .char_indices()
            .map(|(i, c)| CookedChar {
                ch: c,
                orig_start: i,
            })
            .collect();
        let as_string = |cooked: &[CookedChar]| -> String { cooked.iter().map(|c| c.ch).collect() };

        if text.contains(',') {
            substitute_char(&mut cooked, &DECIMAL_COMMA, DECIMAL_COMMA_SUBST);
        }

        let cooked_text = as_string(&cooked);
        let dot_inside = cooked_text
            .find('.')
            .is_some_and(|dot| dot < cooked_text.len() - 1);
        if dot_inside {
            let text_now = as_string(&cooked);
            for mat in DATES.find_iter(&text_now) {
                let from = byte_to_char_index(&text_now, mat.start());
                let to = byte_to_char_index(&text_now, mat.end());
                for c in &mut cooked[from..to] {
                    if c.ch == '.' {
                        c.ch = NON_BREAKING_DOT_SUBST;
                    }
                }
            }
            substitute_char(&mut cooked, &DOTTED_NUMBERS, NON_BREAKING_DOT_SUBST);
            substitute_char(&mut cooked, &DOTTED_ORDINALS, NON_BREAKING_DOT_SUBST);
        }

        let spaced = as_string(&cooked);
        for (start, end) in spaced_decimal_ranges(spaced.as_str()) {
            for c in &mut cooked[start..end] {
                if matches!(c.ch, ' ' | '\u{00A0}') {
                    c.ch = NON_BREAKING_SPACE_SUBST;
                }
            }
        }

        if cooked.iter().any(|c| c.ch == ':') {
            substitute_char(&mut cooked, &COLON_NUMBERS, NON_BREAKING_COLON_SUBST);
        }
        if cooked.iter().any(|c| c.ch == '-') {
            substitute_hyphens(&mut cooked, &NEARBY_HYPHENS);
            substitute_hyphens(&mut cooked, &HYPHEN);
        }
        cooked
    }

    fn words_to_add(&self, token: &str, orig_start: usize, out: &mut Vec<CookedToken>) {
        let push = |text: &str, offset: usize, out: &mut Vec<CookedToken>| {
            out.push(CookedToken {
                text: text.to_string(),
                orig_start: offset,
            });
        };
        if token.is_empty() {
            return;
        }
        if let Some(caps) = CURRENCY_EXPRESSION.captures(token) {
            let (a, b) = if let (Some(sym), Some(val)) = (caps.get(1), caps.get(2)) {
                (sym.as_str(), val.as_str())
            } else if let (Some(val), Some(sym)) = (caps.get(3), caps.get(4)) {
                (val.as_str(), sym.as_str())
            } else {
                (token, "")
            };
            push(a, orig_start, &mut *out);
            if !b.is_empty() {
                push(b, orig_start + a.len(), &mut *out);
            }
            return;
        }
        if !token.contains('-') {
            push(token, orig_start, &mut *out);
            return;
        }
        let normalized = token.replace('\u{2019}', "'");
        let keep_whole = (self.is_word_tagged)(&normalized)
            || CAMEL_CASE_HYPHEN_WORDS
                .iter()
                .any(|w| token.eq_ignore_ascii_case(w));
        if keep_whole {
            push(token, orig_start, &mut *out);
            return;
        }
        let mut offset = 0usize;
        for (i, piece) in token.split('-').enumerate() {
            if i > 0 {
                push("-", orig_start + offset, &mut *out);
                offset += 1;
            }
            if !piece.is_empty() {
                push(piece, orig_start + offset, &mut *out);
                offset += piece.len();
            }
        }
    }

    fn join_emails_and_urls(&self, tokens: Vec<CookedToken>) -> Vec<CookedToken> {
        self.join_urls(self.join_emails(tokens))
    }

    fn join_emails(&self, tokens: Vec<CookedToken>) -> Vec<CookedToken> {
        let text: String = tokens.iter().map(|t| t.text.as_str()).collect();
        if !text.contains('@') || !E_MAIL.is_match(&text) {
            return tokens;
        }
        let mut result = Vec::with_capacity(tokens.len());
        let mut char_pos = 0usize;
        let mut idx = 0usize;
        while idx < tokens.len() {
            if let Some(mat) = E_MAIL.find_iter(&text[char_pos..]).next() {
                let start = char_pos + mat.start();
                let end = char_pos + mat.end();
                while char_pos < start && idx < tokens.len() {
                    result.push(tokens[idx].clone());
                    char_pos += tokens[idx].text.len();
                    idx += 1;
                }
                if char_pos == start {
                    let mut advance = idx;
                    let mut pos = char_pos;
                    while pos < end && advance < tokens.len() {
                        pos += tokens[advance].text.len();
                        advance += 1;
                    }
                    result.push(CookedToken {
                        text: text[start..end].to_string(),
                        orig_start: tokens[idx].orig_start,
                    });
                    idx = advance;
                    char_pos = pos;
                    continue;
                }
            }
            result.push(tokens[idx].clone());
            char_pos += tokens[idx].text.len();
            idx += 1;
        }
        result
    }

    fn join_urls(&self, tokens: Vec<CookedToken>) -> Vec<CookedToken> {
        let mut result = Vec::with_capacity(tokens.len());
        let mut in_url = false;
        let mut url = String::new();
        let mut url_start = 0usize;
        let mut url_quote: Option<String> = None;
        for i in 0..tokens.len() {
            let token = &tokens[i];
            if self.url_starts_at(i, &tokens) && !in_url {
                in_url = true;
                url_quote = i.checked_sub(1).map(|p| tokens[p].text.clone());
                url_start = token.orig_start;
                url.push_str(&token.text);
            } else if in_url && self.url_ends_at(i, &tokens, url_quote.as_deref()) {
                in_url = false;
                url_quote = None;
                result.push(CookedToken {
                    text: std::mem::take(&mut url),
                    orig_start: url_start,
                });
                result.push(token.clone());
            } else if in_url {
                url.push_str(&token.text);
            } else {
                result.push(token.clone());
            }
        }
        if !url.is_empty() {
            result.push(CookedToken {
                text: url,
                orig_start: url_start,
            });
        }
        result
    }

    fn url_starts_at(&self, i: usize, tokens: &[CookedToken]) -> bool {
        let token = &tokens[i].text;
        if PROTOCOLS.contains(&token.as_str())
            && tokens.len() > i + 3
            && tokens[i + 1].text == ":"
            && tokens[i + 2].text == "/"
            && tokens[i + 3].text == "/"
        {
            return true;
        }
        if tokens.len() > i + 1 && token == "www" && tokens[i + 1].text == "." {
            return true;
        }
        if tokens.len() > i + 3
            && tokens[i + 1].text == "."
            && tokens[i + 3].text == "/"
            && DOMAIN_CHARS.is_match(token)
            && DOMAIN_CHARS.is_match(&tokens[i + 2].text)
        {
            return true;
        }
        tokens.len() > i + 5
            && tokens[i + 1].text == "."
            && tokens[i + 3].text == "."
            && tokens[i + 5].text == "/"
            && DOMAIN_CHARS.is_match(token)
            && DOMAIN_CHARS.is_match(&tokens[i + 2].text)
            && DOMAIN_CHARS.is_match(&tokens[i + 4].text)
    }

    fn url_ends_at(&self, i: usize, tokens: &[CookedToken], url_quote: Option<&str>) -> bool {
        let token = &tokens[i].text;
        let is_ws = token
            .chars()
            .all(|c| c == ' ' || c == '\u{00A0}' || c == '\t' || c == '\n' || c == '\r');
        if is_ws || token == ")" || token == "]" {
            return true;
        }
        if let Some(next) = tokens.get(i + 1) {
            let next_ws = next.text.chars().any(|c| c.is_whitespace());
            let next_breaks = matches!(
                next.text.as_str(),
                "\"" | "»" | "«" | "‘" | "’" | "“" | "”" | "'" | "."
            );
            let token_breaks = matches!(token.as_str(), "." | "," | ";" | ":" | "!" | "?");
            if ((next_ws || next_breaks) && (token_breaks || Some(token.as_str()) == url_quote))
                || !URL_CHARS.is_match(token)
            {
                return true;
            }
        } else if !URL_CHARS.is_match(token) || token == "." || Some(token.as_str()) == url_quote {
            return true;
        }
        false
    }
}

fn spaced_decimal_ranges(s: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let digit = |c: char| c.is_ascii_digit();
    let boundary = |i: usize| i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(';
    let lookahead_ok = |p: usize| p >= n || !digit(chars[p]);
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < n {
        let max_lead = chars[i..].iter().take(3).take_while(|c| digit(**c)).count();
        if max_lead == 0 || !boundary(i) {
            i += 1;
            continue;
        }
        let mut matched = None;
        'leads: for lead in (1..=max_lead).rev() {
            let mut group_ends = vec![i + lead];
            let mut j = i + lead;
            while j + 4 <= n
                && chars[j] == ' '
                && digit(chars[j + 1])
                && digit(chars[j + 2])
                && digit(chars[j + 3])
            {
                j += 4;
                group_ends.push(j);
            }
            for &gend in group_ends.iter().rev() {
                if (chars.get(gend) == Some(&DECIMAL_COMMA_SUBST)
                    || chars.get(gend) == Some(&NON_BREAKING_DOT_SUBST))
                    && gend + 1 < n
                    && digit(chars[gend + 1])
                {
                    let mut fend = gend + 1;
                    while fend < n && digit(chars[fend]) {
                        fend += 1;
                    }
                    matched = Some((i, fend));
                    break 'leads;
                }
                if lookahead_ok(gend) {
                    matched = Some((i, gend));
                    break 'leads;
                }
            }
        }
        if let Some((start, end)) = matched {
            ranges.push((start, end));
            i = end;
        } else {
            i += 1;
        }
    }
    ranges
}

fn substitute_char(cooked: &mut [CookedChar], pattern: &Regex, subst: char) {
    let text: String = cooked.iter().map(|c| c.ch).collect();
    for mat in pattern.find_iter(&text) {
        let from = byte_to_char_index(&text, mat.start());
        let to = byte_to_char_index(&text, mat.end());
        for c in &mut cooked[from..to] {
            if matches!(c.ch, ',' | '.' | ':') {
                c.ch = subst;
            }
        }
    }
}

fn substitute_hyphens(cooked: &mut Vec<CookedChar>, pattern: &Regex) {
    let text: String = cooked.iter().map(|c| c.ch).collect();
    let mut positions = Vec::new();
    for mat in pattern.find_iter(&text) {
        let from = byte_to_char_index(&text, mat.start());
        let to = byte_to_char_index(&text, mat.end());
        for (i, c) in cooked[from..to].iter().enumerate() {
            if c.ch == '-' {
                positions.push(from + i);
            }
        }
    }
    for pos in positions.into_iter().rev() {
        let replacement: Vec<CookedChar> = HYPHEN_SUBST_TEXT
            .chars()
            .map(|ch| CookedChar {
                ch,
                orig_start: cooked[pos].orig_start,
            })
            .collect();
        cooked.splice(pos..pos + 1, replacement);
    }
}

fn is_variation_selector(text: &str) -> bool {
    matches!(text.as_bytes(), [0xEF, 0xB8, b] if (0x80..=0x8F).contains(b))
}

impl WordTokenizer for PortugueseWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        if text.is_empty() {
            return Vec::new();
        }
        let cooked = Self::cook(text);
        let cooked_text: String = cooked.iter().map(|c| c.ch).collect();

        let mut tokens: Vec<CookedToken> = Vec::new();
        for mat in WORD_PATTERN.find_iter(&cooked_text) {
            let range = mat.range();
            if range.is_empty() {
                continue;
            }
            let char_start = byte_to_char_index(&cooked_text, range.start);
            let char_end = byte_to_char_index(&cooked_text, range.end);
            let raw: String = cooked[char_start..char_end].iter().map(|c| c.ch).collect();
            if is_variation_selector(&raw) && tokens.last().is_some() {
                if let Some(last) = tokens.last_mut() {
                    last.text.push_str(&raw);
                }
                continue;
            }
            let token = raw
                .replace(DECIMAL_COMMA_SUBST, ",")
                .replace(NON_BREAKING_COLON_SUBST, ":")
                .replace(NON_BREAKING_SPACE_SUBST, " ")
                .replace(NON_BREAKING_DOT_SUBST, ".")
                .replace(HYPHEN_SUBST_TEXT, "-");
            let orig_start = cooked[char_start].orig_start;
            self.words_to_add(&token, orig_start, &mut tokens);
        }

        let tokens = self.join_emails_and_urls(tokens);
        tokens
            .into_iter()
            .map(|t| {
                let at = AnalyzedToken::new(
                    &t.text,
                    offset + t.orig_start,
                    offset + t.orig_start + t.text.len(),
                );
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}

pub fn shared_portuguese_word_tokenizer() -> Option<Arc<PortugueseWordTokenizer>> {
    static SHARED: LazyLock<Option<Arc<PortugueseWordTokenizer>>> = LazyLock::new(|| {
        let tagger = crate::pt::tagger::PortugueseTagger::shared_arc()?;
        if !tagger.has_dictionary() {
            return None;
        }
        Some(Arc::new(PortugueseWordTokenizer::new(Arc::new(
            move |w: &str| tagger.is_word_tagged(w),
        ))))
    });
    SHARED.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenizer() -> PortugueseWordTokenizer {
        PortugueseWordTokenizer::new(Arc::new(|w: &str| {
            matches!(w, "guarda-chuva" | "bem-vindo" | "MERS-CoV")
        }))
    }

    fn texts(tz: &PortugueseWordTokenizer, input: &str) -> Vec<String> {
        tz.tokenize(input, 0)
            .iter()
            .map(|t| t.token().token().to_string())
            .collect()
    }

    #[test]
    fn keeps_decimal_comma_whole() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "A taxa é de 23,5%."),
            vec!["A", " ", "taxa", " ", "é", " ", "de", " ", "23,5%", "."]
        );
        assert_eq!(texts(&tz, "Paga 23,5‰."), vec!["Paga", " ", "23,5‰", "."]);
    }

    #[test]
    fn keeps_time_colon_whole() {
        let tz = tokenizer();
        assert_eq!(texts(&tz, "Bateu 2:35."), vec!["Bateu", " ", "2:35", "."]);
        assert_eq!(
            texts(&tz, "Vamos às 14:30h."),
            vec!["Vamos", " ", "às", " ", "14:30h", "."]
        );
    }

    #[test]
    fn keeps_dotted_numbers_whole_inside_sentence() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "Às 10.00.43, começou."),
            vec!["Às", " ", "10.00.43", ",", " ", "começou", "."]
        );
        assert_eq!(
            texts(&tz, "Das 10:00 às 10.50."),
            vec!["Das", " ", "10:00", " ", "às", " ", "10.50", "."]
        );
    }

    #[test]
    fn sentence_final_dot_stays_separate() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "Chegou às 8."),
            vec!["Chegou", " ", "às", " ", "8", "."]
        );
    }

    #[test]
    fn keeps_dotted_ordinal_together() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "o 12.a da lista"),
            vec!["o", " ", "12.a", " ", "da", " ", "lista"]
        );
        assert_eq!(
            texts(&tz, "Os 8.ºs colocados."),
            vec!["Os", " ", "8.ºs", " ", "colocados", "."]
        );
        assert_eq!(
            texts(&tz, "Os 1.°ˢ homens."),
            vec!["Os", " ", "1", ".", "°ˢ", " ", "homens", "."]
        );
    }

    #[test]
    fn keeps_spaced_thousands_together() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "1 999 349.56 foi o valor."),
            vec!["1 999 349.56", " ", "foi", " ", "o", " ", "valor", "."]
        );
    }

    #[test]
    fn keeps_dotted_dates_whole() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "Em 31.12.2024, saiu."),
            vec!["Em", " ", "31.12.2024", ",", " ", "saiu", "."]
        );
    }

    #[test]
    fn splits_unknown_hyphenated_word_but_keeps_tagged() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "Escreve-lo amanhã"),
            vec!["Escreve", "-", "lo", " ", "amanhã"]
        );
        assert_eq!(
            texts(&tz, "guarda-chuva novo"),
            vec!["guarda-chuva", " ", "novo"]
        );
        assert_eq!(texts(&tz, "MERS-CoV"), vec!["MERS-CoV"]);
    }

    #[test]
    fn splits_currency_expressions() {
        let tz = tokenizer();
        assert_eq!(texts(&tz, "R$35"), vec!["R$", "35"]);
        assert_eq!(texts(&tz, "35€"), vec!["35", "€"]);
        assert_eq!(texts(&tz, "€12,50"), vec!["€", "12,50"]);
    }

    #[test]
    fn token_offsets_reconstruct_original_text() {
        let tz = tokenizer();
        for input in [
            "A taxa é de 23,5%.",
            "Às 10.00.43, 2:35, 1 999 349.56 e guardo-o amanhã.",
            "Custa $12,50 e 30°C.",
            "Ele escreve-lo às 22:05 de 31.12.2024.",
        ] {
            let tokens = tz.tokenize(input, 0);
            let mut rebuilt = String::new();
            let mut last_end = 0usize;
            for t in &tokens {
                let tok = t.token();
                assert_eq!(tok.start(), last_end, "gap in {input:?} after {rebuilt:?}");
                rebuilt.push_str(tok.token());
                last_end = tok.end();
            }
            assert_eq!(rebuilt, input);
        }
    }

    #[test]
    fn offsets_are_absolute_with_sentence_offset() {
        let tz = tokenizer();
        let tokens = tz.tokenize("23,5%", 10);
        assert_eq!(tokens[0].token().token(), "23,5%");
        assert_eq!(tokens[0].token().start(), 10);
        assert_eq!(tokens[0].token().end(), 15);
    }

    #[test]
    fn joins_email_and_url() {
        let tz = tokenizer();
        assert_eq!(
            texts(&tz, "mail user@example.com agora"),
            vec!["mail", " ", "user@example.com", " ", "agora"]
        );
        assert_eq!(
            texts(&tz, "veja www.example.com agora"),
            vec!["veja", " ", "www.example.com", " ", "agora"]
        );
    }

    #[test]
    fn shared_tokenizer_loads_from_resource_dir() {
        let tz = shared_portuguese_word_tokenizer().expect("portuguese tagger dict present");
        let tokens = tz.tokenize("Paga 23,5‰.", 0);
        let texts: Vec<&str> = tokens.iter().map(|t| t.token().token()).collect();
        assert_eq!(texts, vec!["Paga", " ", "23,5‰", "."]);
    }
}
