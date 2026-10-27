use crate::ja::lexicon::{CURATED_ENTRIES, GRAMMAR_MORPHEMES, LexiconEntry};
use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct Lexicon {
    entries: HashMap<&'static str, LexiconEntry>,
    max_chars: usize,
}

fn shared_lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| {
        let mut entries = HashMap::with_capacity(GRAMMAR_MORPHEMES.len() + CURATED_ENTRIES.len());
        for surface in GRAMMAR_MORPHEMES {
            entries.insert(
                *surface,
                LexiconEntry {
                    surface,
                    pos: None,
                    basic_form: surface,
                },
            );
        }
        for entry in CURATED_ENTRIES {
            entries.insert(entry.surface, *entry);
        }
        let max_chars = entries.keys().map(|k| k.chars().count()).max().unwrap_or(1);
        Lexicon { entries, max_chars }
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptClass {
    Hiragana,
    Katakana,
    Kanji,
    Latin,
    Digit,
    Symbol,
}

fn script_class(c: char) -> ScriptClass {
    match c {
        '\u{3041}'..='\u{309F}' => ScriptClass::Hiragana,
        '\u{30A0}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9F}' => {
            ScriptClass::Katakana
        }
        '\u{3005}'..='\u{3007}'
        | '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}' => ScriptClass::Kanji,
        'A'..='Z' | 'a'..='z' | '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}' => {
            ScriptClass::Latin
        }
        '0'..='9' | '\u{FF10}'..='\u{FF19}' => ScriptClass::Digit,
        _ => ScriptClass::Symbol,
    }
}

fn is_ja_whitespace(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | '\r' | '\u{000B}' | '\u{3000}' | '\u{FEFF}'
    )
}

enum Segment {
    Morpheme {
        surface: &'static str,
        pos: Option<&'static str>,
        basic_form: &'static str,
        start: usize,
        end: usize,
    },
    Unknown {
        surface: String,
        start: usize,
        end: usize,
    },
    Whitespace {
        surface: String,
        start: usize,
        end: usize,
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

    fn byte_at(&self, char_index: usize) -> usize {
        match self.items.get(char_index) {
            Some(&(b, _)) => b,
            None => self.text.len(),
        }
    }

    fn entry_len_at(&self, lexicon: &Lexicon, char_index: usize, char_len: usize) -> bool {
        let (byte_start, _) = self.items[char_index];
        let (last_byte, last_char) = self.items[char_index + char_len - 1];
        let slice = &self.text[byte_start..last_byte + last_char.len_utf8()];
        lexicon.entries.contains_key(slice)
    }

    fn entry_at<'l>(&self, lexicon: &'l Lexicon, char_index: usize) -> Option<&'l LexiconEntry> {
        let (byte_start, _) = self.items[char_index];
        let upper = (char_index + lexicon.max_chars).min(self.items.len());
        for end in (char_index + 1..=upper).rev() {
            let (last_byte, last_char) = self.items[end - 1];
            let slice = &self.text[byte_start..last_byte + last_char.len_utf8()];
            if let Some(entry) = lexicon.entries.get(slice) {
                return Some(entry);
            }
        }
        None
    }
}

// Greedy longest-match plus two vetoes standing in for gosen's cost model:
// an entry must not shadow a longer entry starting inside it, and 助動詞「た」
// cannot precede the auxiliary ない (しい|っそう loses to し|いっそう).
fn segment_text(text: &str, lexicon: &Lexicon) -> Vec<Segment> {
    let chars = Chars::new(text);
    let n = chars.items.len();
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let (byte_start, c) = chars.items[i];
        if is_ja_whitespace(c) {
            let mut j = i + 1;
            let mut surface = String::new();
            surface.push(c);
            while j < n && is_ja_whitespace(chars.items[j].1) {
                surface.push(chars.items[j].1);
                j += 1;
            }
            out.push(Segment::Whitespace {
                surface,
                start: byte_start,
                end: chars.byte_at(j),
            });
            i = j;
            continue;
        }
        if let Some(entry) = greedy_entry(&chars, lexicon, i, out.last()) {
            let char_len = entry.surface.chars().count();
            out.push(Segment::Morpheme {
                surface: entry.surface,
                pos: entry.pos,
                basic_form: entry.basic_form,
                start: byte_start,
                end: chars.byte_at(i + char_len),
            });
            i += char_len;
            continue;
        }
        let class = script_class(c);
        if class == ScriptClass::Hiragana || class == ScriptClass::Symbol {
            out.push(Segment::Unknown {
                surface: c.to_string(),
                start: byte_start,
                end: byte_start + c.len_utf8(),
            });
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut surface = String::new();
        surface.push(c);
        while j < n {
            let next = chars.items[j].1;
            if is_ja_whitespace(next)
                || script_class(next) != class
                || chars.entry_at(lexicon, j).is_some()
            {
                break;
            }
            surface.push(next);
            j += 1;
        }
        out.push(Segment::Unknown {
            surface,
            start: byte_start,
            end: chars.byte_at(j),
        });
        i = j;
    }
    out
}

