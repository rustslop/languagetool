use super::tagger::CatalanTagger;
use og_core::Token;
use regex::Regex;
use std::sync::Arc;

const APOS_RECTE: &str = "xxCA_APOS_RECTExx";
const APOS_RODO: &str = "xxCA_APOS_RODOxx";
const DECIMAL_POINT: &str = "xxCA_DECIMALPOINTxx";
const DECIMAL_COMMA: &str = "xxCA_DECIMALCOMMAxx";
const SPACE: &str = "xxCA_SPACExx";
const SPACE0: &str = "xxCA_SPACE0xx";
const ELA_GEMINADA: &str = "xxELA_GEMINADAxx";
const ELA_GEMINADA_UPPERCASE: &str = "xxELA_GEMINADA_UPPERCASExx";

const PF: &str = "(['\u{2019}]en|['\u{2019}]hi|['\u{2019}]ho|['\u{2019}]l|['\u{2019}]ls|['\u{2019}]m|['\u{2019}]n|['\u{2019}]ns|['\u{2019}]s|['\u{2019}]t|-el|-els|-em|-en|-ens|-hi|-ho|-l|-la|-les|-li|-lo|-los|-m|-me|-n|-ne|-nos|-s|-se|-t|-te|-us|-vos)";

const CAMEL_CASE_HYPHEN_WORDS: [&str; 6] = [
    "mers-cov",
    "mcgraw-hill",
    "sars-cov-2",
    "sars-cov",
    "ph-metre",
    "ph-metres",
];

const CLITIC_HYPHEN_FORMS: [&str; 24] = [
    "-el", "-els", "-em", "-en", "-ens", "-hi", "-ho", "-l", "-la", "-les", "-li", "-lo", "-los",
    "-m", "-me", "-n", "-ne", "-nos", "-s", "-se", "-t", "-te", "-us", "-vos",
];

fn is_dict_atomic_clitic(piece: &str) -> bool {
    let mut chars = piece.chars();
    let letter_ok = matches!(chars.next(), Some(c) if "lnmtsdLNMTSD".contains(c));
    let apos_ok = matches!(chars.next(), Some('\'') | Some('\u{2019}'));
    letter_ok && apos_ok && chars.next().is_none()
        || CLITIC_HYPHEN_FORMS
            .iter()
            .any(|form| piece.eq_ignore_ascii_case(form))
}

#[derive(Clone)]
struct AuxChar {
    ch: char,
    orig_start: usize,
    orig_end: usize,
}

fn is_ca_word_char(c: char) -> bool {
    matches!(
        c,
        '§' | '©'
            | '@'
            | '€'
            | '£'
            | '$'
            | '_'
            | '·'
            | '-'
            | '\u{00A8}'
            | '\u{2070}'..='\u{209F}'
            | '°'
            | '%'
            | '‰'
            | '‱'
            | '&'
            | '\u{FFFD}'
            | '\u{00AD}'
            | '\u{00AC}'
            | '\u{0300}'..='\u{036F}'
    ) || c.is_alphabetic()
        || c.is_ascii_digit()
}

#[derive(Clone, Copy)]
enum Segment {
    Group(usize),
    Marker,
}

struct Replacer {
    re: Regex,
    marker: &'static str,
    segments: &'static [Segment],
}

