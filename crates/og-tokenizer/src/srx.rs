use crate::SentenceTokenizer;
use og_core::Sentence;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const MAX_LOOKBEHIND_CONSTRUCT_LENGTH: usize = 100;

struct RawRule {
    breaks: bool,
    before: String,
    after: String,
}

pub struct SrxDocument {
    language_rules: Vec<(String, Vec<RawRule>)>,
    language_maps: Vec<(String, String)>,
    prepared: Mutex<HashMap<String, Arc<PreparedLanguage>>>,
}

struct CompiledRule {
    before: fancy_regex::Regex,
    after_anchored: fancy_regex::Regex,
}

struct PreparedLanguage {
    break_rules: Vec<(CompiledRule, usize)>,
    exception_rules: Vec<CompiledRule>,
}

fn translate_pattern(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 16);
    let bytes: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            match next {
                'u' if i + 5 < bytes.len() => {
                    let hex: String = bytes[i + 2..i + 6].iter().collect();
                    if hex.chars().all(|h| h.is_ascii_hexdigit()) {
                        out.push_str(&format!("\\u{{{hex}}}"));
                        i += 6;
                        continue;
                    }
                }
                'h' => {
                    out.push_str(" \\t\\u{00A0}\\u{1680}\\u{202F}\\u{205F}\\u{3000}");
                    i += 2;
                    continue;
                }
                'v' => {
                    out.push_str("\\n\\u{0B}\\u{0C}\\u{0D}\\u{85}\\u{2028}\\u{2029}");
                    i += 2;
                    continue;
                }
                _ => {}
            }
            out.push(c);
            out.push(next);
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn finitize(pattern: &str) -> String {
    let translated = translate_pattern(pattern);
    let chars: Vec<char> = translated.chars().collect();
    let mut out = String::with_capacity(translated.len() + 16);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let escaped = i > 0 && chars[i - 1] == '\\';
        if c == '*' && !escaped {
            out.push_str(&format!("{{0,{MAX_LOOKBEHIND_CONSTRUCT_LENGTH}}}"));
            i += 1;
            continue;
        }
        if c == '+' && !escaped {
            let prev_quantifier = i >= 2
                && (chars[i - 1] == '?'
                    || chars[i - 1] == '*'
                    || chars[i - 1] == '+'
                    || chars[i - 1] == '}');
            if !prev_quantifier {
                out.push_str(&format!("{{1,{MAX_LOOKBEHIND_CONSTRUCT_LENGTH}}}"));
                i += 1;
                continue;
            }
        }
        if c == '{' && !escaped {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            let has_comma = j < chars.len() && chars[j] == ',';
            if has_comma {
                let mut k = j + 1;
                while k < chars.len() && chars[k].is_ascii_digit() {
                    k += 1;
                }
                if k < chars.len() && chars[k] == '}' && k == j + 1 {
                    let lower: String = chars[i + 1..j].iter().collect();
                    out.push_str(&format!("{{{lower},{MAX_LOOKBEHIND_CONSTRUCT_LENGTH}}}"));
                    i = k + 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn compile(pattern: &str) -> Option<fancy_regex::Regex> {
    fancy_regex::Regex::new(&translate_pattern(pattern)).ok()
}

fn compile_anchored(pattern: &str) -> Option<fancy_regex::Regex> {
    fancy_regex::Regex::new(&format!("\\A(?:{})", translate_pattern(pattern))).ok()
}

impl SrxDocument {
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let mut language_rules: Vec<(String, Vec<RawRule>)> = Vec::new();
        let mut language_maps: Vec<(String, String)> = Vec::new();
        let mut current_rule_name = String::new();
        let mut current_rule: Option<RawRule> = None;
        let mut capture: Option<&'static str> = None;
        let mut buffer = String::new();

        for event in xml_events(xml) {
            match event {
                XmlEvent::Start(name, attrs) => match name.as_str() {
                    "languagerule" => {
                        current_rule_name = attrs
                            .iter()
                            .find(|(k, _)| k == "languagerulename")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                    }
                    "rule" => {
                        let breaks = attrs
                            .iter()
                            .find(|(k, _)| k == "break")
                            .map(|(_, v)| v == "yes")
                            .unwrap_or(false);
                        current_rule = Some(RawRule {
                            breaks,
                            before: String::new(),
                            after: String::new(),
                        });
                    }
                    "beforebreak" => {
                        capture = Some("before");
                        buffer.clear();
                    }
                    "afterbreak" => {
                        capture = Some("after");
                        buffer.clear();
                    }
                    "languagemap" => {
                        let pattern = attrs
                            .iter()
                            .find(|(k, _)| k == "languagepattern")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                        let rule_name = attrs
                            .iter()
                            .find(|(k, _)| k == "languagerulename")
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                        language_maps.push((pattern, rule_name));
                    }
                    _ => {}
                },
                XmlEvent::Text(text) => buffer.push_str(&text),
                XmlEvent::End(name) => match name.as_str() {
                    "beforebreak" | "afterbreak" => {
                        let kind = capture.take().unwrap_or("before");
                        if let Some(rule) = current_rule.as_mut() {
                            if kind == "before" {
                                rule.before = xml_unescape(&buffer);
                            } else {
                                rule.after = xml_unescape(&buffer);
                            }
                        }
                        buffer.clear();
                    }
                    "rule" => {
                        if let Some(rule) = current_rule.take() {
                            if let Some(entry) = language_rules
                                .iter_mut()
                                .find(|(n, _)| *n == current_rule_name)
                            {
                                entry.1.push(rule);
                            } else {
                                language_rules.push((current_rule_name.clone(), vec![rule]));
                            }
                        }
                    }
                    _ => {}
                },
            }
        }

        if language_rules.is_empty() {
            return Err("no languagerules found".to_string());
        }
        Ok(Self {
            language_rules,
            language_maps,
            prepared: Mutex::new(HashMap::new()),
        })
    }

    fn prepared_for(&self, language_code: &str) -> Arc<PreparedLanguage> {
        if let Some(prepared) = self
            .prepared
            .lock()
            .unwrap()
            .get(language_code)
            .map(Arc::clone)
        {
            return prepared;
        }
        let mut ordered_rules: Vec<RawRule> = Vec::new();
        for (pattern, rule_name) in &self.language_maps {
            if pattern_matches(pattern, language_code)
                && let Some((_, rules)) = self.language_rules.iter().find(|(n, _)| n == rule_name)
            {
                ordered_rules.extend(rules.iter().map(|r| RawRule {
                    breaks: r.breaks,
                    before: r.before.clone(),
                    after: r.after.clone(),
                }));
            }
        }

        let mut break_rules: Vec<(CompiledRule, usize)> = Vec::new();
        let mut exception_rules: Vec<CompiledRule> = Vec::new();
        for rule in ordered_rules {
            if rule.breaks {
                let Some(before) = compile(&rule.before) else {
                    continue;
                };
                let Some(after_anchored) = compile_anchored(&rule.after) else {
                    continue;
                };
                break_rules.push((
                    CompiledRule {
                        before,
                        after_anchored,
                    },
                    exception_rules.len(),
                ));
            } else {
                let Some(finitized_before) = fancy_regex::Regex::new(&finitize(&rule.before)).ok()
                else {
                    continue;
                };
                let Some(after_anchored) = compile_anchored(&rule.after) else {
                    continue;
                };
                exception_rules.push(CompiledRule {
                    before: finitized_before,
                    after_anchored,
                });
            }
        }

        let prepared = Arc::new(PreparedLanguage {
            break_rules,
            exception_rules,
        });
        self.prepared
            .lock()
            .unwrap()
            .insert(language_code.to_string(), Arc::clone(&prepared));
        prepared
    }
}

fn pattern_matches(pattern: &str, language_code: &str) -> bool {
    fancy_regex::Regex::new(&format!("\\A(?:{pattern})\\z"))
        .ok()
        .and_then(|re| re.is_match(language_code).ok())
        .unwrap_or(false)
}

fn rule_match_positions(rule: &CompiledRule, text: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    for m in rule.before.find_iter(text).flatten() {
        let end = m.end();
        if text.is_char_boundary(end) && rule.after_anchored.is_match(&text[end..]).unwrap_or(false)
        {
            positions.push(end);
        }
    }
    positions
}

fn exception_match_positions(rule: &CompiledRule, text: &str, limit: usize) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut search_from = 0usize;
    while let Ok(Some(m)) = rule.before.find_from_pos(text, search_from) {
        if m.start() > limit {
            break;
        }
        let end = m.end();
        if text.is_char_boundary(end) && rule.after_anchored.is_match(&text[end..]).unwrap_or(false)
        {
            positions.push(end);
        }
        let mut next = m.start() + 1;
        while next < text.len() && !text.is_char_boundary(next) {
            next += 1;
        }
        search_from = next;
    }
    positions.sort_unstable();
    positions.dedup();
    positions
}

pub struct SrxSentenceSplitter {
    document: Arc<SrxDocument>,
    language_code: String,
}

impl SrxSentenceSplitter {
    pub fn new(document: Arc<SrxDocument>, language_code: &str) -> Self {
        Self {
            document,
            language_code: language_code.to_string(),
        }
    }

    pub fn from_xml(xml: &str, language_code: &str) -> Result<Self, String> {
        Ok(Self::new(
            Arc::new(SrxDocument::from_xml(xml)?),
            language_code,
        ))
    }

    pub fn segment_offsets(&self, text: &str) -> Vec<usize> {
        let prepared = self.document.prepared_for(&self.language_code);

        let mut candidates: Vec<(usize, usize)> = Vec::new();
        for (index, (rule, _)) in prepared.break_rules.iter().enumerate() {
            candidates.extend(
                rule_match_positions(rule, text)
                    .into_iter()
                    .map(|position| (position, index)),
            );
        }
        candidates.sort_unstable();
        let limit = candidates.last().map_or(0, |&(position, _)| position);

        let mut exception_positions: Vec<Option<Vec<usize>>> =
            vec![None; prepared.exception_rules.len()];
        let mut breaks: Vec<usize> = Vec::new();
        let mut start = 0usize;
        let mut examined: Option<usize> = None;
        for &(position, rule_index) in &candidates {
            if position <= start || examined == Some(position) {
                continue;
            }
            examined = Some(position);
            let exception_count = prepared.break_rules[rule_index].1;
            let blocked = (0..exception_count).any(|i| {
                let cache = &mut exception_positions[i];
                let positions = cache.get_or_insert_with(|| {
                    exception_match_positions(&prepared.exception_rules[i], text, limit)
                });
                positions.binary_search(&position).is_ok()
            });
            if !blocked {
                breaks.push(position);
                start = position;
            }
        }
        breaks
    }
}

impl SentenceTokenizer for SrxSentenceSplitter {
    fn split(&self, text: &str) -> Vec<Sentence> {
        let mut sentences = Vec::new();
        let mut start = 0usize;
        for end in self.segment_offsets(text) {
            sentences.push(Sentence::new(&text[start..end], start, end));
            start = end;
        }
        if start < text.len() {
            sentences.push(Sentence::new(&text[start..], start, text.len()));
        }
        sentences
    }
}

pub fn shared_srx_document() -> Option<&'static Arc<SrxDocument>> {
    static DOCUMENT: OnceLock<Option<Arc<SrxDocument>>> = OnceLock::new();
    DOCUMENT
        .get_or_init(|| {
            let candidates = [
                "/home/agent/opengrammar-rs/languagetool/languagetool-core/src/main/resources/org/languagetool/resource/segment.srx",
                "languagetool/languagetool-core/src/main/resources/org/languagetool/resource/segment.srx",
            ];
            candidates.iter().find_map(|path| {
                let xml = std::fs::read_to_string(path).ok()?;
                SrxDocument::from_xml(&xml).ok().map(Arc::new)
            })
        })
        .as_ref()
}

pub fn srx_splitter(language_code: &str) -> Option<SrxSentenceSplitter> {
    shared_srx_document()
        .map(|doc| SrxSentenceSplitter::new(Arc::clone(doc), &format!("{language_code}_two")))
}

enum XmlEvent {
    Start(String, Vec<(String, String)>),
    End(String),
    Text(String),
}

fn xml_events(xml: &str) -> Vec<XmlEvent> {
    let mut events = Vec::new();
    let mut i = 0;
    while let Some(open) = xml[i..].find('<') {
        let abs = i + open;
        if abs > i {
            events.push(XmlEvent::Text(xml[i..abs].to_string()));
        }
        let close = match xml[abs..].find('>') {
            Some(c) => abs + c,
            None => break,
        };
        let inner = &xml[abs + 1..close];
        i = close + 1;
        if let Some(name) = inner.strip_prefix('/') {
            events.push(XmlEvent::End(name.trim().to_string()));
        } else if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        } else {
            let (name, attrs) = parse_tag(inner);
            if name.is_empty() {
                continue;
            }
            if inner.ends_with('/') {
                events.push(XmlEvent::Start(name.clone(), attrs));
                events.push(XmlEvent::End(name));
            } else {
                events.push(XmlEvent::Start(name, attrs));
            }
        }
    }
    events
}

fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let chars: Vec<char> = inner.chars().collect();
    let mut p = 0;
    while p < chars.len() && !chars[p].is_whitespace() {
        p += 1;
    }
    let name: String = chars[..p].iter().collect();
    let mut attrs = Vec::new();
    while p < chars.len() {
        while p < chars.len() && chars[p].is_whitespace() {
            p += 1;
        }
        let key_start = p;
        while p < chars.len() && chars[p] != '=' && !chars[p].is_whitespace() {
            p += 1;
        }
        if key_start == p {
            break;
        }
        let key: String = chars[key_start..p].iter().collect();
        if p < chars.len() && chars[p] == '=' {
            p += 1;
            if p < chars.len() && (chars[p] == '"' || chars[p] == '\'') {
                let quote = chars[p];
                p += 1;
                let value_start = p;
                while p < chars.len() && chars[p] != quote {
                    p += 1;
                }
                attrs.push((key, chars[value_start..p].iter().collect()));
                p += 1;
            } else {
                let value_start = p;
                while p < chars.len() && !chars[p].is_whitespace() {
                    p += 1;
                }
                attrs.push((key, chars[value_start..p].iter().collect()));
            }
        }
    }
    (name, attrs)
}

fn xml_unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splitter() -> Option<SrxSentenceSplitter> {
        srx_splitter("en")
    }

    fn segments(text: &str) -> Vec<String> {
        let Some(s) = splitter() else {
            return Vec::new();
        };
        s.split(text).iter().map(|x| x.text().to_string()).collect()
    }

    #[test]
    fn splits_on_sentence_punctuation() {
        let got = segments("This is a test. Another one follows! What now? Yes.");
        assert_eq!(
            got,
            vec![
                "This is a test. ",
                "Another one follows! ",
                "What now? ",
                "Yes."
            ]
        );
    }

    #[test]
    fn keeps_abbreviations_together() {
        let got = segments("Mr. Smith went home. He slept.");
        assert_eq!(got, vec!["Mr. Smith went home. ", "He slept."]);
    }

    #[test]
    fn does_not_split_decimals_or_urls() {
        let got = segments("It cost 3.50 dollars. See https://example.com/page. Now.");
        assert_eq!(got.len(), 3, "{got:?}");
        let got = segments("Visit example.com. Then stop.");
        assert_eq!(got, vec!["Visit example.com. ", "Then stop."]);
    }

    #[test]
    fn single_newline_is_not_a_break_by_default() {
        let got = segments("first line\nsecond line\n\nnew paragraph");
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0], "first line\nsecond line\n\n");
    }

    #[test]
    fn translate_pattern_converts_java_escapes() {
        assert_eq!(translate_pattern("\\u00A0"), "\\u{00A0}");
        assert_eq!(
            translate_pattern("[\\h\\v]"),
            "[ \\t\\u{00A0}\\u{1680}\\u{202F}\\u{205F}\\u{3000}\\n\\u{0B}\\u{0C}\\u{0D}\\u{85}\\u{2028}\\u{2029}]"
        );
    }

    #[test]
    fn finitize_bounds_unbounded_repetition() {
        assert_eq!(finitize("ab*"), "ab{0,100}");
        assert_eq!(finitize("ab+"), "ab{1,100}");
        assert_eq!(finitize("ab{2,}"), "ab{2,100}");
        assert_eq!(finitize("a\\*b"), "a\\*b");
    }

    #[test]
    fn parses_all_language_rules_from_the_real_file() {
        let Some(doc) = shared_srx_document() else {
            return;
        };
        assert!(
            doc.language_rules.len() > 20,
            "{}",
            doc.language_rules.len()
        );
        let english = doc
            .language_rules
            .iter()
            .find(|(n, _)| n == "English")
            .map(|(_, r)| r.len())
            .unwrap_or_default();
        assert!(english > 50, "{english}");
    }

    fn de_segments(text: &str) -> Vec<String> {
        let Some(s) = srx_splitter("de") else {
            return Vec::new();
        };
        s.split(text).iter().map(|x| x.text().to_string()).collect()
    }

    #[test]
    fn german_srx_keeps_abbreviations_and_dates_together() {
        assert_eq!(
            de_segments("Dies ist ein Satz. Noch einer."),
            vec!["Dies ist ein Satz. ", "Noch einer."]
        );
        assert_eq!(
            de_segments("Dies ist, z. B., ein Satz.").len(),
            1,
            "'z. B.' must not split"
        );
        assert_eq!(
            de_segments("Das Schreiben ist auf den 3.10.2000 datiert.").len(),
            1,
            "dates must not split"
        );
        assert_eq!(
            de_segments("Orangen, Äpfel etc. werden gekauft.").len(),
            1,
            "'etc.' must not split"
        );
        assert_eq!(
            de_segments("Das gilt lt. aktuellem Plan.").len(),
            1,
            "'lt.' must not split"
        );
        assert_eq!(
            de_segments("In der 1. Bundesliga kam es zum Eklat.").len(),
            1,
            "ordinal before noun must not split"
        );
        assert_eq!(
            de_segments("Diese Periode begann im 13. oder 14. Jahrhundert und damit bla.").len(),
            1
        );
        assert_eq!(
            de_segments("Friedrich II. öfter auch bekannt als Friedrich der Große.").len(),
            1,
            "roman ordinal must not split"
        );
    }

    #[test]
    fn german_srx_splits_on_terminators_and_ellipses() {
        assert_eq!(
            de_segments("Dies ist ein Satz."),
            vec!["Dies ist ein Satz."]
        );
        assert_eq!(
            de_segments("Ein Satz! Noch einer."),
            vec!["Ein Satz! ", "Noch einer."]
        );
        assert_eq!(
            de_segments("Ein Satz... Noch einer."),
            vec!["Ein Satz... ", "Noch einer."]
        );
        assert_eq!(
            de_segments("Das ist es.. So geht es weiter."),
            vec!["Das ist es.. ", "So geht es weiter."]
        );
        assert_eq!(
            de_segments("Stimme am lautesten heraustönte …. Sobald er auf der Straße war").len(),
            2,
            "ellipsis terminates"
        );
        assert_eq!(
            de_segments("»Der Papagei ist grün.« Das kam so."),
            vec!["»Der Papagei ist grün.« ", "Das kam so."]
        );
        assert_eq!(
            de_segments("Dies ist ein Satz. \u{00A0}Noch einer.").len(),
            2,
            "non-breaking space still separates sentences"
        );
    }
}
