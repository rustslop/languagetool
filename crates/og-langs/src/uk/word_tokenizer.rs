use og_core::Token;
use regex::Regex;
use std::sync::LazyLock;

const DECIMAL_COMMA_SUBST: char = '\u{E001}';
const NON_BREAKING_SPACE_SUBST: char = '\u{E002}';
const NON_BREAKING_DOT_SUBST: char = '\u{E003}';
const NON_BREAKING_COLON_SUBST: char = '\u{E004}';
const LEFT_BRACE_SUBST: char = '\u{E005}';
const RIGHT_BRACE_SUBST: char = '\u{E006}';
const NON_BREAKING_SLASH_SUBST: char = '\u{E007}';
const LEFT_ANGLE_SUBST: char = '\u{E008}';
const RIGHT_ANGLE_SUBST: char = '\u{E009}';
const SLASH_SUBST: char = '\u{E010}';
const NON_BREAKING_PLACEHOLDER: char = '\u{E109}';
const BREAKING_PLACEHOLDER: char = '\u{E110}';
const NON_BREAKING_PLACEHOLDER2: char = '\u{E120}';
const SOFT_HYPHEN_WRAP_SUBST: char = '\u{E103}';
const URL_START_REPLACE_CHAR: u32 = 0xE300;

const H_CLS: &str =
    "[ \\t\\u{00A0}\\u{1680}\\u{180E}\\u{2000}-\\u{200A}\\u{202F}\\u{205F}\\u{3000}]";
const V_CLS: &str = "[\\n\\u{0B}\\f\\r\\u{85}\\u{2028}\\u{2029}]";
const H_CHS: &str = " \\t\\u{00A0}\\u{1680}\\u{180E}\\u{2000}-\\u{200A}\\u{202F}\\u{205F}\\u{3000}";
const V_CHS: &str = "\\n\\u{0B}\\f\\r\\u{85}\\u{2028}\\u{2029}";

#[derive(Clone, Copy, PartialEq)]
struct Span {
    start: usize,
    end: usize,
}

const INSERTED: Span = Span {
    start: usize::MAX,
    end: usize::MAX,
};

struct Buf {
    chars: Vec<char>,
    spans: Vec<Span>,
    urls: Vec<(char, String)>,
}

struct Caps {
    cs: usize,
    ce: usize,
    groups: Vec<Option<(usize, usize)>>,
}

impl Buf {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            spans: text
                .char_indices()
                .map(|(i, c)| Span {
                    start: i,
                    end: i + c.len_utf8(),
                })
                .collect(),
            urls: Vec::new(),
        }
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }

    fn contains(&self, c: char) -> bool {
        self.chars.contains(&c)
    }

    fn slice(&self, from: usize, to: usize) -> String {
        self.chars[from.min(self.chars.len())..to.min(self.chars.len())]
            .iter()
            .collect()
    }

    fn next(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn prev(&self, i: usize) -> Option<char> {
        (i > 0).then(|| self.chars[i - 1])
    }
}

enum Part {
    Group(usize),
    Keep(char),
    Ins(char),
}

fn parse_template(repl: &str) -> Vec<Part> {
    let chars: Vec<char> = repl.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            parts.push(Part::Group(
                chars[i + 1].to_digit(10).expect("digit") as usize
            ));
            i += 2;
        } else {
            let c = chars[i];
            parts.push(
                if c == BREAKING_PLACEHOLDER || c == NON_BREAKING_PLACEHOLDER2 {
                    Part::Ins(c)
                } else {
                    Part::Keep(c)
                },
            );
            i += 1;
        }
    }
    parts
}

fn java_pattern(pattern: &str) -> Regex {
    let translated = pattern
        .replace("\\h", H_CLS)
        .replace("\\v", V_CLS)
        .replace("\\d", "[0-9]");
    Regex::new(&translated).expect("valid tokenizer pattern")
}

fn char_index(chars: &[char], byte: usize) -> usize {
    let mut count = 0usize;
    let mut bytes = 0usize;
    for c in chars {
        if bytes >= byte {
            return count;
        }
        bytes += c.len_utf8();
        count += 1;
    }
    count
}

fn byte_of_char(chars: &[char], idx: usize) -> usize {
    chars[..idx.min(chars.len())]
        .iter()
        .map(|c| c.len_utf8())
        .sum()
}

struct Rule {
    re: Regex,
    template: Vec<Part>,
    first_only: bool,
}

impl Rule {
    fn new(pattern: &str, repl: &str) -> Self {
        Self {
            re: java_pattern(pattern),
            template: parse_template(repl),
            first_only: false,
        }
    }

    fn first(pattern: &str, repl: &str) -> Self {
        Self {
            re: java_pattern(pattern),
            template: parse_template(repl),
            first_only: true,
        }
    }

    fn apply(&self, buf: &mut Buf, guard: impl Fn(&Buf, &Caps) -> bool) {
        let mut scan_from = 0usize;
        loop {
            let text = buf.text();
            let Some(caps) = self.re.captures_at(&text, scan_from.min(text.len())) else {
                return;
            };
            let whole = caps.get(0).expect("whole match");
            let cs = char_index(&buf.chars, whole.start());
            let ce = char_index(&buf.chars, whole.end());
            let mut groups = Vec::with_capacity(caps.len());
            for group in 1..caps.len() {
                groups.push(caps.get(group).map(|g| {
                    (
                        char_index(&buf.chars, g.start()),
                        char_index(&buf.chars, g.end()),
                    )
                }));
            }
            let info = Caps { cs, ce, groups };
            if !guard(buf, &info) {
                scan_from = byte_of_char(&buf.chars, cs + 1);
                if scan_from >= text.len() {
                    return;
                }
                continue;
            }

            let mut covered = vec![false; ce - cs];
            for (gi, group) in info.groups.iter().enumerate() {
                if let Some((gs, ge)) = group {
                    for slot in covered.iter_mut().take(ge - cs).skip(gs - cs) {
                        *slot = true;
                    }
                    let _ = gi;
                }
            }
            let mut literal_spans: Vec<Span> = Vec::new();
            for (offset, is_covered) in covered.iter().enumerate() {
                if !is_covered {
                    literal_spans.push(buf.spans[cs + offset]);
                }
            }

            let mut out_chars: Vec<char> = Vec::new();
            let mut out_spans: Vec<Span> = Vec::new();
            let mut next_literal = 0usize;
            for part in &self.template {
                match part {
                    Part::Group(g) => {
                        if let Some(Some((gs, ge))) = info.groups.get(*g - 1) {
                            for idx in *gs..*ge {
                                out_chars.push(buf.chars[idx]);
                                out_spans.push(buf.spans[idx]);
                            }
                        }
                    }
                    Part::Keep(c) => {
                        out_chars.push(*c);
                        out_spans
                            .push(literal_spans.get(next_literal).copied().unwrap_or(INSERTED));
                        next_literal += 1;
                    }
                    Part::Ins(c) => {
                        out_chars.push(*c);
                        out_spans.push(INSERTED);
                    }
                }
            }

            let out_len = out_chars.len();
            let mut chars = buf.chars[..cs].to_vec();
            let mut spans = buf.spans[..cs].to_vec();
            chars.append(&mut out_chars);
            spans.append(&mut out_spans);
            chars.extend_from_slice(&buf.chars[ce..]);
            spans.extend_from_slice(&buf.spans[ce..]);
            buf.chars = chars;
            buf.spans = spans;

            if self.first_only {
                return;
            }
            let resume = if out_len > 0 { cs + out_len } else { cs + 1 };
            scan_from = byte_of_char(&buf.chars, resume);
            if scan_from >= buf.text().len() {
                return;
            }
        }
    }
}

fn is_cyrillic_letter(c: char) -> bool {
    matches!(
        c,
        'а'..='я' | 'ї' | 'є' | 'ґ' | 'і' | 'А'..='Я' | 'Ї' | 'Є' | 'Ґ' | 'І'
    )
}

fn is_cyrillic_lower(c: char) -> bool {
    matches!(c, 'а'..='я' | 'ї' | 'є' | 'ґ' | 'і')
}

fn is_superscript(c: char) -> bool {
    matches!(c, '\u{00B9}' | '\u{00B2}' | '\u{00B3}') || ('\u{2070}'..='\u{2079}').contains(&c)
}