fn replacers() -> &'static [Replacer] {
    static REPLACERS: std::sync::OnceLock<Vec<Replacer>> = std::sync::OnceLock::new();
    REPLACERS.get_or_init(|| {
        const BETWEEN: [Segment; 3] = [Segment::Group(1), Segment::Marker, Segment::Group(2)];
        const SPACE0_SEGS: [Segment; 2] = [Segment::Group(1), Segment::Marker];
        const SPACE2_SEGS: [Segment; 5] = [
            Segment::Group(1),
            Segment::Marker,
            Segment::Group(2),
            Segment::Marker,
            Segment::Group(3),
        ];
        let build = |pattern: &str, marker: &'static str, segments: &'static [Segment]| Replacer {
            re: Regex::new(pattern).unwrap(),
            marker,
            segments,
        };
        let vowel = "[aeiouàéèíóòúïüAEIOUÀÈÉÍÒÓÚÏÜ]";
        let ela_class = "[\\.\\u{2022}\\u{22C5}\\u{2219}\\u{F0D7}]";
        vec![
            build(
                &format!("({vowel})l{ela_class}l({vowel})"),
                ELA_GEMINADA,
                &BETWEEN,
            ),
            build(
                &format!("([AEIOUÀÈÉÍÒÓÚÏÜ])L{ela_class}L([AEIOUÀÈÉÍÒÓÚÏÜ])"),
                ELA_GEMINADA_UPPERCASE,
                &BETWEEN,
            ),
            build("([\\p{L}])'([\\p{L}\"‘“«])", APOS_RECTE, &BETWEEN),
            build("([dlDL])'(\\d[\\d\\s\\.,]?)", APOS_RECTE, &BETWEEN),
            build("([\\p{L}])\u{2019}([\\p{L}\"‘“«])", APOS_RODO, &BETWEEN),
            build("([dlDL])\u{2019}(\\d[\\d\\s\\.,]?)", APOS_RODO, &BETWEEN),
            build("(\\d)\\.(\\d)", DECIMAL_POINT, &BETWEEN),
            build("(\\d),(\\d)", DECIMAL_COMMA, &BETWEEN),
            build("(\\d{4}) ", SPACE0, &SPACE0_SEGS),
            build("(\\d) (\\d\\d\\d) (\\d\\d\\d)", SPACE, &SPACE2_SEGS),
            build("(\\d) (\\d\\d\\d)", SPACE, &BETWEEN),
        ]
    })
}

fn starts_marker(aux: &[AuxChar], from: usize, marker: &str) -> bool {
    for (i, marker_char) in (from..).zip(marker.chars()) {
        if aux.get(i).is_none_or(|a| a.ch != marker_char) {
            return false;
        }
    }
    true
}

fn marker_len(aux: &[AuxChar], from: usize) -> usize {
    for marker in [
        APOS_RECTE,
        APOS_RODO,
        DECIMAL_POINT,
        DECIMAL_COMMA,
        SPACE,
        SPACE0,
        ELA_GEMINADA,
        ELA_GEMINADA_UPPERCASE,
    ] {
        if starts_marker(aux, from, marker) {
            return marker.chars().count();
        }
    }
    1
}

fn restore_marker_at(aux: &[AuxChar], i: usize) -> Option<&'static str> {
    if aux.get(i).map(|a| a.ch) != Some('x') {
        return None;
    }
    if starts_marker(aux, i, APOS_RECTE) {
        Some("'")
    } else if starts_marker(aux, i, APOS_RODO) {
        Some("\u{2019}")
    } else if starts_marker(aux, i, DECIMAL_POINT) {
        Some(".")
    } else if starts_marker(aux, i, DECIMAL_COMMA) {
        Some(",")
    } else if starts_marker(aux, i, SPACE) {
        Some(" ")
    } else if starts_marker(aux, i, ELA_GEMINADA) {
        Some("l.l")
    } else if starts_marker(aux, i, ELA_GEMINADA_UPPERCASE) {
        Some("L.L")
    } else {
        None
    }
}

struct Restored {
    text: String,
    disp_char_to_aux: Vec<(usize, usize)>,
}

fn restore(aux: &[AuxChar], from: usize, to: usize) -> Restored {
    let mut text = String::with_capacity(to - from);
    let mut disp_char_to_aux = Vec::with_capacity(to - from);
    let mut i = from;
    while i < to {
        if let Some(restored) = restore_marker_at(aux, i) {
            text.push_str(restored);
            let end = i + marker_len(aux, i);
            disp_char_to_aux.push((i, end));
            i = end;
        } else {
            text.push(aux[i].ch);
            disp_char_to_aux.push((i, i + 1));
            i += 1;
        }
    }
    Restored {
        text,
        disp_char_to_aux,
    }
}

