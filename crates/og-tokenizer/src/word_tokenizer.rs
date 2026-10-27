use crate::WordTokenizer;
use og_core::Token;
use std::sync::Arc;

enum HyphenMode {
    InRuns,
    InnerOnly,
    Delimiter,
}

type EnWordTagged = Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub struct DefaultWordTokenizer {
    english_contractions: bool,
    english_word_chars: bool,
    hyphens: HyphenMode,
    dutch: bool,
    esperanto: bool,
    en_word_tagged: Option<EnWordTagged>,
}

impl DefaultWordTokenizer {
    pub fn new() -> Self {
        Self {
            english_contractions: true,
            english_word_chars: true,
            hyphens: HyphenMode::InnerOnly,
            dutch: false,
            esperanto: false,
            en_word_tagged: None,
        }
    }

    pub fn new_english(en_word_tagged: EnWordTagged) -> Self {
        Self {
            english_contractions: true,
            english_word_chars: true,
            hyphens: HyphenMode::InnerOnly,
            dutch: false,
            esperanto: false,
            en_word_tagged: Some(en_word_tagged),
        }
    }

    pub fn new_generic() -> Self {
        Self {
            english_contractions: false,
            english_word_chars: false,
            hyphens: HyphenMode::InRuns,
            dutch: false,
            esperanto: false,
            en_word_tagged: None,
        }
    }

    pub fn new_splitting_hyphen() -> Self {
        Self {
            english_contractions: false,
            english_word_chars: false,
            hyphens: HyphenMode::Delimiter,
            dutch: false,
            esperanto: false,
            en_word_tagged: None,
        }
    }

    pub fn new_dutch() -> Self {
        Self {
            english_contractions: false,
            english_word_chars: false,
            hyphens: HyphenMode::InRuns,
            dutch: true,
            esperanto: false,
            en_word_tagged: None,
        }
    }

    pub fn new_esperanto() -> Self {
        Self {
            english_contractions: false,
            english_word_chars: false,
            hyphens: HyphenMode::InRuns,
            dutch: false,
            esperanto: true,
            en_word_tagged: None,
        }
    }

    fn is_word_char(&self, c: char) -> bool {
        if self.dutch {
            return !is_nl_delimiter(c);
        }
        if self.esperanto {
            return !is_base_delimiter(c);
        }
        c.is_alphanumeric()
            || c == '_'
            || c == '\''
            || c == '\u{2019}'
            || (self.english_word_chars && is_english_extra_word_char(c))
    }

    fn starts_run(&self, c: char) -> bool {
        self.is_word_char(c) || (matches!(self.hyphens, HyphenMode::InRuns) && c == '-')
    }

    fn continues_run(&self, chars: &[char], i: usize, started: bool) -> bool {
        let c = chars[i];
        if self.is_word_char(c) {
            return true;
        }
        if c != '-' {
            return false;
        }
        match self.hyphens {
            HyphenMode::InRuns => true,
            HyphenMode::InnerOnly => {
                started && i + 1 < chars.len() && self.is_word_char(chars[i + 1])
            }
            HyphenMode::Delimiter => false,
        }
    }

    fn try_consume_url_or_email(
        &self,
        text: &str,
        _chars: &[char],
        _start: usize,
        byte_start: usize,
    ) -> Option<usize> {
        let remaining = &text[byte_start..];

        if remaining.starts_with("http://")
            || remaining.starts_with("https://")
            || remaining.starts_with("ftp://")
        {
            return Some(self.consume_url_like(remaining, byte_start));
        }

        if remaining.starts_with("www.") {
            return Some(self.consume_url_like(remaining, byte_start));
        }

        if let Some(at_pos) = remaining.find('@')
            && at_pos > 0
            && remaining[..at_pos]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '+')
        {
            let domain = &remaining[at_pos + 1..];
            if let Some(slash_pos) = domain.find(|c: char| c.is_whitespace()) {
                let domain_part = &domain[..slash_pos];
                if domain_part.contains('.')
                    && domain_part
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
                {
                    let end = byte_start + at_pos + 1 + slash_pos;
                    return Some(end);
                }
            } else if domain.contains('.')
                && domain
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            {
                return Some(byte_start + remaining.len());
            }
        }