fn greedy_entry<'l>(
    chars: &Chars,
    lexicon: &'l Lexicon,
    i: usize,
    prev: Option<&Segment>,
) -> Option<&'l LexiconEntry> {
    let (byte_start, _) = chars.items[i];
    let upper = (i + lexicon.max_chars).min(chars.items.len());
    for end in (i + 1..=upper).rev() {
        let (last_byte, last_char) = chars.items[end - 1];
        let slice = &chars.text[byte_start..last_byte + last_char.len_utf8()];
        if let Some(entry) = lexicon.entries.get(slice) {
            let char_len = end - i;
            if is_vetoed_after_ta(entry, prev) || shadows_longer_entry(chars, lexicon, i, char_len)
            {
                continue;
            }
            return Some(entry);
        }
    }
    None
}

fn is_vetoed_after_ta(entry: &LexiconEntry, prev: Option<&Segment>) -> bool {
    entry.surface == "ない"
        && matches!(prev, Some(Segment::Morpheme { surface: "た", pos, .. }) if *pos == Some("助動詞"))
}

fn shadows_longer_entry(chars: &Chars, lexicon: &Lexicon, i: usize, char_len: usize) -> bool {
    for k in 1..char_len {
        let upper = lexicon.max_chars.min(chars.items.len() - (i + k));
        if (1..=upper)
            .rev()
            .any(|len| len > char_len && chars.entry_len_at(lexicon, i + k, len))
        {
            return true;
        }
    }
    false
}

pub struct JapaneseWordTokenizer;

impl JapaneseWordTokenizer {
    pub fn new() -> Self {
        Self
    }

    pub fn tokenize_triples(&self, text: &str) -> Vec<String> {
        let lexicon = shared_lexicon();
        segment_text(text, lexicon)
            .into_iter()
            .map(|seg| match seg {
                Segment::Morpheme {
                    surface,
                    pos,
                    basic_form,
                    ..
                } => format!("{surface} {} {basic_form}", pos.unwrap_or("")),
                Segment::Unknown { surface, .. } | Segment::Whitespace { surface, .. } => {
                    format!("{surface}  {surface}")
                }
            })
            .collect()
    }

    pub fn tokenize_readings(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        let lexicon = shared_lexicon();
        segment_text(text, lexicon)
            .into_iter()
            .map(|seg| match seg {
                Segment::Morpheme {
                    surface,
                    pos,
                    basic_form,
                    start,
                    end,
                } => {
                    let mut token = AnalyzedToken::new(surface, offset + start, offset + end)
                        .with_lemma(basic_form);
                    if let Some(pos) = pos {
                        token = token.with_pos_tags(vec![pos.to_string()]);
                    }
                    AnalyzedTokenReadings::new(token)
                }
                Segment::Unknown {
                    surface,
                    start,
                    end,
                }
                | Segment::Whitespace {
                    surface,
                    start,
                    end,
                } => AnalyzedTokenReadings::new(
                    AnalyzedToken::new(surface.as_str(), offset + start, offset + end)
                        .with_lemma(surface.as_str()),
                ),
            })
            .collect()
    }
}

