use crate::disambiguator::ChainedDisambiguator;
use crate::xml_disambiguator::XmlDisambiguator;
use og_core::checker::Disambiguator;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;
const TAG_FOR_NOT_ADDING_TAGS: &str = "_NONE_";

struct MultiwordEntry {
    tag: String,
    lemma: String,
}

pub struct MultiWordChunker {
    m_start_space: HashMap<String, usize>,
    m_start_no_space: HashMap<String, usize>,
    m_full_space: HashMap<String, MultiwordEntry>,
    m_full_no_space: HashMap<String, MultiwordEntry>,
    remove_previous_tags: bool,
    add_ignore_spelling: bool,
}

fn is_camel_case(token: &str) -> bool {
    let bytes: Vec<char> = token.chars().collect();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() || !bytes[i].is_ascii_uppercase() {
        return false;
    }
    i + 1 < bytes.len() && bytes[i + 1..].iter().all(|c| c.is_ascii_alphabetic())
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn load_lines(content: &str) -> (String, Vec<String>) {
    let mut separator = "\t".to_string();
    let mut lines = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("#separatorRegExp=") {
            separator = rest.to_string();
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let expanded = expand_german_line(line);
        lines.extend(expanded);
    }
    (separator, lines)
}

fn split_line<'a>(line: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts: Vec<&str> = if let Some(class) = separator
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
    {
        let chars = unescape_class(class);
        line.split(|c| chars.contains(&c)).collect()
    } else {
        line.split(separator).collect()
    };
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}

fn unescape_class(class: &str) -> Vec<char> {
    let mut chars = Vec::new();
    let mut it = class.chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.next() {
                Some('t') => chars.push('\t'),
                Some('n') => chars.push('\n'),
                Some('r') => chars.push('\r'),
                Some(esc) => chars.push(esc),
                None => {}
            },
            _ => chars.push(c),
        }
    }
    chars
}

fn expand_german_line(line: &str) -> Vec<String> {
    let ends_with_esn = line.chars().last().is_some_and(|c| "ESN".contains(c));
    if line.contains('/') && ends_with_esn {
        let mut parts = line.split('/');
        let stem = parts.next().unwrap_or("").trim();
        let flags = parts.next().unwrap_or("");
        let mut out = vec![stem.to_string()];
        if flags.contains('E') {
            out.push(format!("{stem}e"));
        }
        if flags.contains('S') {
            out.push(format!("{stem}s"));
        }
        if flags.contains('N') {
            out.push(format!("{stem}n"));
        }
        return out;
    }
    vec![line.to_string()]
}

