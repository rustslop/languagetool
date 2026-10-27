use og_spell::Dictionary;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MORFOLOGIK_RULE_SL_SI: &str = "MORFOLOGIK_RULE_SL_SI";

const JAVA_BRAND_WORDS: &[&str] = &["LanguageTool", "LanguageTooler"];

pub(crate) fn hunspell_dir() -> Option<PathBuf> {
    super::sl_resource_dir().map(|dir| dir.join("hunspell"))
}

const ISO_8859_2_HIGH: [char; 96] = [
    '\u{a0}', 'Ą', '˘', 'Ł', '¤', 'Ľ', 'Ś', '§', '¨', 'Š', 'Ş', 'Ť', 'Ź', '\u{ad}', 'Ž', 'Ż', '°',
    'ą', '˛', 'ł', '´', 'ľ', 'ś', 'ˇ', '¸', 'š', 'ş', 'ť', 'ź', '˝', 'ž', 'ż', 'Ŕ', 'Á', 'Â', 'Ă',
    'Ä', 'Ĺ', 'Ć', 'Ç', 'Č', 'É', 'Ę', 'Ë', 'Ě', 'Í', 'Î', 'Ď', 'Đ', 'Ń', 'Ň', 'Ó', 'Ô', 'Ő', 'Ö',
    '×', 'Ř', 'Ů', 'Ú', 'Ű', 'Ü', 'Ý', 'Ţ', 'ß', 'ŕ', 'á', 'â', 'ă', 'ä', 'ĺ', 'ć', 'ç', 'č', 'é',
    'ę', 'ë', 'ě', 'í', 'î', 'ď', 'đ', 'ń', 'ň', 'ó', 'ô', 'ő', 'ö', '÷', 'ř', 'ů', 'ú', 'ű', 'ü',
    'ý', 'ţ', '˙',
];

fn decode_latin2_into(bytes: &[u8], out: &mut String) {
    out.reserve(bytes.len());
    for &byte in bytes {
        if byte < 0xA0 {
            out.push(byte as char);
        } else {
            out.push(ISO_8859_2_HIGH[(byte - 0xA0) as usize]);
        }
    }
}

struct Cfsa2<'a> {
    arcs: &'a [u8],
    label_mapping: &'a [u8],
    has_numbers: bool,
}

fn read_vint(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let mut shift = 0;
    let mut value = 0usize;
    let mut pos = offset;
    loop {
        let byte = *data.get(pos)?;
        pos += 1;
        value |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Some((value, pos));
        }
        shift += 7;
    }
}

impl<'a> Cfsa2<'a> {
    fn from_bytes(data: &'a [u8]) -> Option<Self> {
        if data.len() < 8 || &data[0..4] != b"\\fsa" || data[4] != 0xc6 {
            return None;
        }
        let mut pos = 5;
        let flag_bits = u16::from_be_bytes([*data.get(pos)?, *data.get(pos + 1)?]);
        pos += 2;
        let has_numbers = flag_bits & (1 << 8) != 0;
        let label_mapping_size = *data.get(pos)? as usize;
        pos += 1;
        let label_mapping = data.get(pos..pos + label_mapping_size)?;
        pos += label_mapping_size;
        Some(Self {
            arcs: data.get(pos..)?,
            label_mapping,
            has_numbers,
        })
    }

    fn first_arc(&self, node: usize) -> usize {
        if self.has_numbers {
            read_vint(self.arcs, node).map_or(node, |(_, next)| next)
        } else {
            node
        }
    }

    fn arc_label(&self, arc: usize) -> u8 {
        let index = self.arcs[arc] & 0x1f;
        if index > 0 {
            self.label_mapping[index as usize]
        } else {
            self.arcs[arc + 1]
        }
    }

    fn is_arc_last(&self, arc: usize) -> bool {
        self.arcs[arc] & 0x40 != 0
    }

    fn is_next_set(&self, arc: usize) -> bool {
        self.arcs[arc] & 0x80 != 0
    }

    fn skip_arc(&self, arc: usize) -> usize {
        let mut offset = arc + 1;
        if self.arcs[arc] & 0x1f == 0 {
            offset += 1;
        }
        if !self.is_next_set(arc) {
            while self.arcs.get(offset).is_some_and(|b| b & 0x80 != 0) {
                offset += 1;
            }
            offset += 1;
        }
        offset
    }

    fn next_arc(&self, arc: usize) -> Option<usize> {
        if self.is_arc_last(arc) {
            None
        } else {
            Some(self.skip_arc(arc))
        }
    }

    fn destination(&self, arc: usize) -> usize {
        if self.is_next_set(arc) {
            let mut current = arc;
            while !self.is_arc_last(current) {
                current = self.next_arc(current).unwrap_or(current);
            }
            self.skip_arc(current)
        } else {
            let start = arc + if self.arcs[arc] & 0x1f == 0 { 2 } else { 1 };
            read_vint(self.arcs, start).map_or(0, |(value, _)| value)
        }
    }

    fn root(&self) -> usize {
        self.destination(self.first_arc(0))
    }

    fn is_arc_final(&self, arc: usize) -> bool {
        self.arcs[arc] & 0x20 != 0
    }

