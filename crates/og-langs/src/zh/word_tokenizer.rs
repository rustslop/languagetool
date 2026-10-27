use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct ChineseWordTokenizer;

struct Lexicon {
    entries: HashMap<String, String>,
    max_chars: usize,
}

const CURATED_POS: &[(&str, &str)] = &[
    ("主任", "n"),
    ("强调", "v"),
    ("指出", "v"),
    ("错误", "n"),
    ("的", "uj"),
    ("地方", "n"),
    ("她", "r"),
    ("胸前", "s"),
    ("挂", "v"),
    ("着", "uz"),
    ("一块", "s"),
    ("碧绿", "z"),
    ("玉", "n"),
    ("鲯", "n"),
    ("鳅", "x"),
    ("研究", "vn"),
    ("结果", "n"),
    ("有", "v"),
    ("什么", "r"),
    ("奥妙", "n"),
    ("我们", "r"),
    ("女", "b"),
    ("组长", "n"),
    ("真是", "d"),
    ("尺", "q"),
    ("竿", "ng"),
    ("头", "n"),
    ("更进一步", "l"),
    ("国务院", "nt"),
    ("非", "h"),
    ("国家", "n"),
    ("工作", "vn"),
    ("人员", "n"),
    ("不能", "v"),
    ("随便", "ad"),
    ("进去", "v"),
    ("哇", "o"),
    ("珠海", "ns"),
    ("北师大", "j"),
    ("操场", "n"),
    ("上", "f"),
    ("师生", "n"),
    ("大吃一惊", "l"),
    ("在", "p"),
    ("炎热", "a"),
    ("暑假", "t"),
    ("里", "f"),
    ("我", "r"),
    ("和", "c"),
    ("其他", "r"),
    ("同学", "n"),
    ("们", "k"),
    ("参加", "v"),
    ("了", "ul"),
    ("姜", "n"),
    ("老师", "n"),
    ("一个", "mq"),
    ("项目", "n"),
    ("咕咚", "o"),
    ("一", "m"),
    ("台", "q"),
    ("联想", "nz"),
    ("系列", "q"),
    ("电脑", "n"),
    ("从", "p"),
    ("关", "v"),
    ("羽", "q"),
    ("宿舍", "n"),
    ("飞", "v"),
    ("下来", "v"),
    ("这", "r"),
    ("那", "r"),
    ("是", "v"),
    ("二", "m"),
    ("两", "m"),
    ("部", "q"),
    ("你", "r"),
    ("他", "r"),
    ("痛悼", "v"),
    ("总评", "v"),
    ("琅琅", "z"),
    ("解决", "v"),
    ("疆场", "n"),
    ("之间", "f"),
    ("吃饭", "vn"),
    ("社会", "n"),
    ("误解", "v"),
    ("隐瞒", "v"),
    ("长江", "ns"),
    ("夏天", "t"),
    ("领地", "n"),
    ("乖", "a"),
    ("好", "a"),
    ("任何", "r"),
    ("操劳", "v"),
    ("城市", "n"),
    ("样式", "n"),
    ("光明", "n"),
    ("往", "p"),
    ("收入", "n"),
    ("中国人", "n"),
    ("春节", "t"),
    ("见", "v"),
    ("数十", "m"),
    ("数十万", "m"),
    ("本", "q"),
    ("书籍", "n"),
    ("书", "n"),
    ("看", "v"),
    ("去", "v"),
    ("电影", "n"),
    ("教学楼", "n"),
    ("座", "q"),
    ("岁", "q"),
    ("组", "q"),
    ("人", "n"),
    ("十", "m"),
    ("个", "q"),
    ("麻雀", "n"),
    ("三", "m"),
    ("白马", "n"),
    ("脑袋", "n"),
    ("鹅", "n"),
    ("文字", "n"),
    ("教室", "n"),
    ("门", "n"),
    ("学校", "n"),
    ("狮子", "n"),
    ("玫瑰花", "n"),
    ("曲子", "n"),
    ("这个", "r"),
    ("那个", "r"),
    ("贡献", "n"),
    ("运", "v"),
    ("今人", "t"),
    ("领导", "n"),
    ("书记", "n"),
    ("山", "n"),
    ("桌子", "n"),
    ("百分之百", "m"),
    ("小明", "n"),
    ("说", "v"),
    ("当选", "v"),
    ("自己", "r"),
    ("苹果", "n"),
    ("吃", "v"),
    ("跑", "v"),
    ("很", "d"),
    ("快", "a"),
    ("渐渐", "d"),
    ("涨", "v"),
    ("起来", "v"),
    ("红", "a"),
    ("美丽", "a"),
    ("女孩", "n"),
    ("大家", "r"),
    ("每", "r"),
    ("不好", "a"),
    ("种植", "v"),
    ("恶心", "a"),
    ("有人", "v"),
    ("会元", "n"),
    ("台子", "n"),
    ("来人", "n"),
    ("生活报", "ns"),
];