impl MultiWordChunker {
    pub fn from_file(
        path: &Path,
        allow_first_capitalized: bool,
        allow_all_uppercase: bool,
        allow_titlecase: bool,
        default_tag: Option<&str>,
        remove_previous_tags: bool,
        add_ignore_spelling: bool,
    ) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        Some(Self::from_content(
            &content,
            allow_first_capitalized,
            allow_all_uppercase,
            allow_titlecase,
            default_tag,
            remove_previous_tags,
            add_ignore_spelling,
        ))
    }

    pub fn from_content(
        content: &str,
        allow_first_capitalized: bool,
        allow_all_uppercase: bool,
        allow_titlecase: bool,
        default_tag: Option<&str>,
        remove_previous_tags: bool,
        add_ignore_spelling: bool,
    ) -> Self {
        let (separator, lines) = load_lines(content);
        let mut m_start_space: HashMap<String, usize> = HashMap::new();
        let mut m_start_no_space: HashMap<String, usize> = HashMap::new();
        let mut m_full_space: HashMap<String, MultiwordEntry> = HashMap::new();
        let mut m_full_no_space: HashMap<String, MultiwordEntry> = HashMap::new();

        for line in lines {
            let parts = split_line(&line, &separator);
            let tag = match default_tag {
                Some(dt) => {
                    if parts.len() != 1 {
                        continue;
                    }
                    dt.to_string()
                }
                None => {
                    if parts.len() != 2 {
                        continue;
                    }
                    parts[1].to_string()
                }
            };
            let original = parts[0];
            let contains_space = original.find(' ').map(|i| i > 0).unwrap_or(false);
            let mut variants: Vec<String> = vec![original.to_string()];
            if allow_all_uppercase && !is_camel_case(original) {
                let upper = original.to_uppercase();
                if upper != original
                    && !m_full_no_space.contains_key(&upper)
                    && !m_full_space.contains_key(&upper)
                {
                    variants.push(upper);
                }
            }
            if allow_first_capitalized {
                let first_cap = uppercase_first_char(original);
                if first_cap != original
                    && !m_full_no_space.contains_key(&first_cap)
                    && !m_full_space.contains_key(&first_cap)
                {
                    variants.push(first_cap.clone());
                }
                if allow_titlecase && original.split(' ').count() > 1 {
                    let all_lower = original
                        .split(' ')
                        .all(|w| w.chars().next().map(|c| c.is_lowercase()).unwrap_or(false));
                    if all_lower {
                        let naive = original
                            .split(' ')
                            .map(uppercase_first_char)
                            .collect::<Vec<_>>()
                            .join(" ");
                        if naive != first_cap && naive != original {
                            variants.push(naive);
                        }
                        let _ = &first_cap;
                    }
                }
            }
            for variant in variants {
                if contains_space {
                    let tokens: Vec<&str> = variant.split(' ').collect();
                    let first_token = tokens[0].to_string();
                    let count = tokens.len();
                    let entry = m_start_space.entry(first_token).or_insert(0);
                    if *entry < count {
                        *entry = count;
                    }
                    m_full_space.insert(
                        variant.clone(),
                        MultiwordEntry {
                            tag: tag.clone(),
                            lemma: original.to_string(),
                        },
                    );
                } else {
                    let first_char = variant
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    if !first_char.is_empty() {
                        let len = variant.len();
                        let entry = m_start_no_space.entry(first_char).or_insert(0);
                        if *entry < len {
                            *entry = len;
                        }
                    }
                    m_full_no_space.insert(
                        variant.clone(),
                        MultiwordEntry {
                            tag: tag.clone(),
                            lemma: original.to_string(),
                        },
                    );
                }
            }
        }

        Self {
            m_start_space,
            m_start_no_space,
            m_full_space,
            m_full_no_space,
            remove_previous_tags,
            add_ignore_spelling,
        }
    }

    pub fn apply(&self, tokens: &mut [AnalyzedTokenReadings]) {
        let n = tokens.len();
        for i in 0..n {
            let mut tok = tokens[i].token().token().to_string();
            let mut k = i + 1;
            while k < n && !tokens[k].is_whitespace() {
                tok.push_str(tokens[k].token().token());
                k += 1;
            }
            if let Some(&max_len) = self.m_start_space.get(&tok) {
                let mut key = String::new();
                let mut final_len = 0usize;
                let mut len_counter = 0usize;
                let mut j = i;
                while j < n && j - i < MAX_TOKENS_IN_MULTIWORD {
                    if !tokens[j].is_whitespace() {
                        key.push_str(tokens[j].token().token());
                        if let Some(at) = self.m_full_space.get(&key) {
                            if at.tag != TAG_FOR_NOT_ADDING_TAGS {
                                if final_len == 0 {
                                    let tag = at.tag.clone();
                                    let lemma = at.lemma.clone();
                                    add_plain_reading(&mut tokens[i], &tag, &lemma);
                                } else {
                                    let tag = at.tag.clone();
                                    let lemma = at.lemma.clone();
                                    prepare_new_reading(&mut tokens[i], &tag, &lemma, false);
                                    let tag = at.tag.clone();
                                    let lemma = at.lemma.clone();
                                    prepare_new_reading(&mut tokens[final_len], &tag, &lemma, true);
                                }
                            }
                            if self.add_ignore_spelling {
                                let end = if final_len == 0 { i } else { final_len };
                                for token in &mut tokens[i..=end] {
                                    token.ignore_spelling();
                                }
                            }
                        }
                    } else {
                        if j > 1 && !tokens[j - 1].is_whitespace() {
                            key.push(' ');
                            len_counter += 1;
                        }
                        if len_counter == max_len {
                            break;
                        }
                    }
                    j += 1;
                    final_len = j;
                }
            }
            let first_char = match tokens[i].token().token().chars().next() {
                Some(c) => c.to_string(),
                None => String::new(),
            };
            if !first_char.is_empty() && self.m_start_no_space.contains_key(&first_char) {
                let mut key = String::new();
                let mut j = i;
                while j < n && !tokens[j].is_whitespace() && j - i < MAX_TOKENS_IN_MULTIWORD {
                    key.push_str(tokens[j].token().token());
                    if let Some(at) = self.m_full_no_space.get(&key) {
                        if at.tag != TAG_FOR_NOT_ADDING_TAGS {
                            if i == j {
                                let tag = at.tag.clone();
                                let lemma = at.lemma.clone();
                                add_plain_reading(&mut tokens[i], &tag, &lemma);
                            } else {
                                let tag = at.tag.clone();
                                let lemma = at.lemma.clone();
                                prepare_new_reading(&mut tokens[i], &tag, &lemma, false);
                                let tag = at.tag.clone();
                                let lemma = at.lemma.clone();
                                prepare_new_reading(&mut tokens[j], &tag, &lemma, true);
                            }
                        }
                        if self.add_ignore_spelling {
                            for token in &mut tokens[i..=j] {
                                token.ignore_spelling();
                            }
                        }
                    }
                    j += 1;
                }
            }
        }
        if self.remove_previous_tags {
            self.remove_previous_tags(tokens);
        }
    }

    fn remove_previous_tags(&self, tokens: &mut [AnalyzedTokenReadings]) {
        let mut next_pos_tag = String::new();
        let mut lemma = String::new();
        let mut pos_tag = String::new();
        let mut i = 0;
        while i < tokens.len() {
            if !tokens[i].is_whitespace() {
                if !next_pos_tag.is_empty() {
                    let closing = format!("</{pos_tag}>");
                    let is_last = has_pos_tag_and_lemma(&tokens[i], &closing, &lemma);
                    replace_readings(&mut tokens[i], &[(&next_pos_tag, &lemma)]);
                    if is_last {
                        next_pos_tag.clear();
                        lemma.clear();
                    }
                } else if let Some((tag, entry_lemma)) = multiword_start_token(tokens, i) {
                    pos_tag = tag;
                    lemma = entry_lemma;
                    let closing = format!("</{pos_tag}>");
                    if has_pos_tag_and_lemma(&tokens[i], &closing, &lemma) {
                        remove_marker_readings(&mut tokens[i], &pos_tag);
                        add_plain_reading(&mut tokens[i], &pos_tag, &lemma);
                    } else {
                        replace_readings(&mut tokens[i], &[(&pos_tag, &lemma)]);
                        next_pos_tag = get_next_pos_tag(&pos_tag);
                    }
                }
            }
            i += 1;
        }
    }
}