fn build_aux(text: &str) -> Vec<AuxChar> {
    let mut aux: Vec<AuxChar> = Vec::with_capacity(text.len());
    let mut offset = 0usize;
    for c in text.chars() {
        let mapped = match c {
            '\u{2010}' | '\u{2011}' => '-',
            '\u{02BC}' => '\u{2019}',
            other => other,
        };
        aux.push(AuxChar {
            ch: mapped,
            orig_start: offset,
            orig_end: offset + c.len_utf8(),
        });
        offset += c.len_utf8();
    }
    for replacer in replacers() {
        let mut current: Vec<AuxChar> = Vec::with_capacity(aux.len() + 16);
        let mut snapshot = String::with_capacity(aux.iter().map(|a| a.ch.len_utf8()).sum());
        for a in &aux {
            snapshot.push(a.ch);
        }
        let char_at = |snapshot: &str, byte: usize| snapshot[..byte].chars().count();
        let mut last = 0usize;
        for caps in replacer.re.captures_iter(&snapshot) {
            let whole = caps.get(0).unwrap();
            let whole_start = char_at(&snapshot, whole.start());
            let whole_end = whole_start + snapshot[whole.start()..whole.end()].chars().count();
            current.extend_from_slice(&aux[last..whole_start]);
            let group_aux_range = |group_index: usize| -> Option<(usize, usize)> {
                caps.get(group_index).map(|group| {
                    let group_start = char_at(&snapshot, group.start());
                    let group_end =
                        group_start + snapshot[group.start()..group.end()].chars().count();
                    (group_start, group_end)
                })
            };
            let mut aux_cursor = whole_start;
            for (position, segment) in replacer.segments.iter().enumerate() {
                match *segment {
                    Segment::Group(group_index) => {
                        if let Some((group_start, group_end)) = group_aux_range(group_index) {
                            current.extend_from_slice(&aux[group_start..group_end]);
                            aux_cursor = group_end;
                        }
                    }
                    Segment::Marker => {
                        let next_group_start = replacer.segments[position + 1..]
                            .iter()
                            .find_map(|s| match *s {
                                Segment::Group(g) => group_aux_range(g).map(|(start, _)| start),
                                Segment::Marker => None,
                            })
                            .unwrap_or(whole_end);
                        let marker: Vec<char> = replacer.marker.chars().collect();
                        for (index, marker_char) in marker.iter().enumerate() {
                            let is_last = index == marker.len() - 1;
                            current.push(AuxChar {
                                ch: *marker_char,
                                orig_start: aux[aux_cursor].orig_start,
                                orig_end: if is_last {
                                    aux[next_group_start - 1].orig_end
                                } else {
                                    aux[aux_cursor].orig_start
                                },
                            });
                        }
                        aux_cursor = next_group_start;
                    }
                }
            }
            last = whole_end;
        }
        current.extend_from_slice(&aux[last..]);
        aux = current;
    }
    let mut collapsed: Vec<AuxChar> = Vec::with_capacity(aux.len());
    let mut i = 0usize;
    while i < aux.len() {
        if aux[i].ch == 'x' && starts_marker(&aux, i, SPACE0) {
            let end = i + SPACE0.chars().count();
            collapsed.push(AuxChar {
                ch: ' ',
                orig_start: aux[i].orig_start,
                orig_end: aux[end - 1].orig_end,
            });
            i = end;
        } else {
            collapsed.push(AuxChar {
                ch: aux[i].ch,
                orig_start: aux[i].orig_start,
                orig_end: aux[i].orig_end,
            });
            i += 1;
        }
    }
    collapsed
}

fn patterns() -> &'static [Regex] {
    static PATTERNS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        let flags = "(?i)";
        vec![
            Regex::new(&format!("{flags}^([lnmtsd]['\u{2019}])([^'\u{2019}\\-]*)$")).unwrap(),
            Regex::new(&format!(
                "{flags}^(qui-sap-lo|qui-sap-la|qui-sap-los|qui-sap-les)|(Castella)(-)(la)$"
            ))
            .unwrap(),
            Regex::new(&format!(
                "{flags}^([lnmtsd]['\u{2019}])(.{{2,}}){PF}{PF}{PF}$"
            ))
            .unwrap(),
            Regex::new(&format!("{flags}^(.{{2,}}){PF}{PF}{PF}$")).unwrap(),
            Regex::new(&format!("{flags}^([lnmtsd]['\u{2019}])(.{{2,}}){PF}{PF}$")).unwrap(),
            Regex::new(&format!("{flags}^(.{{2,}}){PF}{PF}$")).unwrap(),
            Regex::new(&format!("{flags}^([lnmtsd]['\u{2019}])(.{{2,}}){PF}$")).unwrap(),
            Regex::new(&format!("{flags}^(.+[^wo]){PF}$")).unwrap(),
            Regex::new(&format!("{flags}^([lnmtsd]['\u{2019}])(.*)$")).unwrap(),
        ]
    })
}