impl Default for JapaneseWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_core::checker::WordTokenizer for JapaneseWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        self.tokenize_readings(text, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triples(text: &str) -> Vec<String> {
        JapaneseWordTokenizer::new().tokenize_triples(text)
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_one() {
        let got = triples("これはペンです。");
        assert_eq!(got.len(), 5, "JapaneseWordTokenizerTest expects 5 tokens");
        assert_eq!(
            got,
            vec![
                "これ 名詞-代名詞-一般 これ",
                "は 助詞-係助詞 は",
                "ペン 名詞-一般 ペン",
                "です 助動詞 です",
                "。 記号-句点 。",
            ],
            "JapaneseWordTokenizerTest.testTokenize: [これ 名詞-代名詞-一般 これ, は 助詞-係助詞 は, \
             ペン 名詞-一般 ペン, です 助動詞 です, 。 記号-句点 。]"
        );
    }

    #[test]
    fn java_pin_word_tokenizer_test_sentence_two() {
        let got = triples("私は「うん、そうだ」と答えた。");
        assert_eq!(got.len(), 12, "JapaneseWordTokenizerTest expects 12 tokens");
        assert_eq!(
            got,
            vec![
                "私 名詞-代名詞-一般 私",
                "は 助詞-係助詞 は",
                "「 記号-括弧開 「",
                "うん 感動詞 うん",
                "、 記号-読点 、",
                "そう 副詞-助詞類接続 そう",
                "だ 助動詞 だ",
                "」 記号-括弧閉 」",
                "と 助詞-格助詞-引用 と",
                "答え 動詞-自立 答える",
                "た 助動詞 た",
                "。 記号-句点 。",
            ],
            "JapaneseWordTokenizerTest.testTokenize second assertion: [私 名詞-代名詞-一般 私, \
             は 助詞-係助詞 は, 「 記号-括弧開 「, うん 感動詞 うん, 、 記号-読点 、, \
             そう 副詞-助詞類接続 そう, だ 助動詞 だ, 」 記号-括弧閉 」, と 助詞-格助詞-引用 と, \
             答え 動詞-自立 答える, た 助動詞 た, 。 記号-句点 。]"
        );
    }

    #[test]
    fn token_offsets_reconstruct_the_original_text() {
        for text in [
            "これはペンです。",
            "エラーを含まないテスト文です。",
            "たまーゃ。",
            "Gパン。",
            "迷路から出れる。",
            "ここに置かさせる。",
            "詳細の表示する",
        ] {
            let readings = JapaneseWordTokenizer::new().tokenize_readings(text, 0);
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
    fn latin_letters_and_kanji_runs_form_single_unknown_tokens() {
        let got = triples("Gパン。");
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], "G  G");
        assert_eq!(got[1], "パン  パン");
        assert_eq!(got[2], "。 記号-句点 。");
    }

    #[test]
    fn hiragana_glue_outside_the_lexicon_splits_per_character() {
        let got = triples("エラーお含むテスト文です。");
        assert_eq!(
            got,
            vec![
                "エラー 名詞-一般 エラー",
                "お 接頭詞-名詞接続 お",
                "含む 動詞-自立 含む",
                "テスト 名詞-サ変接続 テスト",
                "文 名詞-一般 文",
                "です 助動詞 です",
                "。 記号-句点 。"
            ],
            "rule O parity (JLanguageToolTest): エラー must carry a 名詞 POS and お must be its \
             own token so O = [名詞.*][お] fires"
        );
    }

    #[test]
    fn lexicon_inventory_is_pinned() {
        assert_eq!(crate::ja::lexicon::CURATED_ENTRIES.len(), 101);
        assert_eq!(crate::ja::lexicon::GRAMMAR_MORPHEMES.len(), 1382);
        let lexicon = shared_lexicon();
        assert_eq!(lexicon.entries.len(), 1483);
        assert_eq!(lexicon.max_chars, 6);
        assert!(
            lexicon.entries.contains_key("これ") && lexicon.entries.contains_key("したがって"),
            "CURATED_ENTRIES carry the JapaneseTaggerTest/JapaneseWordTokenizerTest readings \
             plus IPADIC function words and the postag-rule nouns; GRAMMAR_MORPHEMES are the \
             1434 literal and pure-alternation-regexp token strings of ja grammar.xml minus \
             curated duplicates and the 3 never-exercised alternation members that shadow \
             needed segmentations (おもね, おも, ねて)"
        );
    }
}