fn get_next_pos_tag(postag: &str) -> String {
    if let Some(rest) = postag.strip_prefix("NC") {
        let mut out = String::from("AQ0");
        let chars: Vec<char> = rest.chars().collect();
        if chars.len() >= 2 {
            out.push(chars[0]);
            out.push(chars[1]);
        }
        out.push('0');
        return out;
    }
    if let Some(rest) = postag.strip_prefix("N ") {
        return format!("J {rest}");
    }
    postag.to_string()
}

fn is_low_priority_tag(tag: &str) -> bool {
    tag == "NPCN000"
}

fn multiword_start_token(tokens: &[AnalyzedTokenReadings], i: usize) -> Option<(String, String)> {
    let mut candidates: Vec<(&str, &str)> = Vec::new();
    for reading in tokens[i].readings() {
        if let Some(pos) = reading.pos_tags().first()
            && pos.starts_with('<')
            && pos.ends_with('>')
            && !pos.starts_with("</")
        {
            candidates.push((pos, reading.lemma().unwrap_or("")));
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let mut selected: Option<(String, String)> = None;
    let mut max_distance = 0usize;
    for (pos, lemma) in candidates {
        let closing = format!("</{}", &pos[1..]);
        let inner_len = pos.chars().count();
        let clean_tag: String = pos
            .chars()
            .skip(1)
            .take(inner_len.saturating_sub(2))
            .collect();
        let mut distance = 1usize;
        while i + distance < tokens.len() {
            if has_pos_tag_and_lemma(&tokens[i + distance], &closing, lemma) {
                if distance > max_distance
                    || (distance == max_distance && !is_low_priority_tag(&clean_tag))
                {
                    max_distance = distance;
                    selected = Some((clean_tag_full(pos), lemma.to_string()));
                }
                break;
            }
            distance += 1;
        }
    }
    selected
}

fn clean_tag_full(pos: &str) -> String {
    let chars: Vec<char> = pos.chars().collect();
    chars[1..chars.len() - 1].iter().collect()
}

fn has_pos_tag_and_lemma(atr: &AnalyzedTokenReadings, pos_tag: &str, lemma: &str) -> bool {
    atr.readings().iter().any(|r| {
        r.pos_tags().first().map(|p| p.as_str()) == Some(pos_tag)
            && r.lemma().map(|l| l == lemma).unwrap_or(false)
    })
}

fn make_reading(atr: &AnalyzedTokenReadings, pos_tag: &str, lemma: &str) -> AnalyzedToken {
    let t = atr.token();
    AnalyzedToken::new(t.token(), t.start(), t.end())
        .with_pos_tags(vec![pos_tag.to_string()])
        .with_lemma(lemma)
}

fn prepare_new_reading(atr: &mut AnalyzedTokenReadings, tag: &str, lemma: &str, is_last: bool) {
    let marker = if is_last {
        format!("</{tag}>")
    } else {
        format!("<{tag}>")
    };
    let reading = make_reading(atr, &marker, lemma);
    push_reading(atr, reading);
}

fn add_plain_reading(atr: &mut AnalyzedTokenReadings, tag: &str, lemma: &str) {
    let reading = make_reading(atr, tag, lemma);
    push_reading(atr, reading);
}

fn push_reading(atr: &mut AnalyzedTokenReadings, reading: AnalyzedToken) {
    let readings = atr.readings_mut();
    let last_is_null = readings
        .last()
        .map(|r| r.pos_tags().is_empty())
        .unwrap_or(false);
    if last_is_null {
        readings.pop();
    }
    let exists = readings.iter().any(|r| {
        r.token() == reading.token()
            && r.pos_tags() == reading.pos_tags()
            && r.lemma() == reading.lemma()
    });
    if !exists {
        readings.push(reading);
    }
}

fn replace_readings(atr: &mut AnalyzedTokenReadings, new_readings: &[(&str, &str)]) {
    let chunks = atr.chunks().to_vec();
    let ignored_by_speller = atr.is_ignored_by_speller();
    let was_immunized = atr.is_immunized();
    let readings: Vec<AnalyzedToken> = new_readings
        .iter()
        .map(|(pos, lemma)| make_reading(atr, pos, lemma))
        .collect();
    let mut updated = match new_readings.first() {
        Some((pos, lemma)) => {
            let primary = make_reading(atr, pos, lemma);
            AnalyzedTokenReadings::new(primary).with_readings(readings)
        }
        None => AnalyzedTokenReadings::new(atr.token().clone()),
    };
    updated.set_chunks(chunks);
    if ignored_by_speller {
        updated.ignore_spelling();
    }
    if was_immunized {
        updated.immunize();
    }
    *atr = updated;
}

fn remove_marker_readings(atr: &mut AnalyzedTokenReadings, pos_tag: &str) {
    let open = format!("<{pos_tag}>");
    let close = format!("</{pos_tag}>");
    atr.readings_mut().retain(|r| {
        !r.pos_tags()
            .first()
            .map(|p| p == &open || p == &close)
            .unwrap_or(false)
    });
}

pub struct MultiWordChunkerDisambiguator {
    chunker: MultiWordChunker,
}

pub struct ChunkerParams {
    pub allow_first_capitalized: bool,
    pub allow_all_uppercase: bool,
    pub allow_titlecase: bool,
    pub default_tag: Option<&'static str>,
    pub remove_previous_tags: bool,
    pub add_ignore_spelling: bool,
}

impl MultiWordChunkerDisambiguator {
    pub fn from_params(path: &Path, params: ChunkerParams) -> Option<Self> {
        let chunker = MultiWordChunker::from_file(
            path,
            params.allow_first_capitalized,
            params.allow_all_uppercase,
            params.allow_titlecase,
            params.default_tag,
            params.remove_previous_tags,
            params.add_ignore_spelling,
        )?;
        Some(Self { chunker })
    }

    pub fn from_resource_dir(resource_dir: &Path) -> Option<Self> {
        Self::from_params(
            &resource_dir.join("multiwords.txt"),
            ChunkerParams {
                allow_first_capitalized: true,
                allow_all_uppercase: true,
                allow_titlecase: false,
                default_tag: None,
                remove_previous_tags: true,
                add_ignore_spelling: true,
            },
        )
    }

    pub fn spelling_global(core_resource_dir: &Path) -> Option<Self> {
        Self::from_params(
            &core_resource_dir.join("spelling_global.txt"),
            ChunkerParams {
                allow_first_capitalized: true,
                allow_all_uppercase: true,
                allow_titlecase: false,
                default_tag: Some(TAG_FOR_NOT_ADDING_TAGS),
                remove_previous_tags: false,
                add_ignore_spelling: true,
            },
        )
    }
}

const EN_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: None,
    remove_previous_tags: true,
    add_ignore_spelling: true,
};
const EN_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some(TAG_FOR_NOT_ADDING_TAGS),
    remove_previous_tags: false,
    add_ignore_spelling: true,
};
const FR_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: None,
    remove_previous_tags: true,
    add_ignore_spelling: false,
};
const FR_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some(TAG_FOR_NOT_ADDING_TAGS),
    remove_previous_tags: false,
    add_ignore_spelling: true,
};
const ES_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: None,
    remove_previous_tags: true,
    add_ignore_spelling: false,
};
const CA_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: None,
    remove_previous_tags: true,
    add_ignore_spelling: false,
};
const CA_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some("NPCN000"),
    remove_previous_tags: false,
    add_ignore_spelling: false,
};
const ES_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some("NPCN000"),
    remove_previous_tags: false,
    add_ignore_spelling: false,
};
const PT_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: true,
    default_tag: None,
    remove_previous_tags: true,
    add_ignore_spelling: true,
};
const PT_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: true,
    allow_titlecase: true,
    default_tag: Some("NPCN000"),
    remove_previous_tags: false,
    add_ignore_spelling: true,
};
const NL_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: true,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some(TAG_FOR_NOT_ADDING_TAGS),
    remove_previous_tags: false,
    add_ignore_spelling: true,
};
const NL_SPELLING_GLOBAL: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: true,
    allow_titlecase: false,
    default_tag: Some(TAG_FOR_NOT_ADDING_TAGS),
    remove_previous_tags: false,
    add_ignore_spelling: true,
};
const BARE_MULTIWORDS: ChunkerParams = ChunkerParams {
    allow_first_capitalized: false,
    allow_all_uppercase: false,
    allow_titlecase: false,
    default_tag: None,
    remove_previous_tags: false,
    add_ignore_spelling: false,
};