        None
    }

    fn consume_url_like(&self, remaining: &str, byte_start: usize) -> usize {
        let end = remaining
            .find(|c: char| c.is_whitespace())
            .unwrap_or(remaining.len());
        byte_start + end
    }

    fn split_contractions(&self, tokens: Vec<Token>) -> Vec<Token> {
        let mut result = Vec::with_capacity(tokens.len());

        for token in tokens {
            let text = token.text();
            let start = token.start();

            if self.dutch {
                for (from, to) in dutch_quote_pieces(text) {
                    if to > from {
                        result.push(Token::new(&text[from..to], start + from, start + to));
                    }
                }
                continue;
            }

            if !text.contains('\'') && !text.contains('\u{2019}') {
                result.push(token);
                continue;
            }

            if !self.english_contractions {
                let mut segment_start = 0usize;
                let mut offset = 0usize;
                for c in text.chars() {
                    let len = c.len_utf8();
                    if c == '\'' || c == '\u{2019}' {
                        if offset > segment_start {
                            let seg = &text[segment_start..offset];
                            result.push(Token::new(seg, start + segment_start, start + offset));
                        }
                        let apos = &text[offset..offset + len];
                        result.push(Token::new(apos, start + offset, start + offset + len));
                        segment_start = offset + len;
                    }
                    offset += len;
                }
                if offset > segment_start {
                    let seg = &text[segment_start..offset];
                    result.push(Token::new(seg, start + segment_start, start + offset));
                }
                continue;
            }

            let pieces = contraction_pieces(text).unwrap_or_else(|| vec![text.to_string()]);
            let mut piece_offset = 0usize;
            for piece in &pieces {
                self.words_to_add(piece, start + piece_offset, &mut result);
                piece_offset += piece.len();
            }
        }

        result
    }

    fn words_to_add(&self, s: &str, token_start: usize, out: &mut Vec<Token>) {
        let mut rest = s;
        let mut offset = 0usize;
        while let Some(stripped) = rest.strip_prefix('-') {
            out.push(Token::new(
                "-",
                token_start + offset,
                token_start + offset + 1,
            ));
            offset += 1;
            rest = stripped;
        }
        let mut hyphens_at_end = 0usize;
        while let Some(stripped) = rest.strip_suffix('-') {
            rest = stripped;
            hyphens_at_end += 1;
        }
        let body_len = rest.len();
        if !rest.is_empty() {
            let keep_whole =
                if !rest.contains('-') && !rest.contains('\'') && !rest.contains('\u{2019}') {
                    true
                } else {
                    let normalized = rest.replace('\u{00AD}', "").replace('\u{2019}', "'");
                    self.en_word_tagged
                        .as_ref()
                        .is_none_or(|tagged| tagged(&normalized))
                        || CAMEL_CASE_HYPHEN_WORDS
                            .iter()
                            .any(|w| rest.eq_ignore_ascii_case(w))
                };
            if keep_whole {
                out.push(Token::new(
                    rest,
                    token_start + offset,
                    token_start + offset + body_len,
                ));
            } else {
                let mut segment_start = 0usize;
                let mut walk = 0usize;
                for c in rest.chars() {
                    let len = c.len_utf8();
                    if c == '\'' || c == '\u{2019}' {
                        if walk > segment_start {
                            let seg = &rest[segment_start..walk];
                            out.push(Token::new(
                                seg,
                                token_start + offset + segment_start,
                                token_start + offset + walk,
                            ));
                        }
                        let apos = &rest[walk..walk + len];
                        out.push(Token::new(
                            apos,
                            token_start + offset + walk,
                            token_start + offset + walk + len,
                        ));
                        segment_start = walk + len;
                    }
                    walk += len;
                }
                if walk > segment_start {
                    let seg = &rest[segment_start..walk];
                    out.push(Token::new(
                        seg,
                        token_start + offset + segment_start,
                        token_start + offset + walk,
                    ));
                }
            }
        }
        offset += body_len;
        for _ in 0..hyphens_at_end {
            out.push(Token::new(
                "-",
                token_start + offset,
                token_start + offset + 1,
            ));
            offset += 1;
        }
    }

    pub fn split_trailing_hyphens(&self, tokens: Vec<Token>) -> Vec<Token> {
        let mut result = Vec::with_capacity(tokens.len());
        for token in tokens {
            let text = token.text();
            if text.len() > 1 && text.ends_with('-') {
                let start = token.start();
                let hyphen_at = start + text.len() - 1;
                result.push(Token::new(&text[..text.len() - 1], start, hyphen_at));
                result.push(Token::new("-", hyphen_at, hyphen_at + 1));
            } else {
                result.push(token);
            }
        }
        result
    }

    pub fn split_underscores(&self, tokens: Vec<Token>) -> Vec<Token> {
        let mut result = Vec::with_capacity(tokens.len());
        for token in tokens {
            let text = token.text();
            if !text.contains('_') || text == "_" {
                result.push(token);
                continue;
            }
            let start = token.start();
            let mut segment_start = 0usize;
            let mut offset = 0usize;
            for (i, b) in text.bytes().enumerate() {
                if b == b'_' {
                    if i > segment_start {
                        let seg = &text[segment_start..i];
                        result.push(Token::new(seg, start + segment_start, start + i));
                    }
                    result.push(Token::new("_", start + i, start + i + 1));
                    segment_start = i + 1;
                }
                offset = i + 1;
            }
            if offset > segment_start {
                let seg = &text[segment_start..];
                result.push(Token::new(seg, start + segment_start, start + text.len()));
            }
        }
        result
    }

    fn join_variation_selectors(&self, tokens: Vec<Token>) -> Vec<Token> {
        if !self.english_word_chars {
            return tokens;
        }
        let mut result: Vec<Token> = Vec::with_capacity(tokens.len());
        for token in tokens {
            let text = token.text();
            if is_variation_selector(text)
                && let Some(last) = result.last()
            {
                let merged = Token::new(
                    format!("{}{}", last.text(), text),
                    last.start(),
                    last.end() + text.len(),
                );
                if let Some(slot) = result.last_mut() {
                    *slot = merged;
                }
            } else {
                result.push(token);
            }
        }
        result
    }
}

impl Default for DefaultWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

fn is_english_extra_word_char(c: char) -> bool {
    matches!(
        c,
        '±' | '§'
            | '©'
            | '@'
            | '€'
            | '£'
            | '¥'
            | '$'
            | '°'
            | '%'
            | '‰'
            | '‱'
            | '&'
            | '\u{FFFD}'
            | '\u{00AD}'
            | '\u{00AC}'
            | '\u{FF0C}'
            | '\u{FF1F}'
            | '\u{00A8}'
    ) || ('\u{0300}'..='\u{036F}').contains(&c)
}

fn is_variation_selector(text: &str) -> bool {
    matches!(text.as_bytes(), [0xEF, 0xB8, b] if (0x80..=0x8F).contains(b))
}

const P0_WORDS: &[&str] = &[
    "fo'c'sle", "rec'd", "rec's", "ok'd", "cc'd", "cc's", "dj'd", "pm'd", "dm'd", "rsvp'd",
];

const NT_STEMS: &[&str] = &[
    "are", "is", "were", "was", "do", "does", "did", "have", "has", "had", "wo", "would", "ca",
    "could", "sha", "should", "must", "ai", "ought", "might", "need", "may", "am", "dare", "das",
    "dass", "hai", "used", "use",
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
    "al-qaida",
];

fn apostrophe_len(bytes: &[u8]) -> Option<usize> {
    match bytes.first() {
        Some(b'\'') => Some(1),
        Some(0xE2) if bytes.starts_with("\u{2019}".as_bytes()) => Some(3),
        _ => None,
    }
}

fn is_dutch_quote(c: char) -> bool {
    matches!(c, '\'' | '`' | '\u{2019}' | '\u{2018}' | '\u{00B4}')
}

fn is_base_delimiter(c: char) -> bool {
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
    )
}

fn is_nl_delimiter(c: char) -> bool {
    (is_base_delimiter(c) && !is_dutch_quote(c)) || c == '_'
}

fn is_eo_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(
            c,
            'ĉ' | 'ĝ' | 'ĥ' | 'ĵ' | 'ŝ' | 'ŭ' | 'Ĉ' | 'Ĝ' | 'Ĥ' | 'Ĵ' | 'Ŝ' | 'Ŭ'
        )
}

const EO_APOS1: &str = "\u{1}\u{1}EO@APOS1\u{1}\u{1}";
const EO_APOS2: &str = "\u{1}\u{1}EO@APOS2\u{1}\u{1}";