fn is_h_or_v(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{00A0}' | '\u{1680}' | '\u{180E}')
        || ('\u{2000}'..='\u{200A}').contains(&c)
        || matches!(c, '\u{202F}' | '\u{205F}' | '\u{3000}')
        || matches!(
            c,
            '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
}

fn is_java_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

fn is_unicode_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn prev_not_cyr_apo(buf: &Buf, cs: usize) -> bool {
    buf.prev(cs)
        .is_none_or(|p| !(is_cyrillic_letter(p) || p == '\'' || p == '\u{0301}' || p == '-'))
}

const ARM5_CHARS: &[char] = &[
    ' ', '\u{00A0}', '\n', '\r', '\t', ',', '.', ';', '!', '?', '\u{2014}', '\u{2015}', ':', '(',
    ')', '[', ']', '{', '}', '<', '>', '/', '|', '\\', '\u{2026}', '\u{00B0}', '$', '\u{20AC}',
    '\u{20B4}', '=', '\u{2116}', '\u{00A7}', '\u{00BF}', '\u{00A1}', '~', '\u{00D7}',
];

fn in_arm11(c: char) -> bool {
    ('\u{2000}'..='\u{200F}').contains(&c)
        || c == '\u{201A}'
        || ('\u{2020}'..='\u{202F}').contains(&c)
        || ('\u{2030}'..='\u{206F}').contains(&c)
        || ('\u{2400}'..='\u{27FF}').contains(&c)
        || c == '\u{21B5}'
        || ('\u{1F000}'..='\u{1FFFF}').contains(&c)
        || ('\u{F000}'..='\u{FFFF}').contains(&c)
        || c == BREAKING_PLACEHOLDER
}

fn global_guard_ok(chars: &[char], i: usize, len: usize) -> bool {
    chars.get(i + len) != Some(&NON_BREAKING_PLACEHOLDER2)
}

fn split_match_len(chars: &[char], i: usize) -> Option<usize> {
    let c = chars[i];
    let bang_run = |chars: &[char], i: usize, bang: char| -> Vec<usize> {
        let mut lens = Vec::new();
        if c == bang && chars.get(i + 1) == Some(&bang) {
            if chars.get(i + 2) == Some(&bang) {
                lens.push(3);
            }
            lens.push(2);
        }
        lens
    };
    for len in bang_run(chars, i, '!') {
        if global_guard_ok(chars, i, len) {
            return Some(len);
        }
    }
    for len in bang_run(chars, i, '?') {
        if global_guard_ok(chars, i, len) {
            return Some(len);
        }
    }
    if c == '.'
        && chars.get(i + 1) == Some(&'.')
        && chars.get(i + 2) == Some(&'.')
        && global_guard_ok(chars, i, 3)
    {
        return Some(3);
    }
    if (c == '!' || c == '?') && matches!(chars.get(i + 1), Some('!' | '?' | '.')) {
        let mut max = 1;
        while max < 3 && matches!(chars.get(i + max), Some('!' | '?' | '.')) {
            max += 1;
        }
        while max >= 2 {
            if global_guard_ok(chars, i, max) {
                return Some(max);
            }
            max -= 1;
        }
    }
    if ARM5_CHARS.contains(&c) && global_guard_ok(chars, i, 1) {
        return Some(1);
    }
    if c == '%' {
        let dash_next = matches!(chars.get(i + 1), Some('-') | Some('\u{2013}'));
        let own_guard_ok = !(dash_next && chars.get(i + 2).is_some_and(|c| is_cyrillic_lower(*c)));
        if own_guard_ok && global_guard_ok(chars, i, 1) {
            return Some(1);
        }
    }
    if matches!(c, '"' | '«' | '»' | '„' | '“')
        && (i == 0 || chars[i - 1] != NON_BREAKING_PLACEHOLDER)
        && global_guard_ok(chars, i, 1)
    {
        return Some(1);
    }
    if is_superscript(c)
        && i > 0
        && is_cyrillic_letter(chars[i - 1])
        && global_guard_ok(chars, i, 1)
    {
        return Some(1);
    }
    if c == '_' || c == '*' {
        let prev_letter =
            i > 0 && (is_cyrillic_letter(chars[i - 1]) || chars[i - 1].is_ascii_alphabetic());
        let mut run = 0;
        while matches!(chars.get(i + run), Some('_') | Some('*')) {
            run += 1;
        }
        if !prev_letter {
            while run > 0 {
                if global_guard_ok(chars, i, run) {
                    return Some(run);
                }
                run -= 1;
            }
            return None;
        }
        while run > 0 {
            let after = chars.get(i + run).copied();
            let own_ok = after.is_none_or(|a| {
                !(is_cyrillic_letter(a) || a.is_ascii_alphabetic() || a.is_ascii_digit())
            });
            if own_ok && global_guard_ok(chars, i, run) {
                return Some(run);
            }
            run -= 1;
        }
        return None;
    }
    if in_arm11(c) && global_guard_ok(chars, i, 1) {
        return Some(1);
    }
    None
}

struct TokenizerRules {
    weird_apostroph: Rule,
    leading_dash: Rule,
    leading_dash_2: Rule,
    decimal_comma: Rule,
    url: Regex,
    em_dash_space: Rule,
    dash_numbers: Rule,
    n_dash_space: Rule,
    n_dash_space2: Rule,
    year_with_r: Rule,
    compound_quotes1: Rule,
    compound_quotes2: Rule,
    words_with_brackets: Rule,
    dotted_numbers3: Rule,
    dotted_numbers: Rule,
    abbr_dot_nar1: Rule,
    abbr_dot_nar2: Rule,
    abbr_dot_2_small: Rule,
    abbr_dot_vo1: Rule,
    abbr_dot_vo2: Rule,
    abbr_dot_vo3: Rule,
    abbr_dot_art: Rule,
    abbr_dot_man: Rule,
    abbr_dot_tys1: Rule,
    abbr_dot_tys2: Rule,
    abbr_dot_vyd: Rule,
    abbr_dot_lat: Rule,
    abbr_dot_prof: Rule,
    abbr_dot_gub: Rule,
    abbr_dot_dash: Rule,
    initials_sp2: Rule,
    initials_sp1: Rule,
    initials_rsp2: Rule,
    initials_rsp1: Rule,
    abbr_dot_kub_sm: Rule,
    abbr_dot_s_g: Rule,
    abbr_dot_chl_kor: Rule,
    abbr_dot_nauk: Rule,
    abbr_dot_pn_zah: Rule,
    abbr_dot_i_t_p: Rule,
    abbr_dot_i_t_ch: Rule,
    abbr_dot_t_zv: Rule,
    abbr_dot_red_avt: Rule,
    abbr_dot_non_ending: Rule,
    abbr_dot_non_ending2: Rule,
    invalid_mln: Rule,
    abbr_at_the_end: Regex,
    web_entities: Rule,
    web_entities2: Rule,
    abbr_dot_ending: Rule,
    decimal_space: Regex,
    colon_numbers: Rule,
    brace_in_word: Rule,
    xml_tag: Rule,
    quote_dash_left: Rule,
    quote_dash_right: Rule,
    apostrophe_begin: Rule,
    apostrophe_end: Rule,
    plus: Rule,
    dash_before_number: Rule,
    number_missing_space: Rule,
}

static RULES: LazyLock<TokenizerRules> = LazyLock::new(|| TokenizerRules {
    weird_apostroph: Rule::new(
        "(?i)([бвджзклмнпрстфхш])([\"\u{201D}\u{201F}`´])([єїюя])",
        "$1\u{E120}$2\u{E120}$3",
    ),
    leading_dash: Rule::first(
        "^([\\u{2014}\\u{2013}])([а-яіїєґА-ЯІЇЄҐA-Z])",
        "$1\u{E110}$2",
    ),
    leading_dash_2: Rule::first("^(-)([А-ЯІЇЄҐA-Z])", "$1\u{E110}$2"),
    decimal_comma: Rule::new("([0-9]),([0-9])", "$1\u{E001}$2"),
    url: java_pattern(&format!(
        "(?i)((https?|ftp)://|www\\.)[^{h}{v}/$.?#),]+\\.[^{h}{v}),\">]*|(mailto:)?[\\p{{L}}0-9._-]+@[\\p{{L}}0-9_-]+(\\.[\\p{{L}}0-9_-]+)+",
        h = H_CHS,
        v = V_CHS
    )),
    em_dash_space: Rule::new("\\u{2014}([\\h\\v])", "\u{E110}\u{2014}$1"),
    dash_numbers: Rule::new(
        "([IVXІХ]+)([\\u{2013}-])([IVXІХ]+)",
        "$1\u{E110}$2\u{E110}$3",
    ),
    n_dash_space: Rule::new("(?i)([а-яіїєґa-z0-9])(\\u{2013}\\h)", "$1\u{E110}$2"),
    n_dash_space2: Rule::new("(?i)([\\h.,;!?]\\u{2013})([а-яіїєґa-z])", "$1\u{E110}$2"),
    year_with_r: Rule::new(
        "((?:[12][0-9]{3}[\\u{2014}\\u{2013}-])?[12][0-9]{3})(рр?\\.)",
        "$1\u{E110}$2",
    ),
    compound_quotes1: Rule::new(
        "(?i)([а-яіїєґ]-)([«\"„])([а-яіїєґ'-]+)([»\"“])",
        "$1$2\u{E120}$3\u{E120}$4\u{E120}",
    ),
    compound_quotes2: Rule::new(
        "(?i)([«\"„])([а-яіїєґ0-9'-]+)([»\"“])(-[а-яіїє])",
        "$1\u{E120}$2\u{E120}$3\u{E120}$4",
    ),
    words_with_brackets: Rule::new(
        "(?i)([а-яіїєґ])\\[([а-яіїєґ]+)\\]",
        "$1[\u{E120}$2]\u{E120}",
    ),
    dotted_numbers3: Rule::new("([0-9])\\.([0-9]+)\\.([0-9])", "$1.\u{E120}$2.\u{E120}$3"),
    dotted_numbers: Rule::new("([0-9])\\.([0-9])", "$1.\u{E120}$2"),
    abbr_dot_nar1: Rule::new(
        "(([0-9]|рік|[рp]\\.|[-\\u{2013}\\u{2014}])[\\h\\v]+нар)\\.",
        "$1.\u{E120}\u{E110}",
    ),
    abbr_dot_nar2: Rule::new(
        "\\b(нар)\\.([\\h\\v]+[0-9а-яіїєґ])",
        "$1.\u{E120}\u{E110}$2",
    ),
    abbr_dot_2_small: Rule::new(
        "([^а-яіїєґА-ЯІЇЄҐ'\\u{0301}-][векнпрстцч]{1,2})\\.(\\h*)([екмнпрстч]{1,2})\\.",
        "$1\u{E003}\u{E110}$2$3\u{E003}\u{E110}",
    ),
    abbr_dot_vo1: Rule::new(
        "\\b([вВу])\\.([\\h\\v]*о)\\.",
        "$1\u{E003}\u{E110}$2\u{E003}\u{E110}",
    ),
    abbr_dot_vo2: Rule::new(
        "\\b(к)\\.([\\h\\v]*с)\\.",
        "$1\u{E003}\u{E110}$2\u{E003}\u{E110}",
    ),
    abbr_dot_vo3: Rule::new(
        "\\b(ч|ст)\\.([\\h\\v]*л)\\.",
        "$1\u{E003}\u{E110}$2\u{E003}\u{E110}",
    ),
    abbr_dot_art: Rule::new(
        "([Аа]рт|[Мм]ал|[Рр]ис|[Сс]пр)\\.([\\h]*(№[\\h]*)?[0-9])",
        "$1\u{E003}\u{E110}$2",
    ),
    abbr_dot_man: Rule::new("(Ман)\\.([\\h]*(Сіті|[Юю]н))", "$1\u{E003}\u{E110}$2"),
    abbr_dot_tys1: Rule::new("([0-9IІ][\\h\\v]+)(тис|арт)\\.", "$1$2\u{E003}\u{E110}"),
    abbr_dot_tys2: Rule::new(
        "(тис|арт)\\.([\\h\\v]+[а-яіїєґ0-9])",
        "$1\u{E003}\u{E110}$2",
    ),
    abbr_dot_vyd: Rule::new(
        "([0-9IІVXХ](-[а-яєі]+)?[\\h\\v]+)(вид)\\.",
        "$1$3\u{E003}\u{E110}",
    ),
    abbr_dot_lat: Rule::new(
        "([^а-яіїєґА-ЯІЇЄҐ'\\u{0301}-]лат)\\.([\\h\\v]+[a-zA-Z])",
        "$1\u{E003}\u{E110}$2",
    ),
    abbr_dot_prof: Rule::new(
        "([Аа]кад|[Пп]роф|[Дд]оц|[Аа]сист|[Аа]рх|ап|тов|вул|бул|бульв|о|р|ім|упорядн?|др|[Пп]реп|Ів|Дж|Ол|[сС]вт|Авг)\\.([\\h\\v]+[А-ЯІЇЄҐа-яіїєґ])",
        "$1\u{E003}\u{E110}$2",
    ),
    abbr_dot_gub: Rule::new(
        "(.[А-ЯІЇЄҐ][а-яіїєґ'-]+[\\h\\v]+губ)\\.",
        "$1\u{E003}\u{E110}",
    ),
    abbr_dot_dash: Rule::new(
        "\\b([А-ЯІЇЄҐ]ж?)\\.([-\\u{2013}]([А-ЯІЇЄҐ][а-яіїєґ']{2}|[А-ЯІЇЄҐ]\\.))",
        "$1\u{E003}$2",
    ),
    initials_sp2: Rule::new(
        "([А-ЯІЇЄҐ])\\.([\\h\\v]{0,5}[А-ЯІЇЄҐ])\\.([\\h\\v]{0,5}[А-ЯІЇЄҐ][а-яіїєґ']+)",
        "$1\u{E003}\u{E110}$2\u{E003}\u{E110}$3",
    ),
    initials_sp1: Rule::new(
        "([А-ЯІЇЄҐ])\\.([\\h\\v]{0,5}[А-ЯІЇЄҐ][а-яіїєґ']+)",
        "$1\u{E003}\u{E110}$2",
    ),
    initials_rsp2: Rule::new(
        "([А-ЯІЇЄҐ][а-яіїєґ']+)([\\h\\v]?[А-ЯІЇЄҐ])\\.([\\h\\v]?[А-ЯІЇЄҐ])\\.",
        "$1\u{E110}$2\u{E003}\u{E110}$3\u{E003}\u{E110}",
    ),
    initials_rsp1: Rule::new(
        "([А-ЯІЇЄҐ][а-яіїєґ']+)([\\h\\v]?[А-ЯІЇЄҐ])\\.",
        "$1\u{E110}$2\u{E003}\u{E110}",
    ),
    abbr_dot_kub_sm: Rule::new(
        "(кв|куб)\\.([\\h\\v]*(?:[смкд]|мк)?м)",
        "$1.\u{E120}\u{E110}$2",
    ),
    abbr_dot_s_g: Rule::new("\\b(с)\\.(-г)\\.", "$1\u{E003}$2\u{E003}\u{E110}"),
    abbr_dot_chl_kor: Rule::new("(чл)\\.(-кор)\\.", "$1.\u{E120}$2.\u{E120}\u{E110}"),
    abbr_dot_nauk: Rule::new(
        "(наук)\\.(-[а-яіїєґ]+)\\.",
        "$1.\u{E120}$2.\u{E120}\u{E110}",
    ),
    abbr_dot_pn_zah: Rule::new(
        "(пн|пд)\\.(-(зах|сх))\\.",
        "$1.\u{E120}\u{E110}$2.\u{E120}\u{E110}",
    ),
    abbr_dot_i_t_p: Rule::new(
        "([ій][\\h\\v]+т\\.)([\\h\\v]*(д|п|ін)\\.)",
        "$1\u{E120}\u{E110}$2\u{E120}\u{E110}",
    ),
    abbr_dot_i_t_ch: Rule::new(
        "([ву][\\h\\v]+т\\.)([\\h\\v]*ч\\.)",
        "$1\u{E120}\u{E110}$2\u{E120}\u{E110}",
    ),
    abbr_dot_t_zv: Rule::new(
        "([\\h\\v\\(]+т\\.)([\\h\\v]*зв\\.)",
        "$1\u{E120}\u{E110}$2\u{E120}\u{E110}",
    ),
    abbr_dot_red_avt: Rule::new(
        "([\\h\\v]+(?:[Рр]ед|[Аа]вт))\\.(\\h*[)\\]а-яіїєґ])",
        "$1.\u{E120}\u{E110}$2",
    ),
    abbr_dot_non_ending: Rule::new(
        "(абз|австрал|ам|амер|англ|акад(ем)?|арк|ауд|біол|бл(?:изьк)?|болг|буд|в|вип|вірм|грец(?:ьк)?\
         |держ|див|дир|діал|дод|дол|досл|доц|доп|екон|ел|жін|зав|заст|зах|зб|зв|зневажл?|зовн|іл|ім|івр|інж|ісп|іст|італ\
         |к|каб|каф|канд|кв|[1-9]-кімн|кімн|кін|кл|кн|коеф|крим|латин|мал|моб|н|[Нн]апр|нач|нім|нац|нпр|образн|оз|оп|оф\
         |п|пен|перекл|перен|пл|пол|пом|пор|порівн|[Пп]оч|пп|прибл|прикм|прим|присл|пров|пром|просп\
         |[Рр]ед|[Рр]еж|розд|розм|рос|рт|рум|с|санскр|[Сс]вв?|скор|соц|співавт|[сС]т|стор|суч|сх|табл|тт|[тТ]ел|техн\
         |укр|філол|фр|франц|худ|[цЦ]ит|ч|чайн|част|ц|яп|япон)\\.",
        "$1.\u{E120}\u{E110}",
    ),
    abbr_dot_non_ending2: Rule::new(
        "([^а-яіїєґА-ЯІЇЄҐ'-]м\\.)([\\h\\v]*[А-ЯІЇЄҐ])",
        "$1\u{E120}\u{E110}$2",
    ),
    invalid_mln: Rule::new("(млн|млрд)\\.( [а-яіїєґ])", "$1.\u{E120}\u{E110}$2"),
    abbr_at_the_end: java_pattern("(тис|губ|[А-ЯІЇЄҐ])\\.[\\h\\v]*$"),
    web_entities: Rule::new(
        "(?i)([а-яіїєґ])\\.(НЕТ|net|Інфо|Info|City|Life|UA|юа|лі|media|com|фм|ru|ру|орг)\\b",
        "$1.\u{E120}$2",
    ),
    web_entities2: Rule::new("(?i)\\.([a-z_-]+)\\.(ua)", ".\u{E120}$1.\u{E120}$2"),
    abbr_dot_ending: Rule::new(
        "([^а-яіїєґА-ЯІЇЄҐ'\\u{0301}-]((та|й|і) (інш?|под)|атм|відс|гр|коп|дес|дол|обл|пов|р|рр|РР|руб|ст|стст|стол|стор|чол|шт))\\.",
        "$1.\u{E120}\u{E110}",
    ),
    decimal_space: java_pattern("(?i)([0-9]{1,3})([\\h][0-9]{3})+"),
    colon_numbers: Rule::new("([0-9]):([0-9])", "$1\u{E004}$2"),
    brace_in_word: Rule::new("(?i)([а-яіїєґ])\\(([а-яіїєґ']+)\\)", "$1\u{E005}$2\u{E006}"),
    xml_tag: Rule::new("(?i)<(/?[a-z_]+/?)>", "\u{E110}\u{E008}$1\u{E009}\u{E110}"),
    quote_dash_left: Rule::new("([а-яіїєґА-ЯІЇЄҐ])([»\"-]+-)", "$1\u{E110}$2"),
    quote_dash_right: Rule::new("([»\"-]+-)([а-яіїєґА-ЯІЇЄҐ])", "$1\u{E110}$2"),
    apostrophe_begin: Rule::new("(^|[\\h\\v(„«\"'])'(\\p{L})", "$1'\u{E110}$2"),
    apostrophe_end: Rule::new("(\\p{L})'([^\\p{L}-]|$)", "$1\u{E110}'$2"),
    plus: Rule::new("\\+", "\u{E110}+\u{E110}"),
    dash_before_number: Rule::new("([-\\u{2013}])", "$1\u{E110}"),
    number_missing_space: Rule::new(
        "((?:[\\h\\v\\u{E110}]|^)[а-яїієґА-ЯІЇЄҐ'-]*[а-яїієґ']?[а-яїієґ])([0-9]+(?:[^а-яіїєґА-ЯІЇЄҐa-zA-Z»\"“]|$))",
        "$1\u{E110}$2",
    ),
});

fn always(_: &Buf, _: &Caps) -> bool {
    true
}

fn guard_n_dash_space(buf: &Buf, caps: &Caps) -> bool {
    let rest = buf.slice(caps.ce, buf.chars.len());
    for w in ["та", "чи", "і", "й"] {
        if rest.starts_with(w) {
            let after: String = rest.chars().skip(w.chars().count()).collect();
            if after.chars().next().is_some_and(is_h_or_v) {
                return false;
            }
        }
    }
    true
}

fn guard_two_small(buf: &Buf, caps: &Caps) -> bool {
    let Some(Some((letters_start, _))) = caps.groups.get(2) else {
        return true;
    };
    let tail = buf.slice(*letters_start, buf.chars.len());
    let mut chars = tail.chars();
    let first = chars.next().unwrap_or(' ');
    let second = chars.next().unwrap_or(' ');
    let third = chars.next().unwrap_or(' ');
    if matches!(first, 'с' | 'м' | 'к' | 'д') && second == 'м' && third == '.' {
        return false;
    }
    if first == 'м' && second == '.' {
        return false;
    }
    true
}

fn guard_non_ending(buf: &Buf, caps: &Caps) -> bool {
    if !prev_not_cyr_apo(buf, caps.cs) {
        return false;
    }
    if caps.ce - caps.cs == 2 && buf.chars.get(caps.cs) == Some(&'в') {
        return false;
    }
    let rest: &[char] = &buf.chars[caps.ce.min(buf.chars.len())..];
    if rest.first() == Some(&NON_BREAKING_PLACEHOLDER2) {
        return false;
    }
    let mut i = 0;
    while i < rest.len() && rest[i] == '.' {
        i += 1;
    }
    if i > 0 && rest[i..].iter().all(|c| is_h_or_v(*c)) {
        return false;
    }
    true
}

fn guard_ending(buf: &Buf, caps: &Caps) -> bool {
    buf.next(caps.ce) != Some(NON_BREAKING_PLACEHOLDER2)
}

fn guard_apostrophe_begin(buf: &Buf, caps: &Caps) -> bool {
    let apos = caps
        .groups
        .first()
        .and_then(|g| g.map(|(_, e)| e))
        .unwrap_or(caps.cs);
    !buf.slice(apos + 1, buf.chars.len()).starts_with("дно")
}

fn guard_apostrophe_end(buf: &Buf, caps: &Caps) -> bool {
    let apos_index = caps.cs + 1;
    for w in ["мо", "тре", "тра", "чо", "нічо", "бо", "зара", "пра"] {
        let p = apos_index.saturating_sub(w.chars().count());
        if p < apos_index
            && buf.slice(p, apos_index) == w
            && (p == 0 || !is_unicode_word(buf.chars[p - 1]))
        {
            return false;
        }
    }
    true
}

fn guard_plus(buf: &Buf, caps: &Caps) -> bool {
    buf.next(caps.ce)
        .is_some_and(|c| is_cyrillic_letter(c) || c.is_ascii_digit())
}

fn guard_dash_before_number(buf: &Buf, caps: &Caps) -> bool {
    (caps.cs == 0 || is_h_or_v(buf.chars[caps.cs - 1]))
        && buf.next(caps.ce).is_some_and(|c| c.is_ascii_digit())
}

impl TokenizerRules {
    fn apply_all(&self, buf: &mut Buf) {
        cleanup_chars(buf);
        self.weird_apostroph.apply(buf, always);
        if matches!(
            buf.chars.first(),
            Some('\u{2014}') | Some('\u{2013}') | Some('-')
        ) {
            let before = buf.text();
            self.leading_dash.apply(buf, always);
            if buf.text() == before {
                self.leading_dash_2.apply(buf, always);
            }
        }
        if buf.contains(',') {
            self.decimal_comma.apply(buf, always);
        }
        let text = buf.text();
        if text.contains("http")
            || text.contains("www")
            || text.contains("@")
            || text.contains("ftp")
        {
            replace_urls(buf, &self.url);
        }
        if buf.contains('\u{2014}') {
            self.em_dash_space.apply(buf, always);
        }
        let n_dash_present = buf.contains('\u{2013}');
        if buf.contains('-') || n_dash_present {
            self.dash_numbers.apply(buf, always);
            if n_dash_present {
                self.n_dash_space.apply(buf, guard_n_dash_space);
                self.n_dash_space2.apply(buf, always);
            }
        }
        replace_literal_pair(buf, "с/г", &[None, Some(NON_BREAKING_SLASH_SUBST), None]);
        replace_literal_pair(
            buf,
            "Л/ДНР",
            &[None, Some(NON_BREAKING_SLASH_SUBST), None, None, None],
        );
        if buf.text().contains("р.") {
            self.year_with_r.apply(buf, always);
        }
        insert_before_hash(buf);
        if buf.contains('%') {
            percent_breaking(buf);
        }
        self.compound_quotes1.apply(buf, always);
        self.compound_quotes2.apply(buf, always);
        if buf.contains('[') {
            self.words_with_brackets.apply(buf, always);
        }

        let dot_index = buf.chars.iter().position(|c| *c == '.');
        let rt_len = trimmed_end_len(buf);
        let dot_inside_sentence = dot_index.is_some_and(|d| d + 1 < rt_len);
        let abbr_at_end = dot_index.is_some_and(|d| d + 1 == rt_len)
            && matches_abbr_at_the_end(buf, &self.abbr_at_the_end);
        if dot_inside_sentence || abbr_at_end {
            self.dotted_numbers3.apply(buf, always);
            self.dotted_numbers.apply(buf, always);
            self.abbr_dot_nar1.apply(buf, always);
            self.abbr_dot_nar2.apply(buf, always);
            self.abbr_dot_2_small.apply(buf, guard_two_small);
            self.abbr_dot_vo1.apply(buf, always);
            self.abbr_dot_vo2.apply(buf, always);
            self.abbr_dot_vo3.apply(buf, always);
            self.abbr_dot_art.apply(buf, always);
            self.abbr_dot_man.apply(buf, always);
            self.abbr_dot_tys1.apply(buf, always);
            self.abbr_dot_tys2.apply(buf, always);
            self.abbr_dot_vyd.apply(buf, always);
            self.abbr_dot_lat.apply(buf, always);
            self.abbr_dot_prof
                .apply(buf, |buf, caps| prev_not_cyr_apo(buf, caps.cs));
            self.abbr_dot_gub.apply(buf, always);
            self.abbr_dot_dash.apply(buf, always);
            self.initials_sp2.apply(buf, always);
            self.initials_sp1.apply(buf, always);
            self.initials_rsp2.apply(buf, always);
            self.initials_rsp1.apply(buf, always);
            self.abbr_dot_kub_sm.apply(buf, always);
            self.abbr_dot_s_g.apply(buf, always);
            self.abbr_dot_chl_kor.apply(buf, always);
            self.abbr_dot_nauk.apply(buf, always);
            self.abbr_dot_pn_zah.apply(buf, always);
            self.abbr_dot_i_t_p.apply(buf, always);
            self.abbr_dot_i_t_ch.apply(buf, always);
            self.abbr_dot_t_zv.apply(buf, always);
            self.abbr_dot_red_avt.apply(buf, always);
            self.abbr_dot_non_ending.apply(buf, guard_non_ending);
            self.abbr_dot_non_ending2.apply(buf, always);
            self.invalid_mln.apply(buf, always);
        }
        if dot_inside_sentence {
            self.web_entities.apply(buf, always);
            self.web_entities2.apply(buf, always);
        }
        self.abbr_dot_ending.apply(buf, guard_ending);
        apply_decimal_space(buf, &self.decimal_space);
        if buf.contains(':') {
            self.colon_numbers.apply(buf, always);
        }
        if buf.contains('(') {
            self.brace_in_word.apply(buf, always);
        }
        if buf.contains('<') {
            self.xml_tag.apply(buf, always);
            fix_angle_slashes(buf);
        }
        if buf.contains('-') {
            self.quote_dash_left.apply(buf, always);
            self.quote_dash_right.apply(buf, always);
        }
        if buf.text().contains("\u{00AD}\n") {
            wrap_soft_hyphens(buf);
        }
        if buf.contains('\'') {
            self.apostrophe_begin.apply(buf, guard_apostrophe_begin);
            self.apostrophe_end.apply(buf, guard_apostrophe_end);
        }
        if buf.contains('+') {
            self.plus.apply(buf, guard_plus);
        }
        if buf.chars.len() > 1 && (buf.contains('-') || buf.contains('\u{2013}')) {
            self.dash_before_number.apply(buf, guard_dash_before_number);
        }
        self.number_missing_space.apply(buf, always);
    }
}

fn cleanup_chars(buf: &mut Buf) {
    for c in buf.chars.iter_mut() {
        *c = match *c {
            '\u{2019}' | '\u{02BC}' | '\u{2018}' => '\'',
            '\u{201A}' => ',',
            '\u{2011}' => '-',
            other => other,
        };
    }
}

fn trimmed_end_len(buf: &Buf) -> usize {
    let mut len = buf.chars.len();
    while len > 0 && is_h_or_v(buf.chars[len - 1]) {
        len -= 1;
    }
    len
}

fn matches_abbr_at_the_end(buf: &Buf, re: &Regex) -> bool {
    let text = buf.text();
    let mut search_from = 0usize;
    while let Some(m) = re.find_at(&text, search_from) {
        let cs = char_index(&buf.chars, m.start());
        if prev_not_cyr_apo(buf, cs) {
            return true;
        }
        search_from = m.start() + 1;
    }
    false
}

fn insert_char_at(buf: &mut Buf, pos: usize, c: char) {
    let pos = pos.min(buf.chars.len());
    buf.chars.insert(pos, c);
    buf.spans.insert(pos, INSERTED);
}

fn insert_before_hash(buf: &mut Buf) {
    let mut i = 0;
    while i < buf.chars.len() {
        if buf.chars[i] == '#' {
            insert_char_at(buf, i, BREAKING_PLACEHOLDER);
            i += 2;
        } else {
            i += 1;
        }
    }
}

fn percent_breaking(buf: &mut Buf) {
    let mut i = 0;
    while i < buf.chars.len() {
        if buf.chars[i] != '%' {
            i += 1;
            continue;
        }
        match buf.chars.get(i + 1) {
            None => return,
            Some('-') | Some('\u{2013}') => i += 1,
            Some(_) => {
                insert_char_at(buf, i + 1, BREAKING_PLACEHOLDER);
                i += 3;
            }
        }
    }
}

fn replace_literal_pair(buf: &mut Buf, needle: &str, replacement: &[Option<char>]) {
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut i = 0;
    while i + needle_chars.len() <= buf.chars.len() {
        if buf.chars[i..i + needle_chars.len()] == needle_chars[..] {
            for (j, repl) in replacement.iter().enumerate() {
                if let Some(c) = repl {
                    buf.chars[i + j] = *c;
                }
            }
            i += needle_chars.len();
        } else {
            i += 1;
        }
    }
}

fn replace_urls(buf: &mut Buf, url_re: &Regex) {
    let mut code = URL_START_REPLACE_CHAR;
    loop {
        let text = buf.text();
        let Some(m) = url_re.find(&text) else {
            return;
        };
        let cs = char_index(&buf.chars, m.start());
        let ce = char_index(&buf.chars, m.end());
        let url: String = buf.chars[cs..ce].iter().collect();
        let span = Span {
            start: buf.spans[cs].start,
            end: buf.spans[ce - 1].end,
        };
        let placeholder = char::from_u32(code).expect("placeholder char");
        code += 1;
        buf.urls.push((placeholder, url));
        buf.chars.splice(cs..ce, [placeholder]);
        buf.spans.splice(cs..ce, [span]);
    }
}

fn apply_decimal_space(buf: &mut Buf, re: &Regex) {
    let mut scan_from = 0usize;
    loop {
        let text = buf.text();
        let Some(m) = re.find_at(&text, scan_from.min(text.len())) else {
            return;
        };
        let cs = char_index(&buf.chars, m.start());
        let ce = char_index(&buf.chars, m.end());
        let prev_ok = cs == 0 || buf.chars[cs - 1] == '(' || is_h_or_v(buf.chars[cs - 1]);
        let next_ok = ce >= buf.chars.len() || buf.chars[ce] == '(' || is_h_or_v(buf.chars[ce]);
        if !prev_ok || !next_ok {
            scan_from = byte_of_char(&buf.chars, cs + 1);
            if scan_from >= text.len() {
                return;
            }
            continue;
        }
        for idx in cs..ce {
            if matches!(buf.chars[idx], ' ' | '\u{00A0}' | '\u{202F}') {
                buf.chars[idx] = NON_BREAKING_SPACE_SUBST;
            }
        }
        scan_from = byte_of_char(&buf.chars, ce);
        if scan_from >= buf.text().len() {
            return;
        }
    }
}

fn fix_angle_slashes(buf: &mut Buf) {
    for i in 0..buf.chars.len() {
        if buf.chars[i] == LEFT_ANGLE_SUBST && buf.chars.get(i + 1) == Some(&'/') {
            buf.chars[i + 1] = SLASH_SUBST;
            buf.spans[i + 1] = INSERTED;
        }
    }
    for i in 0..buf.chars.len() {
        if buf.chars[i] == '/' && buf.chars.get(i + 1) == Some(&RIGHT_ANGLE_SUBST) {
            buf.chars[i] = SLASH_SUBST;
            buf.spans[i] = INSERTED;
        }
    }
}

fn wrap_soft_hyphens(buf: &mut Buf) {
    let mut i = 0;
    while i + 1 < buf.chars.len() {
        if buf.chars[i] == '\u{00AD}' && buf.chars[i + 1] == '\n' {
            if !(i > 0 && is_java_space(buf.chars[i - 1])) {
                let span = Span {
                    start: buf.spans[i].start,
                    end: buf.spans[i + 1].end,
                };
                buf.chars.splice(i..i + 2, [SOFT_HYPHEN_WRAP_SUBST]);
                buf.spans.splice(i..i + 2, [span]);
            }
            i += 1;
        }
        i += 1;
    }
}

pub struct UkrainianWordTokenizer;

impl UkrainianWordTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_tokenizer::WordTokenizer for UkrainianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        tokenize_strings(text)
            .into_iter()
            .map(|(s, start, end)| Token::new(s, start, end))
            .collect()
    }
}