pub fn hybrid_disambiguator(
    code: &str,
    language_resource_dir: &Path,
    core_resource_dir: &Path,
    xml: XmlDisambiguator,
) -> Arc<dyn Disambiguator> {
    let (multiwords, global, xml_first) = match code {
        "en" => (Some(EN_MULTIWORDS), Some(EN_SPELLING_GLOBAL), false),
        "fr" => (Some(FR_MULTIWORDS), Some(FR_SPELLING_GLOBAL), false),
        "es" => (Some(ES_MULTIWORDS), Some(ES_SPELLING_GLOBAL), false),
        "ca" => (Some(CA_MULTIWORDS), Some(CA_SPELLING_GLOBAL), false),
        "pt" => (Some(PT_MULTIWORDS), Some(PT_SPELLING_GLOBAL), false),
        "nl" => (Some(NL_MULTIWORDS), Some(NL_SPELLING_GLOBAL), false),
        "ru" => (Some(BARE_MULTIWORDS), None, false),
        "gl" => (Some(BARE_MULTIWORDS), None, false),
        "pl" => (Some(BARE_MULTIWORDS), None, true),
        "sv" => (Some(BARE_MULTIWORDS), None, true),
        _ => (None, None, false),
    };
    let mut stages: Vec<Arc<dyn Disambiguator>> = Vec::new();
    if let Some(params) = global
        && let Some(chunker) = MultiWordChunkerDisambiguator::from_params(
            &core_resource_dir.join("spelling_global.txt"),
            params,
        )
    {
        stages.push(Arc::new(chunker));
    }
    let xml_stage: Arc<dyn Disambiguator> = Arc::new(xml);
    if xml_first {
        stages.push(xml_stage.clone());
    }
    if let Some(params) = multiwords
        && let Some(chunker) = MultiWordChunkerDisambiguator::from_params(
            &language_resource_dir.join("multiwords.txt"),
            params,
        )
    {
        stages.push(Arc::new(chunker));
    }
    if stages.is_empty() {
        return xml_stage;
    }
    if !xml_first {
        stages.push(xml_stage);
    }
    Arc::new(ChainedDisambiguator::new(stages))
}