fn eo_prepare_apostrophes(text: &str) -> (String, Vec<usize>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut map: Vec<usize> = Vec::with_capacity(text.len() + 16);
    let mut orig = 0usize;
    let push_mapped = |out: &mut String, map: &mut Vec<usize>, piece: &str, orig: &mut usize| {
        for _ in 0..piece.len() {
            map.push(*orig);
        }
        out.push_str(piece);
        *orig += piece.len();
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '\'' {
            let mut buf = [0u8; 4];
            push_mapped(&mut out, &mut map, c.encode_utf8(&mut buf), &mut orig);
            i += 1;
            continue;
        }
        let mut run_start = i;
        while run_start > 0 && is_eo_letter(chars[run_start - 1]) {
            run_start -= 1;
        }
        let run_len = i - run_start;
        let before = if run_start > 0 {
            Some(chars[run_start - 1])
        } else {
            None
        };
        let boundary_ok =
            before.is_none_or(|b| !(b.is_ascii_alphanumeric() || b == '_') && b != '\'');
        if run_len == 0 || !boundary_ok {
            push_mapped(&mut out, &mut map, "'", &mut orig);
            i += 1;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let piece: &str = match next {
            Some(n) if is_eo_letter(n) || n == '-' => EO_APOS2,
            _ => EO_APOS1,
        };
        for (idx, _) in piece.bytes().enumerate() {
            map.push(orig + usize::from(idx > 0));
        }
        out.push_str(piece);
        orig += 1;
        if matches!(next, Some(n) if is_eo_letter(n) || n == '-') {
            out.push(' ');
            map.push(orig);
        }
        i += 1;
    }
    map.push(orig);
    (out, map)
}

fn eo_restore_apostrophes(tokens: Vec<Token>, map: &[usize]) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut iter = tokens.into_iter();
    while let Some(token) = iter.next() {
        let text = token.text();
        if text.ends_with(EO_APOS2) {
            iter.next();
        }
        let restored = text.replace(EO_APOS1, "'").replace(EO_APOS2, "'");
        let start = map[token.start().min(map.len() - 1)];
        let end = map[token.end().min(map.len() - 1)];
        out.push(Token::new(restored, start, end));
    }
    out
}

fn dutch_quote_pieces(text: &str) -> Vec<(usize, usize)> {
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    if char_indices.len() <= 1 {
        return vec![(0, text.len())];
    }
    let n = char_indices.len();
    let starts_with_quote = is_dutch_quote(char_indices[0].1);
    let ends_with_quote = is_dutch_quote(char_indices[n - 1].1);
    let piece = |from: usize, to: usize| {
        (
            char_indices[from].0,
            char_indices[to].0 + char_indices[to].1.len_utf8(),
        )
    };

    if starts_with_quote && ends_with_quote && n > 2 {
        return vec![piece(0, 0), piece(1, n - 2), piece(n - 1, n - 1)];
    }
    if ends_with_quote {
        let mut stem_end = n;
        while stem_end > 0 && is_dutch_quote(char_indices[stem_end - 1].1) {
            stem_end -= 1;
        }
        let mut pieces = Vec::with_capacity(n - stem_end + 1);
        if stem_end > 0 {
            pieces.push(piece(0, stem_end - 1));
        }
        for i in stem_end..n {
            pieces.push(piece(i, i));
        }
        return pieces;
    }
    if starts_with_quote {
        let mut quote_end = 0usize;
        while quote_end < n && is_dutch_quote(char_indices[quote_end].1) {
            quote_end += 1;
        }
        let mut pieces = Vec::with_capacity(quote_end + 1);
        for i in 0..quote_end {
            pieces.push(piece(i, i));
        }
        if quote_end < n {
            pieces.push(piece(quote_end, n - 1));
        }
        return pieces;
    }
    vec![(0, text.len())]
}

fn starts_with_ascii_ci(hay: &str, needle: &str) -> bool {
    let mut hay_bytes = hay.as_bytes().iter();
    needle.as_bytes().iter().all(|nb| {
        hay_bytes
            .next()
            .is_some_and(|hb| hb.eq_ignore_ascii_case(nb))
    })
}

fn eq_ascii_ci_quote_normalized(hay: &str, word: &str) -> bool {
    let mut word_chars = word.chars();
    for hc in hay.chars() {
        let hc = if hc == '\u{2019}' { '\'' } else { hc };
        match word_chars.next() {
            Some(wc) if hc.eq_ignore_ascii_case(&wc) => {}
            _ => return false,
        }
    }
    word_chars.next().is_none()
}

fn contraction_pieces(text: &str) -> Option<Vec<String>> {
    if P0_WORDS
        .iter()
        .any(|w| eq_ascii_ci_quote_normalized(text, w))
    {
        return Some(vec![text.to_string()]);
    }
    if let Some(pieces) = nt_contraction_pieces(text) {
        return Some(pieces);
    }
    if let Some(pieces) = suffix_contraction_pieces(text) {
        return Some(pieces);
    }
    t_prefix_pieces(text)
}

fn nt_contraction_pieces(text: &str) -> Option<Vec<String>> {
    let mut pieces = Vec::new();
    let mut body = text;
    if let Some(len) = apostrophe_len(text.as_bytes()) {
        pieces.push(text[..len].to_string());
        body = &text[len..];
    }
    for stem in NT_STEMS {
        if body.len() > stem.len() && starts_with_ascii_ci(body, stem) {
            let rest = &body[stem.len()..];
            let bytes = rest.as_bytes();
            if bytes.first().is_some_and(|b| b.eq_ignore_ascii_case(&b'n'))
                && apostrophe_len(&bytes[1..]).is_some_and(|alen| {
                    let t = &bytes[1 + alen..];
                    t.len() == 1 && t[0].eq_ignore_ascii_case(&b't')
                })
            {
                pieces.push(body[..stem.len()].to_string());
                pieces.push(rest.to_string());
                return Some(pieces);
            }
        }
    }
    None
}

fn suffix_contraction_pieces(text: &str) -> Option<Vec<String>> {
    let len = text.len();
    for split in (1..len).rev() {
        if !text.is_char_boundary(split) {
            continue;
        }
        let tail = &text[split..];
        let Some(apos_len) = apostrophe_len(tail.as_bytes()) else {
            continue;
        };
        let after_apos = &tail[apos_len..];
        for (suffix, slen) in [
            ("m", 1),
            ("re", 2),
            ("ll", 2),
            ("ve", 2),
            ("d", 1),
            ("s", 1),
        ] {
            if !starts_with_ascii_ci(after_apos, suffix) {
                continue;
            }
            let rest = &after_apos[slen..];
            if rest.is_empty() {
                return Some(vec![text[..split].to_string(), tail.to_string()]);
            }
            if rest == "'" || rest == "-" || rest == "\u{2019}" {
                return Some(vec![
                    text[..split].to_string(),
                    tail[..tail.len() - rest.len()].to_string(),
                    rest.to_string(),
                ]);
            }
        }
    }
    None
}