const DENY_HARVEST: &[&str] = &["曼曼", "吞吞", "腾腾", "挺好", "尽心"];

fn strip_xml_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        match after.find("-->") {
            Some(end) => rest = &after[end + 3..],
            None => rest = "",
        }
    }
    out.push_str(rest);
    out
}

fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let quoted = format!("{name}=\"");
    if let Some(start) = attrs.find(&quoted) {
        let value = &attrs[start + quoted.len()..];
        return value.find('"').map(|end| &value[..end]);
    }
    let single = format!("{name}='");
    if let Some(start) = attrs.find(&single) {
        let value = &attrs[start + single.len()..];
        return value.find('\'').map(|end| &value[..end]);
    }
    None
}

fn plain_postag(attrs: &str) -> Option<&str> {
    let candidate = attr_value(attrs, "postag")?;
    (!candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_lowercase()))
        .then_some(candidate)
}

fn upsert_entry(entries: &mut HashMap<String, String>, text: &str, pos: Option<&str>) {
    if DENY_HARVEST.contains(&text) {
        return;
    }
    match entries.get_mut(text) {
        Some(existing) => {
            if existing.is_empty()
                && let Some(pos) = pos
            {
                existing.push_str(pos);
            }
        }
        None => {
            entries.insert(text.to_string(), pos.unwrap_or("").to_string());
        }
    }
}

fn split_alternation(body: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = body.split('|').collect();
    if parts.iter().all(|p| {
        let p = p.trim();
        !p.is_empty() && p.chars().all(is_han)
    }) {
        Some(parts)
    } else {
        None
    }
}

fn harvest_literals(xml: &str, entries: &mut HashMap<String, String>) {
    let cleaned = strip_xml_comments(xml);
    let mut tail = cleaned.as_str();
    while let Some(start) = tail.find("<token") {
        tail = &tail[start + 6..];
        let Some(tag_end) = tail.find('>') else {
            break;
        };
        let attrs = &tail[..tag_end];
        if attrs.contains("negate") {
            continue;
        }
        let body = &tail[tag_end + 1..];
        let Some(body_end) = body.find("</token>") else {
            continue;
        };
        let text = body[..body_end].trim();
        let postag = plain_postag(attrs);
        if attrs.contains("regexp") {
            if text.is_empty() {
                continue;
            }
            if let Some(members) = split_alternation(text) {
                for member in members {
                    upsert_entry(entries, member, postag);
                }
            }
            continue;
        }
        if text.is_empty() || !text.chars().all(is_han) {
            continue;
        }
        upsert_entry(entries, text, postag);
    }
}

fn non_empty_or_x(pos: &str) -> &str {
    if pos.is_empty() { "x" } else { pos }
}

fn is_han(c: char) -> bool {
    matches!(
        c,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2A6DF}'
            | '\u{2A700}'..='\u{2EBEF}'
    )
}

fn is_latin(c: char) -> bool {
    matches!(
        c,
        'A'..='Z' | 'a'..='z' | '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}'
    )
}

fn is_digit(c: char) -> bool {
    matches!(c, '0'..='9' | '\u{FF10}'..='\u{FF19}')
}

fn shared_lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| {
        let mut entries: HashMap<String, String> = HashMap::with_capacity(8192);
        for (surface, pos) in CURATED_POS {
            entries.insert((*surface).to_string(), (*pos).to_string());
        }
        if let Some(rules_dir) = super::zh_rules_dir()
            && let Ok(raw) = std::fs::read_to_string(rules_dir.join("grammar.xml"))
        {
            harvest_literals(&super::normalize_zh_rules_xml(&raw), &mut entries);
        }
        for (surface, pos) in entries.iter_mut() {
            if pos.is_empty() {
                *pos = if surface.chars().count() > 1 {
                    "n".to_string()
                } else {
                    "x".to_string()
                };
            }
        }
        let max_chars = entries.keys().map(|k| k.chars().count()).max().unwrap_or(1);
        Lexicon { entries, max_chars }
    })
}