fn hyphen_l() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new("(?i)^([\\p{L}]+)(-)([Ll]['\u{2019}])([\\p{L}]+)$").unwrap())
}

type Piece = (String, usize, usize);

pub struct CatalanWordTokenizer {
    tagger: Arc<CatalanTagger>,
}

impl CatalanWordTokenizer {
    pub fn new() -> Self {
        Self {
            tagger: CatalanTagger::shared_arc().unwrap_or_else(|| {
                Arc::new(CatalanTagger::from_resource_dir(std::path::Path::new(
                    "/nonexistent-ca",
                )))
            }),
        }
    }

    fn words_to_add(
        &self,
        piece: &str,
        disp_from: usize,
        disp_to: usize,
        dmap: &[(usize, usize)],
        out: &mut Vec<Piece>,
    ) {
        if piece.is_empty() {
            return;
        }
        let push = |text: String, from: usize, to: usize, out: &mut Vec<Piece>| {
            let aux_from = dmap[from.min(dmap.len() - 1)].0;
            let aux_to = dmap[(to - 1).min(dmap.len() - 1)].1;
            out.push((text, aux_from, aux_to));
        };
        if !piece.contains('-') && !piece.ends_with('\'') && !piece.ends_with('\u{2019}') {
            push(piece.to_string(), disp_from, disp_to, out);
            return;
        }
        let normalized = piece.replace('\u{00AD}', "").replace('\u{2019}', "'");
        if self.tagger.is_word_tagged(&normalized) {
            push(piece.to_string(), disp_from, disp_to, out);
            return;
        }
        if CAMEL_CASE_HYPHEN_WORDS
            .iter()
            .any(|w| piece.eq_ignore_ascii_case(w))
        {
            push(piece.to_string(), disp_from, disp_to, out);
            return;
        }
        let ela_fixed = piece.replace('\u{00AD}', "").replace("l-l", "l\u{b7}l");
        if self.tagger.is_word_tagged(&ela_fixed) {
            push(piece.to_string(), disp_from, disp_to, out);
            return;
        }
        if is_dict_atomic_clitic(piece) {
            push(piece.to_string(), disp_from, disp_to, out);
            return;
        }
        if (piece.ends_with('\'') || piece.ends_with('\u{2019}')) && piece.chars().count() > 1 {
            let stem: String = piece.chars().take(piece.chars().count() - 1).collect();
            let stem_to = disp_to - 1;
            self.words_to_add(&stem, disp_from, stem_to, dmap, out);
            let last = piece.chars().last().unwrap();
            push(last.to_string(), disp_to - 1, disp_to, out);
            return;
        }
        if let Some(caps) = hyphen_l().captures(piece) {
            let mut cursor = 0usize;
            for group_index in 1..=caps.len() {
                if let Some(group) = caps.get(group_index) {
                    let group_text = group.as_str().to_string();
                    let group_chars = group_text.chars().count();
                    let group_from = disp_from + cursor;
                    let group_to = group_from + group_chars;
                    self.words_to_add(&group_text, group_from, group_to, dmap, out);
                    cursor += group_chars;
                }
            }
            return;
        }
        let mut cursor = disp_from;
        let mut segment: String = String::new();
        let mut segment_start = disp_from;
        for c in piece.chars() {
            if c == '-' {
                if !segment.is_empty() {
                    push(std::mem::take(&mut segment), segment_start, cursor, out);
                }
                push("-".to_string(), cursor, cursor + 1, out);
                cursor += 1;
                segment_start = cursor;
            } else {
                segment.push(c);
                cursor += 1;
            }
        }
        if !segment.is_empty() {
            push(segment, segment_start, cursor, out);
        }
    }
}