pub(crate) fn tokenize_strings(text: &str) -> Vec<(String, usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut buf = Buf::new(text);
    if !text.chars().all(|c| c <= '\u{0020}') {
        RULES.apply_all(&mut buf);
    }

    let mut tokens = Vec::new();
    let mut pos = 0usize;
    while pos < buf.chars.len() {
        match split_match_len(&buf.chars, pos) {
            Some(len) => {
                push_token(&buf, pos, pos + len, &mut tokens);
                pos += len;
            }
            None => {
                let start = pos;
                pos += 1;
                while pos < buf.chars.len() && split_match_len(&buf.chars, pos).is_none() {
                    pos += 1;
                }
                push_token(&buf, start, pos, &mut tokens);
            }
        }
    }
    tokens
}

fn push_token(buf: &Buf, from: usize, to: usize, tokens: &mut Vec<(String, usize, usize)>) {
    let raw: String = buf.chars[from..to].iter().collect();
    if raw == BREAKING_PLACEHOLDER.to_string() {
        return;
    }
    let mut text = raw
        .replace(DECIMAL_COMMA_SUBST, ",")
        .replace(NON_BREAKING_SLASH_SUBST, "/")
        .replace(NON_BREAKING_COLON_SUBST, ":")
        .replace(NON_BREAKING_SPACE_SUBST, " ")
        .replace(LEFT_BRACE_SUBST, "(")
        .replace(RIGHT_BRACE_SUBST, ")")
        .replace(LEFT_ANGLE_SUBST, "<")
        .replace(RIGHT_ANGLE_SUBST, ">")
        .replace(SLASH_SUBST, "/")
        .replace(NON_BREAKING_DOT_SUBST, ".")
        .replace(SOFT_HYPHEN_WRAP_SUBST, "\u{00AD}\n")
        .replace([NON_BREAKING_PLACEHOLDER, NON_BREAKING_PLACEHOLDER2], "");
    for (placeholder, url) in &buf.urls {
        text = text.replace(*placeholder, url);
    }
    let mut start = usize::MAX;
    let mut end = usize::MAX;
    for idx in from..to {
        let span = buf.spans[idx];
        if span == INSERTED {
            continue;
        }
        if start == usize::MAX {
            start = span.start;
        }
        end = span.end;
    }
    if start == usize::MAX {
        return;
    }
    tokens.push((text, start, end));
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn check(text: &str, expected: &[&str]) {
        let got: Vec<String> = UkrainianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect();
        let exp: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(got, exp, "tokenizing {text:?}");
    }

    #[test]
    fn pins_the_java_word_tokenizer_test_corpus() {
        let cases: &[(&str, &[&str])] = &[
            (
                "Вони прийшли додому.",
                &["Вони", " ", "прийшли", " ", "додому", "."],
            ),
            (
                "надійшло 2,2 мільйона",
                &["надійшло", " ", "2,2", " ", "мільйона"],
            ),
            (
                "2 000 тон з 12 000 відер",
                &["2 000", " ", "тон", " ", "з", " ", "12 000", " ", "відер"],
            ),
            (
                "надійшло 12 000 000 тон",
                &["надійшло", " ", "12 000 000", " ", "тон"],
            ),
            (
                "надійшло 12\u{202F}000\u{202F}000 тон",
                &["надійшло", " ", "12 000 000", " ", "тон"],
            ),
            (
                "до 01.01.42 400 000 шт.",
                &["до", " ", "01.01.42", " ", "400 000", " ", "шт."],
            ),
            ("2 15 мільярдів", &["2", " ", "15", " ", "мільярдів"]),
            (
                "з 12 0001 відер",
                &["з", " ", "12", " ", "0001", " ", "відер"],
            ),
            (
                "сталося 14.07.2001 вночі",
                &["сталося", " ", "14.07.2001", " ", "вночі"],
            ),
            (
                "вчора о 7.30 ранку",
                &["вчора", " ", "о", " ", "7.30", " ", "ранку"],
            ),
            (
                "вчора о 7:30 ранку",
                &["вчора", " ", "о", " ", "7:30", " ", "ранку"],
            ),
            (
                "3,5-5,6% 7° 7,4°С",
                &["3,5-5,6", "%", " ", "7", "°", " ", "7,4", "°", "С"],
            ),
            (
                "від 12 до14 років",
                &["від", " ", "12", " ", "до", "14", " ", "років"],
            ),
            ("до14-15", &["до", "14-15"]),
            ("Т.Шевченка53", &["Т.", "Шевченка", "53"]),
            ("«Мак2»", &["«", "Мак2", "»"]),
            ("км2", &["км", "2"]),
            ("Мі17", &["Мі", "17"]),
            ("000ххх000", &["000ххх000"]),
            ("+20", &["+", "20"]),
            ("-20", &["-", "20"]),
            ("–20", &["\u{2013}", "20"]),
            ("прислівник+займенник", &["прислівник", "+", "займенник"]),
            ("+займенник", &["+", "займенник"]),
            ("Роттердам+ ", &["Роттердам+", " "]),
            ("дружини¹", &["дружини", "¹"]),
            ("км²", &["км", "²"]),
            ("примітка³²", &["примітка", "³", "²"]),
            ("X²", &["X²"]),
            (
                "Вони прийшли пʼятими зів’ялими.",
                &[
                    "Вони",
                    " ",
                    "прийшли",
                    " ",
                    "п'ятими",
                    " ",
                    "зів'ялими",
                    ".",
                ],
            ),
            (
                "я українець(сміється",
                &["я", " ", "українець", "(", "сміється"],
            ),
            ("ОУН(б) та КП(б)У", &["ОУН(б)", " ", "та", " ", "КП(б)У"]),
            (
                "Негода є... заступником",
                &["Негода", " ", "є", "...", " ", "заступником"],
            ),
            ("Запагубили!.. також", &["Запагубили", "!..", " ", "також"]),
            ("— Гм.", &["—", " ", "Гм", "."]),
            ("стін\u{00AD}ку", &["стін\u{00AD}ку"]),
            ("стін\u{00AD}\nку", &["стін\u{00AD}\nку"]),
            ("п\"яний", &["п\"яний"]),
            ("▶Трансформація", &["▶", "Трансформація"]),
            ("усмішку😁", &["усмішку", "😁"]),
            (
                "http://youtube.com:80/herewego?start=11&quality=high%3F ",
                &[
                    "http://youtube.com:80/herewego?start=11&quality=high%3F",
                    " ",
                ],
            ),
            (" http://example.org", &[" ", "http://example.org"]),
            ("www.example.org", &["www.example.org"]),
            ("elect@ombudsman.gov.ua", &["elect@ombudsman.gov.ua"]),
            (
                "https://www.phpbb.com/downloads/\">сторінку",
                &["https://www.phpbb.com/downloads/", "\"", ">", "сторінку"],
            ),
            ("<sup>3</sup>", &["<sup>", "3", "</sup>"]),
            (
                "Засідав І.Єрмолюк.",
                &["Засідав", " ", "І.", "Єрмолюк", "."],
            ),
            (
                "Засідав І.   Єрмолюк.",
                &["Засідав", " ", "І.", " ", " ", " ", "Єрмолюк", "."],
            ),
            (
                "Засідав І. П. Єрмолюк.",
                &["Засідав", " ", "І.", " ", "П.", " ", "Єрмолюк", "."],
            ),
            (
                "Засідав І.П.Єрмолюк.",
                &["Засідав", " ", "І.", "П.", "Єрмолюк", "."],
            ),
            ("І.\u{00A0}Єрмолюк.", &["І.", "\u{00A0}", "Єрмолюк", "."]),
            (
                "Засідав Єрмолюк І.",
                &["Засідав", " ", "Єрмолюк", " ", "І."],
            ),
            (
                "Засідав Єрмолюк І. П.",
                &["Засідав", " ", "Єрмолюк", " ", "І.", " ", "П."],
            ),
            (
                "Засідав Єрмолюк І. та інші",
                &["Засідав", " ", "Єрмолюк", " ", "І.", " ", "та", " ", "інші"],
            ),
            (
                "140 тис. працівників",
                &["140", " ", "тис.", " ", "працівників"],
            ),
            (
                "450 тис. 297 грн",
                &["450", " ", "тис.", " ", "297", " ", "грн"],
            ),
            ("297 грн...", &["297", " ", "грн", "..."]),
            ("297 грн.", &["297", " ", "грн", "."]),
            ("450 тис.", &["450", " ", "тис."]),
            ("450 тис.\n", &["450", " ", "тис.", "\n"]),
            ("354\u{202F}тис.", &["354", "\u{202F}", "тис."]),
            (
                "911 тис.грн. з бюджету",
                &["911", " ", "тис.", "грн", ".", " ", "з", " ", "бюджету"],
            ),
            (
                "за $400\n  тис., здавалося б",
                &[
                    "за",
                    " ",
                    "$",
                    "400",
                    "\n",
                    " ",
                    " ",
                    "тис.",
                    ",",
                    " ",
                    "здавалося",
                    " ",
                    "б",
                ],
            ),
            (
                "найважчого жанру— оповідання",
                &["найважчого", " ", "жанру", "—", " ", "оповідання"],
            ),
            ("\u{2015}оповідання", &["\u{2015}", "оповідання"]),
            ("проф. Артюхов", &["проф.", " ", "Артюхов"]),
            ("чл.-кор. Артюхов", &["чл.-кор.", " ", "Артюхов"]),
            ("проф.\u{00A0}Артюхов", &["проф.", "\u{00A0}", "Артюхов"]),
            ("Ів. Франко", &["Ів.", " ", "Франко"]),
            (
                "кутю\u{00A0}— щедру",
                &["кутю", "\u{00A0}", "—", " ", "щедру"],
            ),
            (
                "також зав. відділом",
                &["також", " ", "зав.", " ", "відділом"],
            ),
            ("до н. е.", &["до", " ", "н.", " ", "е."]),
            ("до н.е.", &["до", " ", "н.", "е."]),
            ("в. о. начальника", &["в.", " ", "о.", " ", "начальника"]),
            ("в.о. начальника", &["в.", "о.", " ", "начальника"]),
            ("100 к.с.", &["100", " ", "к.", "с."]),
            ("1998 р.н.", &["1998", " ", "р.", "н."]),
            ("22 коп.", &["22", " ", "коп."]),
            ("800 гр. м'яса", &["800", " ", "гр.", " ", "м'яса"]),
            (
                "18-19 ст.ст. були",
                &["18-19", " ", "ст.", "ст.", " ", "були"],
            ),
            ("І ст. 11", &["І", " ", "ст.", " ", "11"]),
            ("куб. м", &["куб.", " ", "м"]),
            ("куб.м", &["куб.", "м"]),
            ("ам. долл", &["ам.", " ", "долл"]),
            ("4 дол.", &["4", " ", "дол."]),
            ("св. ап. Петра", &["св.", " ", "ап.", " ", "Петра"]),
            ("У с. Вижва", &["У", " ", "с.", " ", "Вижва"]),
            ("оз. Вижва", &["оз.", " ", "Вижва"]),
            (
                "Довжиною 30 см. з гаком.",
                &[
                    "Довжиною",
                    " ",
                    "30",
                    " ",
                    "см",
                    ".",
                    " ",
                    "з",
                    " ",
                    "гаком",
                    ".",
                ],
            ),
            (
                "Довжиною 30 см. Поїхали.",
                &["Довжиною", " ", "30", " ", "см", ".", " ", "Поїхали", "."],
            ),
            (
                "100 м. дороги.",
                &["100", " ", "м", ".", " ", "дороги", "."],
            ),
            ("в м.Київ", &["в", " ", "м.", "Київ"]),
            (
                "На висоті 4000 м...",
                &["На", " ", "висоті", " ", "4000", " ", "м", "..."],
            ),
            (
                "№47 (м. Слов'янськ)",
                &["№", "47", " ", "(", "м.", " ", "Слов'янськ", ")"],
            ),
            ("с.-г.", &["с.-г."]),
            (
                "100 грн. в банк",
                &["100", " ", "грн", ".", " ", "в", " ", "банк"],
            ),
            ("таке та ін.", &["таке", " ", "та", " ", "ін."]),
            ("і т. ін.", &["і", " ", "т.", " ", "ін."]),
            ("і т.д.", &["і", " ", "т.", "д."]),
            ("в т. ч.", &["в", " ", "т.", " ", "ч."]),
            (
                "до т. зв. сальону",
                &["до", " ", "т.", " ", "зв.", " ", "сальону"],
            ),
            (
                "(т. зв. сальон)",
                &["(", "т.", " ", "зв.", " ", "сальон", ")"],
            ),
            (" і под.", &[" ", "і", " ", "под."]),
            (
                "Інститут ім. акад. Вернадського.",
                &[
                    "Інститут",
                    " ",
                    "ім.",
                    " ",
                    "акад.",
                    " ",
                    "Вернадського",
                    ".",
                ],
            ),
            (
                "Палац ім. гетьмана Скоропадського.",
                &[
                    "Палац",
                    " ",
                    "ім.",
                    " ",
                    "гетьмана",
                    " ",
                    "Скоропадського",
                    ".",
                ],
            ),
            ("від лат. momento", &["від", " ", "лат.", " ", "momento"]),
            (
                "отримав рос. орден",
                &["отримав", " ", "рос.", " ", "орден"],
            ),
            (
                "на 1-кімн. кв. в центрі",
                &["на", " ", "1-кімн.", " ", "кв.", " ", "в", " ", "центрі"],
            ),
            ("1 кв. км.", &["1", " ", "кв.", " ", "км", "."]),
            (
                "Валерій (міліціонер-пародист.\n–  Авт.) стане пародистом.",
                &[
                    "Валерій",
                    " ",
                    "(",
                    "міліціонер-пародист",
                    ".",
                    "\n",
                    "–",
                    " ",
                    " ",
                    "Авт.",
                    ")",
                    " ",
                    "стане",
                    " ",
                    "пародистом",
                    ".",
                ],
            ),
            (
                "Сьогодні (у четвер.  — Ред.), вранці.",
                &[
                    "Сьогодні",
                    " ",
                    "(",
                    "у",
                    " ",
                    "четвер",
                    ".",
                    " ",
                    " ",
                    "—",
                    " ",
                    "Ред.",
                    ")",
                    ",",
                    " ",
                    "вранці",
                    ".",
                ],
            ),
            ("яку авт. устиг", &["яку", " ", "авт.", " ", "устиг"]),
            ("пише ред. Бойків", &["пише", " ", "ред.", " ", "Бойків"]),
            ("диво з див.", &["диво", " ", "з", " ", "див", "."]),
            ("диво з див...", &["диво", " ", "з", " ", "див", "..."]),
            ("тел.: 044-425-20-63", &["тел.", ":", " ", "044-425-20-63"]),
            ("с/г", &["с/г"]),
            ("ім.Василя", &["ім.", "Василя"]),
            ("ст.231", &["ст.", "231"]),
            ("2016-2017рр.", &["2016-2017", "рр."]),
            ("30.04.2010р.", &["30.04.2010", "р."]),
            (
                "ні могили 6в. ",
                &["ні", " ", "могили", " ", "6в", ".", " "],
            ),
            ("в... одягненому", &["в", "...", " ", "одягненому"]),
            ("10 млн. чоловік", &["10", " ", "млн.", " ", "чоловік"]),
            (
                "від Таврійської губ.5",
                &["від", " ", "Таврійської", " ", "губ.", "5"],
            ),
            (
                "від червоних губ.",
                &["від", " ", "червоних", " ", "губ", "."],
            ),
            ("К.-Святошинський", &["К.-Святошинський"]),
            ("К.-Г. Руффман", &["К.-Г.", " ", "Руффман"]),
            ("Рис. 10", &["Рис.", " ", "10"]),
            ("худ. фільм", &["худ.", " ", "фільм"]),
            (
                "рік нар. невідомий",
                &["рік", " ", "нар.", " ", "невідомий"],
            ),
            ("нар. 1945", &["нар.", " ", "1945"]),
            (
                "(1995 р. нар.)",
                &["(", "1995", " ", "р.", " ", "нар.", ")"],
            ),
            ("нар. бл. 1720", &["нар.", " ", "бл.", " ", "1720"]),
            (
                "(нар. у серпні 1904)",
                &["(", "нар.", " ", "у", " ", "серпні", " ", "1904", ")"],
            ),
            (
                "977 — нар. Кріс Мартін",
                &["977", " ", "—", " ", "нар.", " ", "Кріс", " ", "Мартін"],
            ),
            (
                "Ради нар. депутатів",
                &["Ради", " ", "нар.", " ", "депутатів"],
            ),
            ("нар. арт.", &["нар.", " ", "арт", "."]),
            ("біля нар. Сумно", &["біля", " ", "нар", ".", " ", "Сумно"]),
            ("- Вибори-2019", &["-", " ", "Вибори-2019"]),
            ("порівн. з англ", &["порівн.", " ", "з", " ", "англ"]),
            (
                "поч. 1945 - кін. 1946",
                &["поч.", " ", "1945", " ", "-", " ", "кін.", " ", "1946"],
            ),
            ("Поч. XX ст.", &["Поч.", " ", "XX", " ", "ст."]),
            (
                "Чигиринський пов. Такої губернії",
                &["Чигиринський", " ", "пов.", " ", "Такої", " ", "губернії"],
            ),
            ("Чигиринський пов.", &["Чигиринський", " ", "пов."]),
            ("З пов. Горобець", &["З", " ", "пов.", " ", "Горобець"]),
            ("пом. 1994", &["пом.", " ", "1994"]),
            (
                "щиколоток. с. жел",
                &["щиколоток", ".", " ", "с.", " ", "жел"],
            ),
            ("2-ге вид.,", &["2-ге", " ", "вид.", ","]),
            ("наук.-метод.", &["наук.-метод."]),
            ("д[окто]р[ом]", &["д[окто]р[ом]"]),
            (
                "\u{2019}продукти харчування\u{2019}",
                &["'", "продукти", " ", "харчування", "'"],
            ),
            ("схема 'гроші'", &["схема", " ", "'", "гроші", "'"]),
            (
                "(\u{2018}дзеркало\u{2019})",
                &["(", "'", "дзеркало", "'", ")"],
            ),
            ("все 'дно піду", &["все", " ", "'дно", " ", "піду"]),
            (
                "трохи 'дно 'дному сказано",
                &["трохи", " ", "'дно", " ", "'дному", " ", "сказано"],
            ),
            ("а мо',", &["а", " ", "мо'", ","]),
            ("підемо'", &["підемо", "'"]),
            ("ЗДОРОВ\u{2019}Я.", &["ЗДОРОВ'Я", "."]),
            ("''український''", &["''", "український", "''"]),
            ("'є", &["'", "є"]),
            ("'(є)", &["'", "(", "є", ")"]),
            ("Кан\u{2019}-Ка Но Рей", &["Кан'-Ка", " ", "Но", " ", "Рей"]),
            (
                "і екс-«депутат» вибув",
                &["і", " ", "екс-«депутат»", " ", "вибув"],
            ),
            (
                "тих \"200\"-х багато",
                &["тих", " ", "\"200\"-х", " ", "багато"],
            ),
            ("«діди»-українці", &["«діди»-українці"]),
            ("вересні--жовтні", &["вересні", "--", "жовтні"]),
            ("—У певному", &["—", "У", " ", "певному"]),
            ("-У певному", &["-", "У", " ", "певному"]),
            ("праця—голова", &["праця", "—", "голова"]),
            ("Людина—", &["Людина", "—"]),
            ("Х–ХІ", &["Х", "–", "ХІ"]),
            ("VII-VIII", &["VII", "-", "VIII"]),
            ("Стрий– ", &["Стрий", "–", " "]),
            (
                "фіто– та термотерапії",
                &["фіто–", " ", "та", " ", "термотерапії"],
            ),
            (" –Виділено", &[" ", "–", "Виділено"]),
            ("так,–так", &["так", ",", "–", "так"]),
            ("а%його", &["а", "%", "його"]),
            ("5%-го", &["5%-го"]),
            ("5\u{2032}", &["5", "\u{2032}"]),
            ("'\u{26AA}'", &["'", "\u{26AA}", "'"]),
            ("_60-річний_", &["_", "60-річний", "_"]),
            (
                "**25 жінок України:**",
                &["**", "25", " ", "жінок", " ", "України", ":", "**"],
            ),
            ("Веретениця**", &["Веретениця", "**"]),
            ("мові***,", &["мові", "***", ","]),
            ("*Оренбург", &["*", "Оренбург"]),
            ("з*ясував", &["з*ясував"]),
            ("#робота_редактора", &["#робота_редактора"]),
            (
                "https://uk.wikipedia.org/wiki/Список_аеропортів_України",
                &["https://uk.wikipedia.org/wiki/Список_аеропортів_України"],
            ),
            ("ОСОБА_5", &["ОСОБА_5"]),
        ];
        for (text, expected) in cases {
            check(text, expected);
        }
    }

    #[test]
    fn token_offsets_index_the_original_text() {
        let texts = [
            "Вони прийшли додому.",
            "Засідав І. П. Єрмолюк.",
            "до 01.01.42 400 000 шт.",
            "Валерій (міліціонер-пародист.\n–  Авт.) стане пародистом.",
            "911 тис.грн. з бюджету",
            "тих \"200\"-х багато",
            "д[окто]р[ом]",
            "тел.: 044-425-20-63",
            "і екс-«депутат» вибув",
            "5%-го",
            "стін\u{00AD}ку",
            "2-ге вид.,",
            "- Вибори-2019",
            "в м.Київ",
            "Кан'-Ка Но Рей",
            "https://uk.wikipedia.org/wiki/Список_аеропортів_України і ще",
            "на 1-кімн. кв. в центрі",
        ];
        for text in texts {
            for token in UkrainianWordTokenizer::new().tokenize(text) {
                let (start, end) = (token.start(), token.end());
                assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
                let slice = &text[start..end];
                let normalized = slice
                    .replace(['\u{2019}', '\u{02BC}', '\u{2018}'], "'")
                    .replace('\u{201A}', ",")
                    .replace('\u{2011}', "-");
                assert!(
                    token.text() == slice || token.text() == normalized,
                    "token {:?} must span its original text: {slice:?} at {start}..{end}",
                    token.text()
                );
            }
        }
    }
}