    fn walk(&self, node: usize, sequence: &mut Vec<u8>, out: &mut dyn FnMut(&[u8])) {
        let mut arc = Some(self.first_arc(node));
        while let Some(current) = arc {
            sequence.push(self.arc_label(current));
            if self.is_arc_final(current) {
                out(sequence);
            }
            let dest = self.destination(current);
            if dest != 0 {
                self.walk(dest, sequence, out);
            }
            sequence.pop();
            arc = self.next_arc(current);
        }
    }
}

fn info_value(info: &str, key: &str) -> Option<String> {
    info.lines()
        .find_map(|l| l.trim().strip_prefix(key))
        .map(str::trim)
        .map(str::to_string)
}

pub(crate) fn load_latin2_hunspell_words(dir: &Path, dict: &mut Dictionary) -> bool {
    let Ok(data) = std::fs::read(dir.join("sl_SI.dict")) else {
        return false;
    };
    let Ok(info) = std::fs::read_to_string(dir.join("sl_SI.info")) else {
        return false;
    };
    if info_value(&info, "fsa.dict.encoding=").as_deref() != Some("iso-8859-2") {
        return false;
    }
    let Some(separator) =
        info_value(&info, "fsa.dict.separator=").and_then(|sep| sep.chars().next())
    else {
        return false;
    };
    let Some(fsa) = Cfsa2::from_bytes(&data) else {
        return false;
    };
    let mut loaded = false;
    let mut word = String::new();
    fsa.walk(fsa.root(), &mut Vec::new(), &mut |sequence: &[u8]| {
        let stem = sequence
            .split(|byte| *byte == separator as u8)
            .next()
            .unwrap_or(&[]);
        if stem.is_empty() {
            return;
        }
        word.clear();
        decode_latin2_into(stem, &mut word);
        dict.add_word(&word);
        loaded = true;
    });
    loaded
}

pub fn slovenian_speller_dictionary() -> Option<Arc<Dictionary>> {
    static CACHE: og_core::releasable::Releasable<Dictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let mut dict = Dictionary::new();
        let hunspell = hunspell_dir()?;
        let mut have_words = load_latin2_hunspell_words(&hunspell, &mut dict);
        if let Ok(extra) = std::fs::read_to_string(hunspell.join("spelling.txt")) {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                    have_words = true;
                }
            }
        }
        if !have_words {
            return None;
        }
        Some(Arc::new(dict))
    })
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(dir) = hunspell_dir()
        && let Ok(words) = std::fs::read_to_string(dir.join("ignore.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_string());
            }
        }
    }
    ignore
}

pub fn slovenian_speller_rule() -> og_spell::SpellingCheckRule {
    let dictionary = slovenian_speller_dictionary().unwrap_or_else(|| Arc::new(Dictionary::new()));
    let extra_words = JAVA_BRAND_WORDS
        .iter()
        .map(|word| (*word).to_string())
        .collect();
    og_spell::SpellingCheckRule::new_shared(dictionary, MORFOLOGIK_RULE_SL_SI, extra_words)
        .with_ignore_words(shared_ignore_words())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin2_table_decodes_the_slovenian_diacritics() {
        let mut out = String::new();
        decode_latin2_into(&[0xE8, 0xB9, 0xBE, 0xC8, 0xA9, 0xAE], &mut out);
        assert_eq!(out, "čšžČŠŽ");
        let mut pairs = String::new();
        decode_latin2_into(&[0xA1, 0xB7, 0xD7, 0xF4, 0xF5], &mut pairs);
        assert_eq!(pairs, "Ąˇ×ôő");
        let mut ascii = String::new();
        decode_latin2_into(b"npr.", &mut ascii);
        assert_eq!(ascii, "npr.");
    }

    #[test]
    fn speller_dictionary_loads_the_latin2_hunspell_word_list() {
        let dict = slovenian_speller_dictionary().expect("sl hunspell dictionary");
        assert!(
            dict.contains("črkovanja"),
            "Latin-2 0xE8/0xBE bytes must decode"
        );
        assert!(dict.contains("življenje"));
        assert!(
            dict.contains("Kupil"),
            "case-insensitive via the lowercase form"
        );
        assert!(
            dict.contains("npr."),
            "the dict stores the abbreviation with its dot"
        );
        assert!(
            !dict.contains("npr"),
            "the bare stem is unknown, as in Java"
        );
        assert!(!dict.contains("xqzvję"));
    }

    #[test]
    fn speller_rule_carries_the_java_id_and_brand_words() {
        use og_core::rule::Rule as _;
        let rule = slovenian_speller_rule();
        assert_eq!(rule.id(), MORFOLOGIK_RULE_SL_SI);
        assert!(
            rule.is_known_word("LanguageTool"),
            "MorfologikSpeller.java:98"
        );
        assert!(rule.is_known_word("moko"));
        assert!(!rule.is_known_word("npr"));
        assert!(rule.is_known_word("št"), "hunspell/ignore.txt entry");
        let suggestions = rule.get_suggestions("črkovanj", 5);
        assert!(
            suggestions
                .iter()
                .all(|s| s.starts_with("črkovan") || s.starts_with("Črkovan")),
            "diacritic-safe suggestions over the Latin-2 word list: {suggestions:?}"
        );
    }
}