impl Default for CatalanWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_tokenizer::WordTokenizer for CatalanWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let aux = build_aux(text);
        let mut i = 0usize;
        let mut pieces: Vec<Piece> = Vec::new();
        while i < aux.len() {
            let from = i;
            let to = if is_ca_word_char(aux[i].ch) {
                while i < aux.len() && is_ca_word_char(aux[i].ch) {
                    i += 1;
                }
                i
            } else {
                i += 1;
                i
            };
            let restored = restore(&aux, from, to);
            let disp = restored.text;
            let dmap = &restored.disp_char_to_aux;
            if disp.chars().count() == 1 {
                let only = disp.chars().next().unwrap();
                if ('\u{FE00}'..='\u{FE0F}').contains(&only)
                    && let Some(last) = pieces.last_mut()
                    && !dmap.is_empty()
                {
                    let aux_index = dmap[0].0;
                    if aux_index == from {
                        last.0.push(only);
                        last.2 = to;
                        continue;
                    }
                }
            }
            let mut s: String = disp.clone();
            let mut disp_from = 0usize;
            let mut disp_to = disp.chars().count();
            while s.chars().count() > 1 && s.starts_with('-') {
                pieces.push(("-".to_string(), dmap[disp_from].0, dmap[disp_from].1));
                s = s.chars().skip(1).collect();
                disp_from += 1;
            }
            let mut hyphens_at_end = 0usize;
            while s.chars().count() > 1 && s.ends_with('-') {
                s.pop();
                hyphens_at_end += 1;
            }
            disp_to -= hyphens_at_end;
            let mut contraction_handled = false;
            if s.contains('\'') || s.contains('\u{2019}') || s.contains('-') {
                for pattern in patterns() {
                    if let Some(caps) = pattern.captures(&s) {
                        let mut cursor = 0usize;
                        for group_index in 1..=caps.len() {
                            if let Some(group) = caps.get(group_index) {
                                let group_text = group.as_str().to_string();
                                let group_chars = group_text.chars().count();
                                let group_from = disp_from + cursor;
                                let group_to = group_from + group_chars;
                                self.words_to_add(
                                    &group_text,
                                    group_from,
                                    group_to,
                                    dmap,
                                    &mut pieces,
                                );
                                cursor += group_chars;
                            }
                        }
                        contraction_handled = true;
                        break;
                    }
                }
            } else if s.chars().count() <= 4 {
                let lower = s.to_lowercase();
                let split_index = if lower == "al" || lower == "als" {
                    1
                } else if lower == "del"
                    || lower == "dels"
                    || lower == "pel"
                    || lower == "pels"
                    || lower == "can"
                {
                    2
                } else {
                    0
                };
                if split_index > 0 && !s.is_empty() {
                    let head: String = s.chars().take(split_index).collect();
                    let tail: String = s.chars().skip(split_index).collect();
                    let head_from = dmap[disp_from].0;
                    let head_to = dmap[disp_from + split_index - 1].1;
                    let tail_from = dmap[disp_from + split_index].0;
                    let tail_to = dmap[disp_to - 1].1;
                    pieces.push((head, head_from, head_to));
                    pieces.push((tail, tail_from, tail_to));
                    contraction_handled = true;
                }
            }
            if !contraction_handled {
                self.words_to_add(&s, disp_from, disp_to, dmap, &mut pieces);
            }
            while hyphens_at_end > 0 {
                pieces.push((
                    "-".to_string(),
                    dmap[disp_to.min(dmap.len() - 1)].0,
                    dmap[disp_to.min(dmap.len() - 1)].1,
                ));
                disp_to += 1;
                hyphens_at_end -= 1;
            }
        }
        join_emails_and_urls(pieces)
            .into_iter()
            .map(|(text, aux_from, aux_to)| {
                let start = aux[aux_from].orig_start;
                let end = aux[(aux_to - 1).min(aux.len() - 1)].orig_end.max(start);
                Token::new(text, start, end)
            })
            .collect()
    }
}

fn email_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            "@?\\b[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@((\\[[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\])|(([a-zA-Z\\-0-9]+\\.)+[a-zA-Z]{2,}))\\b",
        )
        .unwrap()
    })
}

fn url_chars_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new("^[a-zA-ZÄÖÜäöü0-9/$-_+!*'(),?#~]+$").unwrap())
}

fn domain_chars_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new("^[a-zA-Z0-9][a-zA-Z0-9\\-]+$").unwrap())
}

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

fn join_emails_and_urls(pieces: Vec<Piece>) -> Vec<Piece> {
    join_urls(join_emails(pieces))
}