impl Disambiguator for MultiWordChunkerDisambiguator {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence) {
        let tokens = sentence.tokens_mut();
        self.chunker.apply(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atr(text: &str, start: usize) -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, start, start + text.len()))
    }

    fn atr_with_space(text: &str, start: usize) -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, start, start + text.len()))
    }

    fn sentence_tokens(text: &str) -> Vec<AnalyzedTokenReadings> {
        let mut out = Vec::new();
        let mut pos = 0;
        let mut expect_space = false;
        for word in text.split(' ') {
            if expect_space {
                out.push(atr_with_space(" ", pos));
                pos += 1;
                expect_space = false;
            }
            if word.is_empty() {
                continue;
            }
            out.push(atr(word, pos));
            pos += word.len();
            expect_space = true;
        }
        out
    }

    #[test]
    fn multiword_phrase_replaces_readings() {
        let content = "status quo\tNN:UN\nquid pro quo\tNN\n";
        let chunker = MultiWordChunker::from_content(content, true, true, false, None, true, false);
        let mut tokens = sentence_tokens("The status quo is odd");
        chunker.apply(&mut tokens);
        let status = tokens
            .iter()
            .find(|t| t.token().token() == "status")
            .unwrap();
        let quo = tokens.iter().find(|t| t.token().token() == "quo").unwrap();
        assert_eq!(
            status
                .readings()
                .iter()
                .map(|r| (r.token(), r.pos_tags().concat(), r.lemma().unwrap_or("")))
                .map(|(a, b, c)| (a.to_string(), b, c.to_string()))
                .collect::<Vec<_>>(),
            vec![(
                "status".to_string(),
                "NN:UN".to_string(),
                "status quo".to_string()
            )]
        );
        assert_eq!(
            quo.readings()
                .iter()
                .map(|r| (r.token(), r.pos_tags().concat(), r.lemma().unwrap_or("")))
                .map(|(a, b, c)| (a.to_string(), b, c.to_string()))
                .collect::<Vec<_>>(),
            vec![(
                "quo".to_string(),
                "NN:UN".to_string(),
                "status quo".to_string()
            )]
        );
    }

    #[test]
    fn single_token_entry_keeps_existing_readings() {
        let content = "coffee\tNN:UN\n";
        let chunker = MultiWordChunker::from_content(content, true, true, false, None, true, false);
        let mut tokens = sentence_tokens("coffee");
        tokens[0].readings_mut().push(
            AnalyzedToken::new("coffee", 0, 6)
                .with_pos_tags(vec!["NN".to_string()])
                .with_lemma("coffee"),
        );
        chunker.apply(&mut tokens);
        assert!(tokens[0].readings().len() >= 2);
        assert!(tokens[0].has_pos_tag("NN:UN"));
    }

    #[test]
    fn no_space_entry_spans_split_tokens() {
        let content = "don't\tVB\n";
        let chunker = MultiWordChunker::from_content(content, true, true, false, None, true, false);
        let mut tokens = vec![atr("do", 0), atr("n't", 2), atr("!", 5)];
        chunker.apply(&mut tokens);
        assert_eq!(
            tokens[0]
                .readings()
                .iter()
                .map(|r| (r.pos_tags().concat(), r.lemma().unwrap_or("").to_string()))
                .collect::<Vec<_>>(),
            vec![("VB".to_string(), "don't".to_string())]
        );
        assert_eq!(
            tokens[1]
                .readings()
                .iter()
                .map(|r| (r.pos_tags().concat(), r.lemma().unwrap_or("").to_string()))
                .collect::<Vec<_>>(),
            vec![("VB".to_string(), "don't".to_string())]
        );
    }

    #[test]
    fn casing_variants_match() {
        let content = "big sur\tNNP\n";
        let chunker = MultiWordChunker::from_content(content, true, true, false, None, true, false);
        let mut tokens = sentence_tokens("We visited Big sur today");
        chunker.apply(&mut tokens);
        let big = tokens.iter().find(|t| t.token().token() == "Big").unwrap();
        assert_eq!(
            big.readings()
                .iter()
                .map(|r| (r.pos_tags().concat(), r.lemma().unwrap_or("").to_string()))
                .collect::<Vec<_>>(),
            vec![("NNP".to_string(), "big sur".to_string())]
        );
        let mut title = sentence_tokens("We visited Big Sur today");
        chunker.apply(&mut title);
        let big = title.iter().find(|t| t.token().token() == "Big").unwrap();
        assert!(big.readings().iter().all(|r| r.pos_tags().is_empty()));
    }

    #[test]
    fn spelling_global_marks_phrase_without_adding_readings() {
        let content = "Emmanuel Macron\nBrigitte Macron\n";
        let chunker = MultiWordChunker::from_content(
            content,
            true,
            true,
            false,
            Some(TAG_FOR_NOT_ADDING_TAGS),
            false,
            true,
        );
        let mut tokens = sentence_tokens("We met Emmanuel Macron yesterday");
        chunker.apply(&mut tokens);
        let emmanuel = tokens
            .iter()
            .find(|t| t.token().token() == "Emmanuel")
            .unwrap();
        let macron = tokens
            .iter()
            .find(|t| t.token().token() == "Macron")
            .unwrap();
        let yesterday = tokens
            .iter()
            .find(|t| t.token().token() == "yesterday")
            .unwrap();
        assert!(emmanuel.is_ignored_by_speller());
        assert!(macron.is_ignored_by_speller());
        assert!(!yesterday.is_ignored_by_speller());
        assert!(emmanuel.readings().iter().all(|r| r.pos_tags().is_empty()));
        assert!(macron.readings().iter().all(|r| r.pos_tags().is_empty()));
    }

    #[test]
    fn spelling_global_marks_single_token() {
        let content = "log4j\nb2match\n";
        let chunker = MultiWordChunker::from_content(
            content,
            true,
            true,
            false,
            Some(TAG_FOR_NOT_ADDING_TAGS),
            false,
            true,
        );
        let mut tokens = sentence_tokens("log4j is used");
        chunker.apply(&mut tokens);
        assert!(tokens[0].is_ignored_by_speller());
        assert!(!tokens[2].is_ignored_by_speller());
        assert!(tokens[0].readings().iter().all(|r| r.pos_tags().is_empty()));
    }

    fn fake_tagged(text: &str) -> Vec<AnalyzedTokenReadings> {
        let mut tokens = sentence_tokens(text);
        for token in &mut tokens {
            if token.is_whitespace() {
                continue;
            }
            let (word, start, end) = (
                token.token().token().to_string(),
                token.token().start(),
                token.token().end(),
            );
            token.readings_mut().push(
                AnalyzedToken::new(&word, start, end)
                    .with_pos_tags(vec!["FakePosTag".to_string()])
                    .with_lemma(&word),
            );
        }
        tokens
    }

    fn reading_pairs(atr: &AnalyzedTokenReadings) -> Vec<(String, String)> {
        atr.readings()
            .iter()
            .map(|r| {
                (
                    r.pos_tags().concat(),
                    r.lemma().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    fn with_sent_start(tokens: Vec<AnalyzedTokenReadings>) -> Vec<AnalyzedTokenReadings> {
        let mut out = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        out.extend(tokens);
        out
    }

    #[test]
    fn wrapped_chunk_tags_keep_existing_readings() {
        let content = "ah this\tfake\nah for shame\tadv\nnot for this either\tfake\n";
        let chunker = MultiWordChunker::from_content(content, true, true, true, None, false, false);
        let mut tokens = with_sent_start(fake_tagged("ah for shame"));
        chunker.apply(&mut tokens);
        let pairs: Vec<(String, String)> = tokens
            .iter()
            .filter(|t| !t.is_whitespace())
            .flat_map(reading_pairs)
            .collect();
        assert!(
            pairs.contains(&("<adv>".to_string(), "ah for shame".to_string())),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&("</adv>".to_string(), "ah for shame".to_string())),
            "{pairs:?}"
        );
        assert!(
            !pairs
                .iter()
                .any(|(tag, _)| tag.contains("adv") && tag != "<adv>" && tag != "</adv>"),
            "{pairs:?}"
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|t| !t.is_whitespace())
                .filter(|t| reading_pairs(t).iter().any(|(tag, _)| tag == "FakePosTag"))
                .count(),
            3,
            "{pairs:?}"
        );
    }

    #[test]
    fn remove_previous_tags_stamp_every_phrase_token() {
        let content = "ah this\tfake\nah for shame\tadv\nnot for this either\tfake\n";
        let chunker = MultiWordChunker::from_content(content, true, true, true, None, true, false);
        let mut tokens = with_sent_start(fake_tagged("ah for shame"));
        chunker.apply(&mut tokens);
        let words: Vec<&AnalyzedTokenReadings> = tokens
            .iter()
            .skip(1)
            .filter(|t| !t.is_whitespace())
            .collect();
        assert_eq!(words.len(), 3);
        for word in &words {
            assert_eq!(
                reading_pairs(word),
                vec![("adv".to_string(), "ah for shame".to_string())],
                "{}: {:?}",
                word.token().token(),
                reading_pairs(word)
            );
        }
    }

    #[test]
    fn nc_wrapped_entries_become_noun_then_adjective_tags() {
        let content = "rhythm and blues\tNCMS000\n";
        let chunker = MultiWordChunker::from_content(content, true, true, true, None, true, false);
        let mut tokens = with_sent_start(sentence_tokens("rhythm and blues"));
        chunker.apply(&mut tokens);
        let words: Vec<&AnalyzedTokenReadings> = tokens
            .iter()
            .skip(1)
            .filter(|t| !t.is_whitespace())
            .collect();
        assert_eq!(
            reading_pairs(words[0]),
            vec![("NCMS000".to_string(), "rhythm and blues".to_string())]
        );
        for word in &words[1..] {
            assert_eq!(
                reading_pairs(word),
                vec![("AQ0MS0".to_string(), "rhythm and blues".to_string())],
                "{}: {:?}",
                word.token().token(),
                reading_pairs(word)
            );
        }
    }

    #[test]
    fn low_priority_marker_loses_against_real_multiword_at_same_distance() {
        let chunker = MultiWordChunker::from_content("", true, true, true, None, true, false);
        let mut tokens = sentence_tokens("big sur");
        tokens[0].readings_mut().extend([
            AnalyzedToken::new("big", 0, 3)
                .with_pos_tags(vec!["<NP00000>".to_string()])
                .with_lemma("big sur"),
            AnalyzedToken::new("big", 0, 3)
                .with_pos_tags(vec!["<NPCN000>".to_string()])
                .with_lemma("big sur global"),
        ]);
        tokens[2].readings_mut().extend([
            AnalyzedToken::new("sur", 4, 7)
                .with_pos_tags(vec!["</NP00000>".to_string()])
                .with_lemma("big sur"),
            AnalyzedToken::new("sur", 4, 7)
                .with_pos_tags(vec!["</NPCN000>".to_string()])
                .with_lemma("big sur global"),
        ]);
        chunker.apply(&mut tokens);
        assert_eq!(
            reading_pairs(&tokens[0]),
            vec![("NP00000".to_string(), "big sur".to_string())],
            "{:?}",
            reading_pairs(&tokens[0])
        );
        assert_eq!(
            reading_pairs(&tokens[2]),
            vec![("NP00000".to_string(), "big sur".to_string())],
            "{:?}",
            reading_pairs(&tokens[2])
        );
    }

    #[test]
    fn lettercase_variants_follow_java() {
        let content = "rhythm and blues\tNN\nVênus de Milo\tNN\niPad\tNN\n";
        let chunker = MultiWordChunker::from_content(content, true, true, true, None, true, false);
        for text in ["Rhythm and blues", "Rhythm And Blues", "RHYTHM AND BLUES"] {
            let mut tokens = with_sent_start(sentence_tokens(text));
            chunker.apply(&mut tokens);
            assert_eq!(
                reading_pairs(&tokens[1]),
                vec![("NN".to_string(), "rhythm and blues".to_string())],
                "{text}: {:?}",
                reading_pairs(&tokens[1])
            );
        }
        let mut tokens = with_sent_start(sentence_tokens("VÊNUS DE MILO"));
        chunker.apply(&mut tokens);
        assert_eq!(
            reading_pairs(&tokens[1]),
            vec![("NN".to_string(), "Vênus de Milo".to_string())],
            "{:?}",
            reading_pairs(&tokens[1])
        );
        for text in ["Vênus De Milo", "vênus de milo", "IPAD"] {
            let mut tokens = with_sent_start(sentence_tokens(text));
            chunker.apply(&mut tokens);
            assert!(
                tokens[1].readings().iter().all(|r| r.pos_tags().is_empty()),
                "{text} should not match: {:?}",
                reading_pairs(&tokens[1])
            );
        }
    }

    #[test]
    fn german_suffix_expansion_adds_e_s_n_forms() {
        let chunker =
            MultiWordChunker::from_content("auto/ES\n", true, true, true, Some("NN"), false, false);
        for text in ["auto", "autoe", "autos"] {
            let mut tokens = sentence_tokens(text);
            chunker.apply(&mut tokens);
            assert_eq!(
                reading_pairs(&tokens[0]),
                vec![("NN".to_string(), text.to_string())],
                "{text}: {:?}",
                reading_pairs(&tokens[0])
            );
        }
        let mut tokens = sentence_tokens("auton");
        chunker.apply(&mut tokens);
        assert!(
            tokens[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "{:?}",
            reading_pairs(&tokens[0])
        );
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let with_default = MultiWordChunker::from_content(
            "status quo\tNN\n",
            true,
            true,
            true,
            Some("NN"),
            false,
            false,
        );
        let mut tokens = sentence_tokens("status quo");
        with_default.apply(&mut tokens);
        assert!(
            tokens[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "{:?}",
            reading_pairs(&tokens[0])
        );

        let without_default = MultiWordChunker::from_content(
            "one two three\tNN\textra\ncoffee\t\n",
            true,
            true,
            true,
            None,
            false,
            false,
        );
        let mut tokens = sentence_tokens("one two three");
        without_default.apply(&mut tokens);
        assert!(
            tokens
                .iter()
                .filter(|t| !t.is_whitespace())
                .all(|t| t.readings().iter().all(|r| r.pos_tags().is_empty())),
            "{:?}",
            tokens
                .iter()
                .filter(|t| !t.is_whitespace())
                .flat_map(reading_pairs)
                .collect::<Vec<_>>()
        );
        let mut tokens = sentence_tokens("coffee");
        without_default.apply(&mut tokens);
        assert!(
            tokens[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "{:?}",
            reading_pairs(&tokens[0])
        );
    }

    #[test]
    fn ignore_spelling_marking_keeps_existing_readings() {
        let content = "coffee bar\n";
        let chunker = MultiWordChunker::from_content(
            content,
            true,
            true,
            false,
            Some(TAG_FOR_NOT_ADDING_TAGS),
            false,
            true,
        );
        let mut tokens = sentence_tokens("coffee bar");
        let start = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        tokens.insert(0, start);
        tokens[1]
            .readings_mut()
            .push(AnalyzedToken::new("coffee", 0, 6).with_pos_tags(vec!["NN".to_string()]));
        chunker.apply(&mut tokens);
        assert!(tokens[1].is_ignored_by_speller());
        assert!(tokens[3].is_ignored_by_speller());
        assert!(tokens[1].has_pos_tag("NN"));
    }
}

#[cfg(test)]
mod hybrid_tests {
    use super::*;

    fn fr_resource_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr")
    }

    fn core_resource_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        )
    }

    #[test]
    fn french_multiwords_chunker_tags_phrases_from_resource_file() {
        let disambiguator = MultiWordChunkerDisambiguator::from_params(
            &fr_resource_dir().join("multiwords.txt"),
            FR_MULTIWORDS,
        );
        let Some(disambiguator) = disambiguator else {
            panic!("fr multiwords.txt should load");
        };
        let mut sentence = AnalyzedSentence::new("home page", 0, 9);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("home", 3, 7)
                    .with_pos_tags(vec!["J".to_string()])
                    .with_lemma("home".to_string()),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 7, 8).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("page", 8, 12)
                    .with_pos_tags(vec!["N".to_string()])
                    .with_lemma("page".to_string()),
            ),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let home = &sentence.tokens()[1];
        assert!(
            home.readings()
                .iter()
                .any(|r| r.pos_tags().contains(&"N f s".to_string())),
            "home page should be chunked as N f s, got {:?}",
            home.readings()
        );
    }

    #[test]
    fn hybrid_disambiguator_builds_french_chain() {
        let xml = XmlDisambiguator::from_xml("<rules></rules>").unwrap();
        let chain = hybrid_disambiguator("fr", &fr_resource_dir(), &core_resource_dir(), xml);
        let mut sentence = AnalyzedSentence::new("x", 0, 1);
        chain.disambiguate(&mut sentence);
    }

    fn lang_resource_dir(code: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/resource/{code}"
        ))
    }

    fn phrase_sentence(first: &str, second: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(
            &format!("{first} {second}"),
            0,
            first.len() + 1 + second.len(),
        );
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(first, 0, first.len())
                    .with_pos_tags(vec!["AQ0CS0".to_string()])
                    .with_lemma(first),
            ),
            AnalyzedTokenReadings::new(AnalyzedToken::new(" ", first.len(), first.len() + 1)),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(second, first.len() + 1, first.len() + 1 + second.len())
                    .with_pos_tags(vec!["NCMS000".to_string()])
                    .with_lemma(second),
            ),
        ]);
        sentence
    }

    #[test]
    fn hybrid_disambiguator_chunks_spanish_multiwords_from_the_resource_file() {
        let xml = XmlDisambiguator::from_xml("<rules></rules>").unwrap();
        let chain = hybrid_disambiguator("es", &lang_resource_dir("es"), &core_resource_dir(), xml);
        let mut sentence = phrase_sentence("opera", "prima");
        chain.disambiguate(&mut sentence);
        let opera = &sentence.tokens()[1];
        assert!(
            opera
                .readings()
                .iter()
                .any(|r| r.pos_tags().contains(&"NCFSO00".to_string())),
            "opera prima should be chunked as NCFSO00, got {:?}",
            reading_tuples(opera)
        );
    }

    #[test]
    fn hybrid_disambiguator_builds_nl_ru_and_pl_chains() {
        for code in ["nl", "ru", "pl"] {
            let xml = XmlDisambiguator::from_xml("<rules></rules>").unwrap();
            let chain =
                hybrid_disambiguator(code, &lang_resource_dir(code), &core_resource_dir(), xml);
            let mut sentence = phrase_sentence("opera", "prima");
            chain.disambiguate(&mut sentence);
        }
    }

    #[test]
    fn hybrid_disambiguator_without_resources_returns_the_xml_stage() {
        let xml = XmlDisambiguator::from_xml("<rules></rules>").unwrap();
        let missing = std::path::Path::new("/nonexistent-resource-dir");
        let chain = hybrid_disambiguator("xx", missing, missing, xml);
        let mut sentence = phrase_sentence("opera", "prima");
        chain.disambiguate(&mut sentence);
        let opera = &sentence.tokens()[1];
        assert_eq!(
            reading_tuples(opera),
            vec![("AQ0CS0".to_string(), "opera".to_string())],
            "{:?}",
            reading_tuples(opera)
        );
    }

    fn en_resource_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        )
    }

    fn reading_tuples(atr: &AnalyzedTokenReadings) -> Vec<(String, String)> {
        atr.readings()
            .iter()
            .map(|r| {
                (
                    r.pos_tags().concat(),
                    r.lemma().unwrap_or_default().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn hybrid_disambiguator_builds_english_chain() {
        let xml = XmlDisambiguator::from_xml("<rules></rules>").unwrap();
        let chain = hybrid_disambiguator("en", &en_resource_dir(), &core_resource_dir(), xml);
        let mut sentence = AnalyzedSentence::new("20th Century Fox released it", 0, 28);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("20th", 0, 4)
                    .with_pos_tags(vec!["CD".to_string()])
                    .with_lemma("20th".to_string()),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 4, 5).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("Century", 5, 12)
                    .with_pos_tags(vec!["NNP".to_string()])
                    .with_lemma("Century".to_string()),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 12, 13).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("Fox", 13, 16)
                    .with_pos_tags(vec!["NNP".to_string()])
                    .with_lemma("Fox".to_string()),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 16, 17).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("released", 17, 25)
                    .with_pos_tags(vec!["VBD".to_string()])
                    .with_lemma("release".to_string()),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 25, 26).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("it", 26, 28)
                    .with_pos_tags(vec!["PRP".to_string()])
                    .with_lemma("it".to_string()),
            ),
        ]);
        chain.disambiguate(&mut sentence);
        let tokens = sentence.tokens();
        for idx in [1usize, 3, 5] {
            assert_eq!(
                reading_tuples(&tokens[idx]),
                vec![("NNP".to_string(), "20th Century Fox".to_string())],
                "token {idx} must be chunked to a single NNP reading with the multiword lemma"
            );
            assert!(
                tokens[idx].is_ignored_by_speller(),
                "token {idx} must carry the spelling_global ignore flag across the reading replacement"
            );
        }
        assert!(!tokens[7].is_ignored_by_speller());
        assert_eq!(
            reading_tuples(&tokens[7]),
            vec![("VBD".to_string(), "release".to_string())]
        );
        assert!(!tokens[9].is_ignored_by_speller());
    }
}

#[cfg(test)]
mod sep_tests {
    use super::*;

    #[test]
    fn semicolon_separator_header_is_honored() {
        let content = "#separatorRegExp=[\\t;]\nhome page;N f s\n";
        let chunker = MultiWordChunker::from_content(content, true, true, false, None, true, false);
        let mut tokens = vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 3).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("home", 3, 7).with_pos_tags(vec!["J".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new(" ", 7, 8).with_pos_tags(vec![" ".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("page", 8, 12).with_pos_tags(vec!["N".to_string()]),
            ),
        ];
        chunker.apply(&mut tokens);
        assert!(
            tokens[1]
                .readings()
                .iter()
                .any(|r| r.pos_tags().contains(&"N f s".to_string())),
            "got {:?}",
            tokens[1].readings()
        );
    }
}