fn t_prefix_pieces(text: &str) -> Option<Vec<String>> {
    for tail_word in ["was", "were", "is"] {
        if !text.ends_with(tail_word) || text.eq_ignore_ascii_case(tail_word) {
            continue;
        }
        let head = &text[..text.len() - tail_word.len()];
        let bytes = head.as_bytes();
        if let Some(alen) = apostrophe_len(bytes)
            && bytes.len() == alen + 1
            && bytes[alen].eq_ignore_ascii_case(&b't')
        {
            return Some(vec![head.to_string(), tail_word.to_string()]);
        }
    }
    None
}

impl WordTokenizer for DefaultWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        if text.is_empty() {
            return Vec::new();
        }

        if self.esperanto {
            let (prepared, map) = eo_prepare_apostrophes(text);
            let raw = self.tokenize_base(&prepared);
            return eo_restore_apostrophes(raw, &map);
        }

        let tokens = self.tokenize_base(text);
        self.split_contractions(self.join_variation_selectors(tokens))
    }
}

impl DefaultWordTokenizer {
    fn tokenize_base(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut byte_offset = 0;

        while i < chars.len() {
            let c = chars[i];

            if c.is_whitespace() {
                let start_byte = byte_offset;
                let mut end_byte = byte_offset;
                while i < chars.len() && chars[i].is_whitespace() {
                    end_byte += chars[i].len_utf8();
                    i += 1;
                }
                tokens.push(Token::new(
                    &text[start_byte..end_byte],
                    start_byte,
                    end_byte,
                ));
                byte_offset = end_byte;
            } else if let Some(url_end) =
                self.try_consume_url_or_email(text, &chars, i, byte_offset)
            {
                tokens.push(Token::new(
                    &text[byte_offset..url_end],
                    byte_offset,
                    url_end,
                ));
                while byte_offset < url_end && i < chars.len() {
                    byte_offset += chars[i].len_utf8();
                    i += 1;
                }
                byte_offset = url_end;
            } else if self.starts_run(c) {
                let start_byte = byte_offset;
                let mut end_byte = byte_offset;
                while i < chars.len() && self.continues_run(&chars, i, end_byte > start_byte) {
                    end_byte += chars[i].len_utf8();
                    i += 1;
                }
                tokens.push(Token::new(
                    &text[start_byte..end_byte],
                    start_byte,
                    end_byte,
                ));
                byte_offset = end_byte;
            } else {
                let start_byte = byte_offset;
                let end_byte = byte_offset + c.len_utf8();
                tokens.push(Token::new(
                    &text[start_byte..end_byte],
                    start_byte,
                    end_byte,
                ));
                byte_offset = end_byte;
                i += 1;
            }
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WordTokenizer;

    fn en_tz() -> DefaultWordTokenizer {
        let tagger = og_tagger::english_tagger::shared_english_tagger().expect("shared en tagger");
        DefaultWordTokenizer::new_english(std::sync::Arc::new(move |w: &str| {
            tagger.is_word_tagged(w)
        }))
    }

    #[test]
    fn test_basic_tokenization() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello world");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[1].text(), " ");
        assert_eq!(tokens[2].text(), "world");
    }

    #[test]
    fn test_punctuation() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello, world!");
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[1].text(), ",");
        assert_eq!(tokens[2].text(), " ");
        assert_eq!(tokens[3].text(), "world");
        assert_eq!(tokens[4].text(), "!");
    }

    #[test]
    fn test_offsets() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("ab cd");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 2);
        assert_eq!(tokens[1].start(), 2);
        assert_eq!(tokens[1].end(), 3);
        assert_eq!(tokens[2].start(), 3);
        assert_eq!(tokens[2].end(), 5);
    }

    #[test]
    fn test_apostrophe() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("don't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n't");
    }

    #[test]
    fn test_empty() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_multiple_spaces() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello  world");
        assert_eq!(tokens[1].text(), "  ");
    }

    #[test]
    fn test_unicode() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("café");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "café");
    }

    fn token_texts(tokenizer: &DefaultWordTokenizer, input: &str) -> Vec<String> {
        tokenizer
            .tokenize(input)
            .iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    fn assert_offsets_valid(tokens: &[Token]) {
        for (i, tok) in tokens.iter().enumerate() {
            assert_eq!(
                tok.text().len(),
                tok.end() - tok.start(),
                "Token #{} ({:?}): text byte length != end - start",
                i,
                tok.text()
            );
            if i > 0 {
                let prev = &tokens[i - 1];
                assert_eq!(
                    tok.start(),
                    prev.end(),
                    "Gap or overlap between token #{} ({:?}) and token #{} ({:?})",
                    i - 1,
                    prev.text(),
                    i,
                    tok.text()
                );
            }
        }
    }

    fn assert_reconstruction(tokens: &[Token], original: &str) {
        let reconstructed: String = tokens.iter().map(|t| t.text()).collect();
        assert_eq!(reconstructed, original);
    }

    #[test]
    fn test_basic_two_words() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello world");
        assert_eq!(token_texts(&tz, "Hello world"), vec!["Hello", " ", "world"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello world");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[2].end(), 11);
    }

    #[test]
    fn test_period_at_end() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello.");
        assert_eq!(token_texts(&tz, "Hello."), vec!["Hello", "."]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello.");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
    }

    #[test]
    fn test_multiple_trailing_punctuation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Wow!!!");
        assert_eq!(token_texts(&tz, "Wow!!!"), vec!["Wow", "!", "!", "!"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Wow!!!");
    }

    #[test]
    fn test_comma_separation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello, world");
        assert_eq!(
            token_texts(&tz, "Hello, world"),
            vec!["Hello", ",", " ", "world"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello, world");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[3].start(), 7);
    }

    #[test]
    fn test_comma_no_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a,b");
        assert_eq!(token_texts(&tz, "a,b"), vec!["a", ",", "b"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "a,b");
    }

    #[test]
    fn test_apostrophe_in_contractions() {
        let tz = DefaultWordTokenizer::new();

        let tokens = tz.tokenize("don't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n't");
        assert_offsets_valid(&tokens);

        let tokens = tz.tokenize("it's");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "it");
        assert_eq!(tokens[1].text(), "'s");

        let tokens = tz.tokenize("I'm");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "I");
        assert_eq!(tokens[1].text(), "'m");

        let tokens = tz.tokenize("can't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "ca");
        assert_eq!(tokens[1].text(), "n't");

        let tokens = tz.tokenize("won't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "wo");
        assert_eq!(tokens[1].text(), "n't");

        let tokens = tz.tokenize("they're");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "they");
        assert_eq!(tokens[1].text(), "'re");
    }

    #[test]
    fn test_apostrophe_with_sentence() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("I don't know");
        assert_eq!(
            token_texts(&tz, "I don't know"),
            vec!["I", " ", "do", "n't", " ", "know"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "I don't know");
    }

    #[test]
    fn test_unicode_right_single_quotation_mark() {
        let tz = en_tz();
        let tokens = tz.tokenize("don\u{2019}t");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n\u{2019}t");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_two_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello  world");
        assert_eq!(
            token_texts(&tz, "Hello  world"),
            vec!["Hello", "  ", "world"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello  world");
    }

    #[test]
    fn test_three_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a   b");
        assert_eq!(token_texts(&tz, "a   b"), vec!["a", "   ", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_tabs() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a\tb");
        assert_eq!(token_texts(&tz, "a\tb"), vec!["a", "\t", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_mixed_whitespace() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a \t b");
        assert_eq!(tokens[1].text(), " \t ");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "a \t b");
    }

    #[test]
    fn test_number_at_start() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("42 is the answer");
        assert_eq!(
            token_texts(&tz, "42 is the answer"),
            vec!["42", " ", "is", " ", "the", " ", "answer"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "42 is the answer");
    }

    #[test]
    fn test_number_only() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("12345");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "12345");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_alphanumeric_mixed() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("hello_123");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "hello_123");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_decimal_number() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("3.14");
        assert_eq!(token_texts(&tz, "3.14"), vec!["3", ".", "14"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_number_with_comma() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("1,000");
        assert_eq!(token_texts(&tz, "1,000"), vec!["1", ",", "000"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_exclamation_and_question() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello! How are you?");
        assert_eq!(
            token_texts(&tz, "Hello! How are you?"),
            vec!["Hello", "!", " ", "How", " ", "are", " ", "you", "?"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello! How are you?");
    }

    #[test]
    fn test_semicolon_and_colon() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("yes:no;maybe");
        assert_eq!(
            token_texts(&tz, "yes:no;maybe"),
            vec!["yes", ":", "no", ";", "maybe"]
        );
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_ellipsis_standalone() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("...");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), ".");
        assert_eq!(tokens[1].text(), ".");
        assert_eq!(tokens[2].text(), ".");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "...");
    }

    #[test]
    fn test_hyphenated_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("well-known");
        assert_eq!(token_texts(&tz, "well-known"), vec!["well-known"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "well-known");
    }

    #[test]
    fn test_double_hyphenated() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("state-of-the-art");
        assert_eq!(
            token_texts(&tz, "state-of-the-art"),
            vec!["state-of-the-art"]
        );
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_parentheses_around_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello (world)");
        assert_eq!(
            token_texts(&tz, "Hello (world)"),
            vec!["Hello", " ", "(", "world", ")"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello (world)");
    }

    #[test]
    fn test_parentheses_no_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("(hello)");
        assert_eq!(token_texts(&tz, "(hello)"), vec!["(", "hello", ")"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_nested_parentheses() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("((x))");
        assert_eq!(token_texts(&tz, "((x))"), vec!["(", "(", "x", ")", ")"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_empty_string_returns_no_tokens() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_single_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_single_character() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "a");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 1);
    }

    #[test]
    fn test_only_punctuation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("!?.,;");
        assert_eq!(token_texts(&tz, "!?.,;"), vec!["!", "?", ".", ",", ";"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "!?.,;");
    }

    #[test]
    fn test_at_symbol() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("user@host");
        assert_eq!(token_texts(&tz, "user@host"), vec!["user@host"]);
        assert_offsets_valid(&tokens);
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(
            token_texts(&generic, "user@host"),
            vec!["user", "@", "host"]
        );
    }

    #[test]
    fn test_hash_symbol() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("#tag");
        assert_eq!(token_texts(&tz, "#tag"), vec!["#", "tag"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_dollar_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("$100");
        assert_eq!(token_texts(&tz, "$100"), vec!["$100"]);
        assert_offsets_valid(&tokens);
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(token_texts(&generic, "$100"), vec!["$", "100"]);
    }

    #[test]
    fn test_percent_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("100%");
        assert_eq!(token_texts(&tz, "100%"), vec!["100%"]);
        assert_offsets_valid(&tokens);
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(token_texts(&generic, "100%"), vec!["100", "%"]);
    }

    #[test]
    fn test_ampersand() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("A&B");
        assert_eq!(token_texts(&tz, "A&B"), vec!["A&B"]);
        assert_offsets_valid(&tokens);
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(token_texts(&generic, "A&B"), vec!["A", "&", "B"]);
    }

    #[test]
    fn test_english_word_characters_from_java() {
        let tz = DefaultWordTokenizer::new();
        assert_eq!(token_texts(&tz, "40–45%"), vec!["40", "–", "45%"]);
        assert_eq!(token_texts(&tz, "$12,50"), vec!["$12", ",", "50"]);
        assert_eq!(
            token_texts(&tz, "€5 and £3"),
            vec!["€5", " ", "and", " ", "£3"]
        );
        assert_eq!(token_texts(&tz, "12‰"), vec!["12‰"]);
        assert_eq!(token_texts(&tz, "90°"), vec!["90°"]);
        assert_eq!(token_texts(&tz, "±2"), vec!["±2"]);
        assert_eq!(token_texts(&tz, "§2"), vec!["§2"]);
        assert_eq!(token_texts(&tz, "©2024"), vec!["©2024"]);
        assert_eq!(token_texts(&tz, "Q&A's"), vec!["Q&A", "'s"]);
        assert_eq!(token_texts(&tz, "cafe\u{0301}"), vec!["cafe\u{0301}"]);
        assert_eq!(token_texts(&tz, "a\u{00AD}b"), vec!["a\u{00AD}b"]);
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(token_texts(&generic, "90°"), vec!["90", "°"]);
        assert_eq!(
            token_texts(&generic, "cafe\u{0301}"),
            vec!["cafe", "\u{0301}"]
        );
    }

    #[test]
    fn test_slash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("and/or");
        assert_eq!(token_texts(&tz, "and/or"), vec!["and", "/", "or"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_backslash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a\\b");
        assert_eq!(token_texts(&tz, "a\\b"), vec!["a", "\\", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_caret() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("x^2");
        assert_eq!(token_texts(&tz, "x^2"), vec!["x", "^", "2"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_tilde() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("~test");
        assert_eq!(token_texts(&tz, "~test"), vec!["~", "test"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_pipe() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a|b");
        assert_eq!(token_texts(&tz, "a|b"), vec!["a", "|", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_angle_brackets() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("<html>");
        assert_eq!(token_texts(&tz, "<html>"), vec!["<", "html", ">"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_square_brackets() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("[item]");
        assert_eq!(token_texts(&tz, "[item]"), vec!["[", "item", "]"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_curly_braces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("{key}");
        assert_eq!(token_texts(&tz, "{key}"), vec!["{", "key", "}"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_equals_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("x=1");
        assert_eq!(token_texts(&tz, "x=1"), vec!["x", "=", "1"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_plus_and_asterisk() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("1+2*3");
        assert_eq!(token_texts(&tz, "1+2*3"), vec!["1", "+", "2", "*", "3"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_leading_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(" Hello");
        assert_eq!(token_texts(&tz, " Hello"), vec![" ", "Hello"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, " Hello");
    }

    #[test]
    fn test_trailing_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello ");
        assert_eq!(token_texts(&tz, "Hello "), vec!["Hello", " "]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello ");
    }

    #[test]
    fn test_leading_and_trailing_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("  Hello  ");
        assert_eq!(token_texts(&tz, "  Hello  "), vec!["  ", "Hello", "  "]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "  Hello  ");
    }

    #[test]
    fn test_only_whitespace() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("   ");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "   ");
        assert!(tokens[0].is_whitespace());
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_only_tab() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\t");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "\t");
        assert!(tokens[0].is_whitespace());
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_only_newline() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "\n");
        assert!(tokens[0].is_whitespace());
    }

    #[test]
    fn test_newline_between_words() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("line1\nline2");
        assert_eq!(
            token_texts(&tz, "line1\nline2"),
            vec!["line1", "\n", "line2"]
        );
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_token_is_word_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello, world!");
        assert!(tokens[0].is_word());
        assert!(!tokens[1].is_word());
        assert!(!tokens[2].is_word());
        assert!(tokens[3].is_word());
        assert!(!tokens[4].is_word());
    }

    #[test]
    fn test_token_is_whitespace_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a b");
        assert!(!tokens[0].is_whitespace());
        assert!(tokens[1].is_whitespace());
        assert!(!tokens[2].is_whitespace());
    }

    #[test]
    fn test_token_is_punctuation_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a, b!");
        assert!(!tokens[0].is_punctuation());
        assert!(tokens[1].is_punctuation());
        assert!(!tokens[2].is_punctuation());
        assert!(!tokens[3].is_punctuation());
        assert!(tokens[4].is_punctuation());
    }

    #[test]
    fn test_token_len() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hi  world");
        assert_eq!(tokens[0].len(), 2);
        assert_eq!(tokens[1].len(), 2);
        assert_eq!(tokens[2].len(), 5);
    }

    #[test]
    fn test_unicode_multi_byte_characters() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("café résumé");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "café");
        assert_eq!(tokens[1].text(), " ");
        assert_eq!(tokens[2].text(), "résumé");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "café résumé");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[2].end(), 14);
    }

    #[test]
    fn test_cjk_characters() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("你好");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "你好");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_emoji_separate_from_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hi👍");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "Hi");
        assert_eq!(tokens[1].text(), "👍");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_underscore_in_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("snake_case");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "snake_case");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_underscore_leading() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("_private");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "_private");
    }

    #[test]
    fn test_underscore_only() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("_");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "_");
    }

    #[test]
    fn test_long_sentence() {
        let tz = DefaultWordTokenizer::new();
        let input = "The quick brown fox jumps over the lazy dog.";
        let tokens = tz.tokenize(input);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
        assert_eq!(tokens.last().unwrap().text(), ".");
        assert_eq!(tokens.first().unwrap().text(), "The");
    }

    #[test]
    fn test_complex_mixed_input() {
        let tz = DefaultWordTokenizer::new();
        let input = "Hello, (world)! Don't you agree? Yes—no; maybe...";
        let tokens = tz.tokenize(input);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
    }

    #[test]
    fn test_repeated_punctuation_word_pattern() {
        let tz = DefaultWordTokenizer::new();
        let input = "a.b.c.d";
        let tokens = tz.tokenize(input);
        assert_eq!(
            token_texts(&tz, input),
            vec!["a", ".", "b", ".", "c", ".", "d"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
    }

    #[test]
    fn test_single_period() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(".");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), ".");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 1);
    }

    #[test]
    fn test_single_comma() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(",");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), ",");
    }

    #[test]
    fn test_single_exclamation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("!");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "!");
    }

    #[test]
    fn test_double_quotes() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\"hello\"");
        assert_eq!(token_texts(&tz, "\"hello\""), vec!["\"", "hello", "\""]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_single_quotes_around_word() {
        let tz = en_tz();
        let tokens = tz.tokenize("'hello'");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "'");
        assert_eq!(tokens[1].text(), "hello");
        assert_eq!(tokens[2].text(), "'");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_dutch_tokenizer_matches_java() {
        let tz = DefaultWordTokenizer::new_dutch();
        assert_eq!(
            token_texts(&tz, "De oma's fiets."),
            vec!["De", " ", "oma's", " ", "fiets", "."]
        );
        assert_eq!(
            token_texts(&tz, "'s ochtends"),
            vec!["'", "s", " ", "ochtends"]
        );
        assert_eq!(token_texts(&tz, "oma's'"), vec!["oma's", "'"]);
        assert_eq!(token_texts(&tz, "zo\u{2019}n"), vec!["zo\u{2019}n"]);
        assert_eq!(
            token_texts(&tz, "e-mail adres"),
            vec!["e-mail", " ", "adres"]
        );
        assert_eq!(
            token_texts(&tz, "REMOTE_USER en naam@bedrijf.nl"),
            vec!["REMOTE", "_", "USER", " ", "en", " ", "naam@bedrijf.nl"]
        );
        assert_eq!(token_texts(&tz, "'oma's'"), vec!["'", "oma's", "'"]);
        let tokens = tz.tokenize("De oma's fiets.");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "De oma's fiets.");
    }

    #[test]
    fn test_dutch_tokenizer_matches_java_test_expectations() {
        let tz = DefaultWordTokenizer::new_dutch();
        assert_eq!(
            token_texts(&tz, "This is\u{00A0}a test"),
            vec!["This", " ", "is", "\u{00A0}", "a", " ", "test"]
        );
        assert_eq!(
            token_texts(&tz, "Bla bla oma's bla bla 'test"),
            vec![
                "Bla", " ", "bla", " ", "oma's", " ", "bla", " ", "bla", " ", "'", "test"
            ]
        );
        assert_eq!(
            token_texts(&tz, "Ik zie het''"),
            vec!["Ik", " ", "zie", " ", "het", "'", "'"]
        );
        assert_eq!(
            token_texts(&tz, "''Ik zie het"),
            vec!["'", "'", "Ik", " ", "zie", " ", "het"]
        );
        assert_eq!(
            token_texts(&tz, "Ik 'zie' het"),
            vec!["Ik", " ", "'", "zie", "'", " ", "het"]
        );
        assert_eq!(
            token_texts(&tz, "Ik \u{2018}zie\u{2019} het"),
            vec!["Ik", " ", "\u{2018}", "zie", "\u{2019}", " ", "het"]
        );
        assert_eq!(
            token_texts(&tz, "Ik \u{201C}zie\u{201D} het"),
            vec!["Ik", " ", "\u{201C}", "zie", "\u{201D}", " ", "het"]
        );
        assert_eq!(token_texts(&tz, "'x'"), vec!["'", "x", "'"]);
        assert_eq!(token_texts(&tz, "`x`"), vec!["`", "x", "`"]);
    }

    #[test]
    fn test_esperanto_tokenizer_matches_java() {
        let tz = DefaultWordTokenizer::new_esperanto();
        assert_eq!(token_texts(&tz, "L'espero."), vec!["L'", "espero", "."]);
        assert_eq!(
            token_texts(&tz, "Dank'al vi, mi sukcesis."),
            vec![
                "Dank'", "al", " ", "vi", ",", " ", "mi", " ", "sukcesis", "."
            ]
        );
        assert_eq!(
            token_texts(&tz, "Neni' okazis."),
            vec!["Neni'", " ", "okazis", "."]
        );
        assert_eq!(
            token_texts(&tz, "Mi havas un' fraton."),
            vec!["Mi", " ", "havas", " ", "un'", " ", "fraton", "."]
        );
        assert_eq!(token_texts(&tz, "'citita'"), vec!["'", "citita", "'"]);
        let tokens = tz.tokenize("Dank'al vi");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Dank'al vi");
        assert_eq!((tokens[0].start(), tokens[0].end()), (0, 5));
        assert_eq!((tokens[1].start(), tokens[1].end()), (5, 7));
        assert_eq!((tokens[2].start(), tokens[2].end()), (7, 8));
        assert_eq!((tokens[3].start(), tokens[3].end()), (8, 10));
    }

    #[test]
    fn test_dutch_symbols_join_word_runs_like_java_delimiter_complement() {
        let tz = DefaultWordTokenizer::new_dutch();
        assert_eq!(token_texts(&tz, "$15."), vec!["$15", "."]);
        assert_eq!(token_texts(&tz, "20%"), vec!["20%"]);
        assert_eq!(
            token_texts(&tz, "Apple Watch\u{00A9}"),
            vec!["Apple", " ", "Watch\u{00A9}"]
        );
        assert_eq!(token_texts(&tz, "&euml;"), vec!["&euml", ";"]);
        assert_eq!(
            token_texts(&tz, "be&euml;indigen"),
            vec!["be&euml", ";", "indigen"]
        );
        assert_eq!(
            token_texts(&tz, "€5 en £3"),
            vec!["€5", " ", "en", " ", "£3"]
        );
        assert_eq!(token_texts(&tz, "15:00"), vec!["15", ":", "00"]);
        assert_eq!(token_texts(&tz, "a\u{2013}b"), vec!["a", "\u{2013}", "b"]);
        assert_eq!(token_texts(&tz, "wrd\u{00B2}"), vec!["wrd", "\u{00B2}"]);
        assert_eq!(token_texts(&tz, "mijn.net"), vec!["mijn", ".", "net"]);
    }

    #[test]
    fn test_english_contraction_pipeline_matches_java() {
        let tz = en_tz();
        assert_eq!(token_texts(&tz, "it'S"), vec!["it", "'S"]);
        assert_eq!(token_texts(&tz, "WE'RE"), vec!["WE", "'RE"]);
        assert_eq!(
            token_texts(&tz, "We don'T need this."),
            vec![
                "We", " ", "do", "n", "'", "T", " ", "need", " ", "this", "."
            ]
        );
        assert_eq!(token_texts(&tz, "'We're'"), vec!["'", "We", "'re", "'"]);
        assert_eq!(token_texts(&tz, "'twas"), vec!["'", "t", "was"]);
        assert_eq!(token_texts(&tz, "fo'c'sle"), vec!["fo'c'sle"]);
        assert_eq!(token_texts(&tz, "doin' that"), vec!["doin'", " ", "that"]);
        assert_eq!(
            token_texts(&tz, "ne\u{2019}er e'er o\u{2019}er jack-o'-lantern"),
            vec![
                "ne\u{2019}er",
                " ",
                "e'er",
                " ",
                "o\u{2019}er",
                " ",
                "jack-o'-lantern"
            ]
        );
        assert_eq!(
            token_texts(&tz, "He's sad, but WE'RE happy."),
            vec![
                "He", "'s", " ", "sad", ",", " ", "but", " ", "WE", "'RE", " ", "happy", "."
            ]
        );
        assert_eq!(token_texts(&tz, "We'RE"), vec!["We", "'RE"]);
        let tokens = tz.tokenize("We don'T need this.");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "We don'T need this.");
    }

    #[test]
    fn test_en_dash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("10–20");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "10");
        assert_eq!(tokens[1].text(), "\u{2013}");
        assert_eq!(tokens[2].text(), "20");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_em_dash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("word—another");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "word");
        assert_eq!(tokens[1].text(), "\u{2014}");
        assert_eq!(tokens[2].text(), "another");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_default_trait() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("test");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "test");
    }

    #[test]
    fn test_url_http() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Visit http://example.com today");
        assert_eq!(tokens[0].text(), "Visit");
        assert_eq!(tokens[1].text(), " ");
        assert_eq!(tokens[2].text(), "http://example.com");
        assert_eq!(tokens[3].text(), " ");
        assert_eq!(tokens[4].text(), "today");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_url_https() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("See https://www.example.com/path?q=1 here");
        assert_eq!(tokens[2].text(), "https://www.example.com/path?q=1");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_www_url() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Go to www.example.com now");
        let texts: Vec<&str> = tokens.iter().map(|t| t.text()).collect();
        assert!(
            texts.contains(&"www.example.com"),
            "Expected 'www.example.com' in tokens: {:?}",
            texts
        );
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_email() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Email user@example.com please");
        assert_eq!(tokens[2].text(), "user@example.com");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_url_no_false_positive() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("I like cats and dogs");
        assert!(tokens.iter().all(|t| !t.text().contains('@')));
    }

    #[test]
    fn test_splitting_hyphen_cuts_joined_numbers_and_arabic_punctuation() {
        let tz = DefaultWordTokenizer::new_splitting_hyphen();
        assert_eq!(
            token_texts(&tz, "25-03-2022"),
            vec!["25", "-", "03", "-", "2022"]
        );
        assert_eq!(token_texts(&tz, "قال،أحمد؟"), vec!["قال", "،", "أحمد", "؟"]);
    }

    #[test]
    fn test_crimean_tatar_trailing_hyphen_split() {
        let tz = DefaultWordTokenizer::new_generic();
        let split = |text: &str| -> Vec<String> {
            tz.split_trailing_hyphens(tz.tokenize(text))
                .iter()
                .map(|t| t.text().to_string())
                .collect()
        };
        assert_eq!(split("abc-"), vec!["abc", "-"]);
        assert_eq!(split("-abc"), vec!["-abc"]);
        assert_eq!(split("abc-def"), vec!["abc-def"]);
        assert_eq!(split("a-b-"), vec!["a-b", "-"]);
        assert_eq!(split("--"), vec!["-", "-"]);
        assert_eq!(split(" - "), vec![" ", "-", " "]);
        assert_eq!(split("ab- def"), vec!["ab", "-", " ", "def"]);
    }

    #[test]
    fn test_hyphen_attaches_to_word_runs_at_boundaries() {
        let tz = DefaultWordTokenizer::new_generic();
        assert_eq!(token_texts(&tz, "Кое -кто"), vec!["Кое", " ", "-кто"]);
        assert_eq!(token_texts(&tz, "Кое- кто"), vec!["Кое-", " ", "кто"]);
        assert_eq!(token_texts(&tz, "well- known"), vec!["well-", " ", "known"]);
        assert_eq!(token_texts(&tz, " - "), vec![" ", "-", " "]);
        assert_eq!(token_texts(&tz, "a--b"), vec!["a--b"]);
    }

    #[test]
    fn test_english_fullwidth_comma_and_question_mark_stay_in_word_runs() {
        let tz = DefaultWordTokenizer::new();
        assert_eq!(
            token_texts(&tz, "Be careful\u{FF0C}this is not a standard comma."),
            vec![
                "Be",
                " ",
                "careful\u{FF0C}this",
                " ",
                "is",
                " ",
                "not",
                " ",
                "a",
                " ",
                "standard",
                " ",
                "comma",
                "."
            ]
        );
        assert_eq!(
            token_texts(&tz, "Is he an average person\u{FF1F}"),
            vec![
                "Is",
                " ",
                "he",
                " ",
                "an",
                " ",
                "average",
                " ",
                "person\u{FF1F}"
            ]
        );
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(
            token_texts(&generic, "a\u{FF0C}b"),
            vec!["a", "\u{FF0C}", "b"]
        );
    }

    #[test]
    fn test_english_hyphens_split_off_at_boundaries_like_words_to_add() {
        let tz = DefaultWordTokenizer::new();
        assert_eq!(token_texts(&tz, "break- up"), vec!["break", "-", " ", "up"]);
        assert_eq!(
            token_texts(&tz, "This is great -- ok"),
            vec!["This", " ", "is", " ", "great", " ", "-", "-", " ", "ok"]
        );
        assert_eq!(token_texts(&tz, "well-known"), vec!["well-known"]);
    }

    #[test]
    fn test_diaeresis_is_en_word_char() {
        let tz = DefaultWordTokenizer::new();
        assert_eq!(token_texts(&tz, "a\u{00A8}b"), vec!["a\u{00A8}b"]);
    }

    #[test]
    fn test_p0_curly_apostrophe_variants_stay_whole() {
        let tz = en_tz();
        assert_eq!(
            token_texts(&tz, "fo\u{2019}c\u{2019}sle"),
            vec!["fo\u{2019}c\u{2019}sle"]
        );
        assert_eq!(token_texts(&tz, "rsvp\u{2019}d"), vec!["rsvp\u{2019}d"]);
    }

    #[test]
    fn test_camel_case_hyphen_words_stay_whole() {
        let tz = en_tz();
        assert_eq!(token_texts(&tz, "MERS-CoV"), vec!["MERS-CoV"]);
        assert_eq!(token_texts(&tz, "McGraw-Hill"), vec!["McGraw-Hill"]);
        assert_eq!(token_texts(&tz, "SARS-CoV-2"), vec!["SARS-CoV-2"]);
        assert_eq!(token_texts(&tz, "pH-metres"), vec!["pH-metres"]);
        assert_eq!(token_texts(&tz, "al-Qaida"), vec!["al-Qaida"]);
    }

    #[test]
    fn test_variation_selectors_join_previous_token() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("word\u{26A0}\u{FE0F}!");
        assert_eq!(
            token_texts(&tz, "word\u{26A0}\u{FE0F}!"),
            vec!["word", "\u{26A0}\u{FE0F}", "!"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "word\u{26A0}\u{FE0F}!");
        let generic = DefaultWordTokenizer::new_generic();
        assert_eq!(
            token_texts(&generic, "word\u{26A0}\u{FE0F}"),
            vec!["word", "\u{26A0}", "\u{FE0F}"]
        );
    }

    #[test]
    fn test_leading_and_trailing_hyphens_split_off() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("-word");
        assert_eq!(token_texts(&tz, "-word"), vec!["-", "word"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "-word");

        let tokens = tz.tokenize("word-");
        assert_eq!(token_texts(&tz, "word-"), vec!["word", "-"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "word-");

        let tokens = tz.tokenize("--word--");
        assert_eq!(
            token_texts(&tz, "--word--"),
            vec!["-", "-", "word", "-", "-"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "--word--");
    }

    #[test]
    fn test_split_underscores_separates_the_boundaries() {
        let tz = en_tz();
        let texts = |tokens: &[Token]| -> Vec<String> {
            tokens.iter().map(|t| t.text().to_string()).collect()
        };
        let tokens = tz.split_underscores(tz.tokenize("foo_bar"));
        assert_eq!(texts(&tokens), vec!["foo", "_", "bar"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "foo_bar");

        let tokens = tz.split_underscores(tz.tokenize("foo_"));
        assert_eq!(texts(&tokens), vec!["foo", "_"]);

        let tokens = tz.split_underscores(tz.tokenize("_foo"));
        assert_eq!(texts(&tokens), vec!["_", "foo"]);

        let tokens = tz.split_underscores(tz.tokenize("__"));
        assert_eq!(texts(&tokens), vec!["_", "_"]);

        let tokens = tz.split_underscores(tz.tokenize("plain"));
        assert_eq!(texts(&tokens), vec!["plain"]);

        let tokens = tz.split_underscores(tz.tokenize("_"));
        assert_eq!(texts(&tokens), vec!["_"]);
    }
}