fn join_emails(pieces: Vec<Piece>) -> Vec<Piece> {
    if !pieces.iter().any(|(text, _, _)| text.contains('@')) {
        return pieces;
    }
    let mut concat = String::new();
    let mut spans = Vec::with_capacity(pieces.len());
    for (text, _, _) in &pieces {
        spans.push((concat.len(), concat.len() + text.len()));
        concat.push_str(text);
    }
    let regex = email_regex();
    let mut matches: Vec<(usize, usize)> = Vec::new();
    for caps in regex.captures_iter(&concat) {
        let whole = caps.get(0).unwrap();
        let preceded_by_colon = whole.start() > 0 && concat[..whole.start()].ends_with(':');
        if !preceded_by_colon {
            matches.push((whole.start(), whole.end()));
        }
    }
    if matches.is_empty() {
        return pieces;
    }
    let mut out = Vec::with_capacity(pieces.len());
    let mut idx = 0usize;
    let mut match_iter = matches.iter().peekable();
    while idx < pieces.len() {
        let (text, from, to) = &pieces[idx];
        let (piece_start, piece_end) = spans[idx];
        if let Some(&&(match_start, match_end)) = match_iter.peek()
            && piece_start >= match_start
            && piece_end <= match_end
        {
            if piece_start == match_start {
                out.push((concat[match_start..match_end].to_string(), *from, *to));
            }
            idx += 1;
            if piece_end == match_end {
                match_iter.next();
            }
            continue;
        }
        out.push((text.clone(), *from, *to));
        idx += 1;
    }
    out
}

fn is_ws_token(token: &str) -> bool {
    !token.is_empty() && token.chars().all(char::is_whitespace)
}

fn url_starts_at(i: usize, l: &[Piece]) -> bool {
    let token = l[i].0.as_str();
    if PROTOCOLS.contains(&token)
        && l.len() > i + 3
        && l[i + 1].0 == ":"
        && l[i + 2].0 == "/"
        && l[i + 3].0 == "/"
    {
        return true;
    }
    if l.len() > i + 1 && l[i].0 == "www" && l[i + 1].0 == "." {
        return true;
    }
    if l.len() > i + 3
        && l[i + 1].0 == "."
        && l[i + 3].0 == "/"
        && domain_chars_regex().is_match(token)
        && domain_chars_regex().is_match(&l[i + 2].0)
    {
        return true;
    }
    l.len() > i + 5
        && l[i + 1].0 == "."
        && l[i + 3].0 == "."
        && l[i + 5].0 == "/"
        && domain_chars_regex().is_match(token)
        && domain_chars_regex().is_match(&l[i + 2].0)
        && domain_chars_regex().is_match(&l[i + 4].0)
}

fn url_ends_at(i: usize, l: &[Piece], url_quote: &str) -> bool {
    let token = l[i].0.as_str();
    if is_ws_token(token) || token == ")" || token == "]" {
        return true;
    }
    if l.len() > i + 1 {
        let next = l[i + 1].0.as_str();
        let next_boundary = is_ws_token(next)
            || matches!(next, "\"" | "»" | "«" | "‘" | "’" | "“" | "”" | "'" | ".");
        let token_final = matches!(token, "." | "," | ";" | ":" | "!" | "?") || token == url_quote;
        if (next_boundary && token_final) || !url_chars_regex().is_match(token) {
            return true;
        }
    } else if !url_chars_regex().is_match(token) || token == "." || token == url_quote {
        return true;
    }
    false
}