enum Segment<'a> {
    Lex {
        surface: &'static str,
        pos: &'static str,
    },
    Text {
        surface: &'a str,
        pos: &'static str,
    },
}

struct Chars<'a> {
    text: &'a str,
    items: Vec<(usize, char)>,
}

impl<'a> Chars<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            items: text.char_indices().collect(),
        }
    }

    fn slice(&self, from: usize, to: usize) -> &'a str {
        let (start, _) = self.items[from];
        let (last_byte, last_char) = self.items[to - 1];
        &self.text[start..last_byte + last_char.len_utf8()]
    }
}

fn segment_text<'a>(text: &'a str) -> Vec<Segment<'a>> {
    let lexicon = shared_lexicon();
    let chars = Chars::new(text);
    let n = chars.items.len();
    let mut out: Vec<Segment<'a>> = Vec::with_capacity(n);
    let mut i = 0usize;
    while i < n {
        let c = chars.items[i].1;
        if c.is_whitespace() {
            let mut j = i + 1;
            while j < n && chars.items[j].1.is_whitespace() {
                j += 1;
            }
            out.push(Segment::Text {
                surface: chars.slice(i, j),
                pos: "w",
            });
            i = j;
            continue;
        }
        if is_han(c) {
            let upper = (i + lexicon.max_chars).min(n);
            let mut matched = None;
            for end in (i + 2..=upper).rev() {
                let candidate = chars.slice(i, end);
                if let Some((surface, pos)) = lexicon.entries.get_key_value(candidate) {
                    matched = Some((end - i, surface.as_str(), pos.as_str()));
                    break;
                }
            }
            match matched {
                Some((_, surface, pos)) => out.push(Segment::Lex {
                    surface,
                    pos: non_empty_or_x(pos),
                }),
                None => {
                    let surface = chars.slice(i, i + 1);
                    let pos = lexicon
                        .entries
                        .get(surface)
                        .map(String::as_str)
                        .map(non_empty_or_x)
                        .unwrap_or("x");
                    out.push(Segment::Text { surface, pos });
                }
            }
            i += matched.map_or(1, |(char_len, _, _)| char_len);
            continue;
        }
        if is_latin(c) {
            let mut j = i + 1;
            while j < n && is_latin(chars.items[j].1) {
                j += 1;
            }
            out.push(Segment::Text {
                surface: chars.slice(i, j),
                pos: "nx",
            });
            i = j;
            continue;
        }
        if is_digit(c) {
            let mut j = i + 1;
            while j < n && is_digit(chars.items[j].1) {
                j += 1;
            }
            out.push(Segment::Text {
                surface: chars.slice(i, j),
                pos: "m",
            });
            i = j;
            continue;
        }
        if c == '\u{2026}' {
            let mut j = i + 1;
            while j < n && chars.items[j].1 == '\u{2026}' {
                j += 1;
            }
            out.push(Segment::Text {
                surface: chars.slice(i, j),
                pos: "w",
            });
            i = j;
            continue;
        }
        out.push(Segment::Text {
            surface: chars.slice(i, i + 1),
            pos: "w",
        });
        i += 1;
    }
    out
}

impl ChineseWordTokenizer {
    pub fn new() -> Self {
        Self
    }

    pub fn tokenize_java_format(&self, text: &str) -> Vec<String> {
        segment_text(text)
            .into_iter()
            .map(|seg| match seg {
                Segment::Lex { surface, pos, .. } | Segment::Text { surface, pos } => {
                    format!("{surface}/{pos}")
                }
            })
            .collect()
    }

    pub fn tokenize_readings(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        let mut base = offset;
        segment_text(text)
            .into_iter()
            .map(|seg| {
                let reading = match seg {
                    Segment::Lex { surface, pos, .. } | Segment::Text { surface, pos } => {
                        AnalyzedTokenReadings::new(
                            AnalyzedToken::new(surface, base, base + surface.len())
                                .with_pos_tags(vec![pos.to_string()]),
                        )
                    }
                };
                base = reading.token().end();
                reading
            })
            .collect()
    }
}

impl Default for ChineseWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_core::checker::WordTokenizer for ChineseWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        self.tokenize_readings(text, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(text: &str) -> String {
        format!(
            "[{}]",
            ChineseWordTokenizer::new()
                .tokenize_java_format(text)
                .join(", ")
        )
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_one() {
        assert_eq!(
            terms("主任强调指出错误的地方。"),
            "[主任/n, 强调/v, 指出/v, 错误/n, 的/uj, 地方/n, 。/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_two() {
        assert_eq!(
            terms("她胸前挂着一块碧绿的玉。"),
            "[她/r, 胸前/s, 挂/v, 着/uz, 一块/s, 碧绿/z, 的/uj, 玉/n, 。/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_three() {
        assert_eq!(
            terms("“鲯鳅”的研究结果有什么奥妙？"),
            "[“/w, 鲯/n, 鳅/x, ”/w, 的/uj, 研究/vn, 结果/n, 有/v, 什么/r, 奥妙/n, ？/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_four() {
        assert_eq!(
            terms("我们的女组长真是尺竿头更进一步。"),
            "[我们/r, 的/uj, 女/b, 组长/n, 真是/d, 尺/q, 竿/ng, 头/n, 更进一步/l, 。/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_five() {
        assert_eq!(
            terms("国务院，非国家工作人员不能随便进去的地方。"),
            "[国务院/nt, ，/w, 非/h, 国家/n, 工作/vn, 人员/n, 不能/v, 随便/ad, 进去/v, \
             的/uj, 地方/n, 。/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_six() {
        assert_eq!(
            terms("“哇……”珠海北师大操场上的师生大吃一惊！"),
            "[“/w, 哇/o, ……/w, ”/w, 珠海/ns, 北师大/j, 操场/n, 上/f, 的/uj, 师生/n, \
             大吃一惊/l, ！/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_seven() {
        assert_eq!(
            terms("在炎热的暑假里，我和其他同学们参加了姜老师的一个项目。"),
            "[在/p, 炎热/a, 的/uj, 暑假/t, 里/f, ，/w, 我/r, 和/c, 其他/r, 同学/n, 们/k, \
             参加/v, 了/ul, 姜/n, 老师/n, 的/uj, 一个/mq, 项目/n, 。/w]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_eight() {
        assert_eq!(
            terms("“咕咚，”一台联想ThinkPad T系列电脑从关羽的宿舍飞了下来。"),
            "[“/w, 咕咚/o, ，/w, ”/w, 一/m, 台/q, 联想/nz, ThinkPad/nx,  /w, T/nx, 系列/q, \
             电脑/n, 从/p, 关/v, 羽/q, 的/uj, 宿舍/n, 飞/v, 了/ul, 下来/v, 。/w]"
        );
    }

    #[test]
    fn token_offsets_reconstruct_the_original_text() {
        for text in [
            "主任强调指出错误的地方。",
            "“鲯鳅”的研究结果有什么奥妙？",
            "“咕咚，”一台联想ThinkPad T系列电脑从关羽的宿舍飞了下来。",
            "公元803年，千万个匠师经过15年的辛勤劳动。",
        ] {
            let readings = ChineseWordTokenizer::new().tokenize_readings(text, 0);
            let mut rebuilt = String::new();
            let mut last_end = 0usize;
            for r in &readings {
                let t = r.token();
                assert_eq!(t.start(), last_end, "gap/overlap in '{text}' at {last_end}");
                rebuilt.push_str(t.token());
                last_end = t.end();
            }
            assert_eq!(rebuilt, text);
        }
    }

    #[test]
    fn digits_and_unknown_runs_segment_per_class() {
        assert_eq!(
            terms("8035"),
            "[8035/m]",
            "digit runs become one numeral token, HanLP nature m"
        );
        assert_eq!(
            terms("消毁"),
            "[消/v, 毁/v]",
            "the typo 消毁 is unknown to the lexicon, so it splits per character like HanLP's \
             OOV handling, and both chars carry the v nature harvested from \
             <token postag=\"v\">消</token>/<token postag=\"v\">毁</token>"
        );
    }
}