fn join_urls(l: Vec<Piece>) -> Vec<Piece> {
    let mut new_list: Vec<Piece> = Vec::new();
    let mut in_url = false;
    let mut url_text = String::new();
    let mut url_from = 0usize;
    let mut url_quote = String::new();
    let mut i = 0usize;
    while i < l.len() {
        if !in_url && url_starts_at(i, &l) {
            in_url = true;
            if let Some(last) = new_list.last() {
                url_quote = last.0.clone();
            }
            url_from = l[i].1;
            url_text.push_str(&l[i].0);
        } else if in_url && url_ends_at(i, &l, &url_quote) {
            in_url = false;
            url_quote.clear();
            new_list.push((std::mem::take(&mut url_text), url_from, l[i].1));
            new_list.push(l[i].clone());
        } else if in_url {
            url_text.push_str(&l[i].0);
        } else {
            new_list.push(l[i].clone());
        }
        i += 1;
    }
    if !url_text.is_empty() {
        let to = l.last().map(|p| p.2).unwrap_or(url_from);
        new_list.push((url_text, url_from, to));
    }
    new_list
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        CatalanWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_pronoms_febles_split() {
        assert_eq!(
            texts("Emporta-te'ls a l'observatori dels mars"),
            vec![
                "Emporta".to_string(),
                "-te".to_string(),
                "'ls".to_string(),
                " ".to_string(),
                "a".to_string(),
                " ".to_string(),
                "l'".to_string(),
                "observatori".to_string(),
                " ".to_string(),
                "de".to_string(),
                "ls".to_string(),
                " ".to_string(),
                "mars".to_string(),
            ],
            "Java CatalanWordTokenizerTest expects exactly [Emporta, -te, 'ls,  , a,  , l', \
             observatori,  , de, ls,  , mars]: the closed pronom-feble class stays whole like the \
             dictionary keeps it, while 'dels' still splits into de+ls"
        );
        assert_eq!(
            texts("Emporta-te\u{2019}ls a l\u{2019}observatori dels mars").len(),
            13,
            "the typographic apostrophe behaves identically"
        );
    }

    #[test]
    fn java_pin_apos_initial_and_pattern_groups() {
        assert_eq!(texts("AVALUA'T"), vec!["AVALUA", "'T"]);
        assert_eq!(
            texts("Se'n d\u{f3}na vergonya"),
            vec!["Se", "'n", " ", "d\u{f3}na", " ", "vergonya"]
        );
        assert_eq!(texts("-M'agradaria."), vec!["-", "M'", "agradaria", "."]);
        assert_eq!(
            texts("No acabava d\u{2019}entendre\u{2019}l b\u{e9}"),
            vec![
                "No",
                " ",
                "acabava",
                " ",
                "d\u{2019}",
                "entendre",
                "\u{2019}l",
                " ",
                "b\u{e9}"
            ]
        );
        assert_eq!(
            texts("N'hi ha vint-i-quatre"),
            vec!["N'", "hi", " ", "ha", " ", "vint", "-", "i", "-", "quatre"]
        );
    }

    #[test]
    fn java_pin_hyphen_words_split_without_the_dictionary() {
        assert_eq!(texts("Sud-Est"), vec!["Sud", "-", "Est"]);
        assert_eq!(
            texts("la direcci\u{f3} E-SE"),
            vec!["la", " ", "direcci\u{f3}", " ", "E", "-", "SE"]
        );
        assert_eq!(
            texts("El tren Barcelona-València"),
            vec!["El", " ", "tren", " ", "Barcelona", "-", "València"]
        );
        assert_eq!(
            texts("\u{2018}El tren Barcelona-València\u{2019}"),
            vec![
                "\u{2018}",
                "El",
                " ",
                "tren",
                " ",
                "Barcelona",
                "-",
                "València",
                "\u{2019}"
            ]
        );
        assert_eq!(
            texts("Castella-la Manxa"),
            vec!["Castella", "-", "la", " ", "Manxa"]
        );
        assert_eq!(texts("S\u{e0}sser-l'Alguer").len(), 4);
        assert_eq!(
            texts("Em\u{ed}lia-Romanya"),
            vec!["Em\u{ed}lia", "-", "Romanya"]
        );
        assert_eq!(texts("abans-d'ahir").len(), 3);
        assert_eq!(texts("Qui-sap-lo temps").len(), 7);
    }

    #[test]
    fn java_pin_contractions_al_del_pel_can() {
        assert!(
            texts("Emporta-te'ls a l'observatori dels mars")
                .windows(2)
                .any(|w| w == ["de", "ls"]),
            "'dels' must split into de+ls"
        );
        assert_eq!(texts("al"), vec!["a", "l"]);
        assert_eq!(texts("pels"), vec!["pe", "ls"]);
        assert_eq!(texts("can"), vec!["ca", "n"]);
        assert_eq!(texts("casa"), vec!["casa"]);
    }
    #[test]
    fn java_pin_ela_geminada_forms() {
        assert_eq!(
            texts("col\u{b7}laboraci\u{f3}"),
            vec!["col\u{b7}laboraci\u{f3}".to_string()]
        );
        assert_eq!(
            texts("col.laboraci\u{f3}"),
            vec!["col.laboraci\u{f3}".to_string()]
        );
        assert_eq!(
            texts("col\u{2022}laboraci\u{f3}"),
            vec!["col.laboraci\u{f3}".to_string()]
        );
        assert_eq!(
            texts("col\u{b7}Laboraci\u{f3}"),
            vec!["col\u{b7}Laboraci\u{f3}".to_string()]
        );
        assert_eq!(
            texts("COL\u{2219}LABORADORES"),
            vec!["COL.LABORADORES".to_string()]
        );
        assert_eq!(texts("sol\u{2219}licitud"), vec!["sol.licitud".to_string()]);
    }

    #[test]
    fn java_pin_digit_grouping_and_decimals() {
        assert_eq!(texts("\u{c9}s d'1 km.").len(), 7);
        assert_eq!(texts("\u{c9}s d'1,5 km.").len(), 7);
        assert_eq!(texts("10 000").len(), 1);
        assert_eq!(texts("1 000 000").len(), 1);
        assert_eq!(texts("2005 57 114"), vec!["2005", " ", "57 114"]);
        assert_eq!(texts("2005 454"), vec!["2005", " ", "454"]);
        assert_eq!(texts("$1"), vec!["$1"]);
    }

    #[test]
    fn java_pin_urls_and_emails() {
        assert_eq!(
            texts("Visiteu 'http://www.softcatala.org'"),
            vec!["Visiteu", " ", "'", "http://www.softcatala.org", "'"]
        );
        assert_eq!(
            texts("Visiteu \"http://www.softcatala.org\""),
            vec!["Visiteu", " ", "\"", "http://www.softcatala.org", "\""]
        );
        assert_eq!(texts("name@example.com").len(), 1);
        assert_eq!(texts("name@example.com.").len(), 2);
        assert_eq!(texts("name@example.com:").len(), 2);
    }

    #[test]
    fn java_pin_emoji_and_variation_selectors() {
        assert_eq!(
            texts("Sol Pic\u{f3} (\u{1F98C}+\u{1F99A})"),
            vec![
                "Sol",
                " ",
                "Pic\u{f3}",
                " ",
                "(",
                "\u{1F98C}",
                "+",
                "\u{1F99A}",
                ")"
            ]
        );
        assert_eq!(texts("\u{1FAE1}prova."), vec!["\u{1FAE1}", "prova", "."]);
        assert_eq!(
            texts("\u{1FAE1}\u{1FAE1}prova\u{1FAE1}"),
            vec!["\u{1FAE1}", "\u{1FAE1}", "prova", "\u{1FAE1}"]
        );
        assert_eq!(texts("\u{2764}\u{FE0F}"), vec!["\u{2764}\u{FE0F}"]);
        assert_eq!(
            texts("\u{2764}\u{FE0F}prova"),
            vec!["\u{2764}\u{FE0F}", "prova"]
        );
        assert_eq!(texts("H\u{2082}O"), vec!["H\u{2082}O"]);
        assert_eq!(texts("\u{1FAE1}"), vec!["\u{1FAE1}"]);
    }

    #[test]
    fn java_pin_hangul_filler_and_leading_hyphen() {
        assert_eq!(
            texts("\u{c9}s ell \u{3161}va dir."),
            vec!["\u{c9}s", " ", "ell", " ", "\u{3161}va", " ", "dir", "."]
        );
        assert_eq!(
            texts("-contar-se'n-"),
            vec!["-", "contar", "-se", "'n", "-"]
        );
    }

    #[test]
    fn token_offsets_reconstruct_the_original_text() {
        let tokenizer = CatalanWordTokenizer::new();
        for text in [
            "Emporta-te'ls a l'observatori dels mars",
            "\u{c9}s d'1,5 km.",
            "col\u{2022}laboraci\u{f3}",
            "2005 57 114",
            "S\u{e0}sser-l'Alguer",
        ] {
            let tokens = tokenizer.tokenize(text);
            let mut last_end = 0usize;
            for token in &tokens {
                assert_eq!(token.start(), last_end, "gap before {token:?} in {text:?}");
                last_end = token.end();
            }
            assert_eq!(last_end, text.len(), "coverage of {text:?}");
        }
        let text = "col\u{2022}laboraci\u{f3} bonica";
        let tokens = tokenizer.tokenize(text);
        assert_eq!(
            &text[tokens[0].start()..tokens[0].end()],
            "col\u{2022}laboraci\u{f3}"
        );
        assert_eq!(tokens[0].text(), "col.laboraci\u{f3}");
    }
}
