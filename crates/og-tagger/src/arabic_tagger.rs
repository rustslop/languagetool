use std::path::Path;
use std::sync::LazyLock;

use og_core::{AnalyzedToken, AnalyzedTokenReadings};

use crate::generic_tagger::GenericTagger;

const TASHKEEL_CHARS: &[char] = &[
    '\u{64b}', '\u{64c}', '\u{64d}', '\u{64e}', '\u{64f}', '\u{650}', '\u{651}', '\u{652}',
    '\u{653}', '\u{654}', '\u{655}', '\u{656}', '\u{640}',
];

static PRONOUN_SUFFIX_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        "(\u{643}|\u{647}\u{627}|\u{647}\u{645}\u{627}|\u{647}\u{645}|\u{647}\u{646}|\u{643}\u{645}\u{627}|\u{643}\u{645}|\u{643}\u{646}|\u{646}\u{627}|\u{64a})$",
    )
    .expect("valid pronoun suffix pattern")
});

fn noun_flag_pos(flag_type: &str) -> usize {
    match flag_type {
        "CATEGORY" => 1,
        "GENDER" => 4,
        "NUMBER" => 5,
        "CASE" => 6,
        "INFLECT_MARK" => 7,
        "CONJ" => 9,
        "JAR" => 10,
        "PRONOUN" => 11,
        _ => 0,
    }
}

fn verb_flag_pos(flag_type: &str) -> usize {
    match flag_type {
        "CATEGORY" => 1,
        "TRANS" => 2,
        "GENDER" => 4,
        "NUMBER" => 5,
        "PERSON" => 6,
        "INFLECT_MARK" => 7,
        "TENSE" => 8,
        "VOICE" => 9,
        "CASE" => 10,
        "CONJ" => 12,
        "ISTIQBAL" => 13,
        "PRONOUN" => 14,
        _ => 0,
    }
}

fn particle_flag_pos(flag_type: &str) -> usize {
    match flag_type {
        "CATEGORY" => 1,
        "OPTION" => 2,
        "GENDER" => 4,
        "NUMBER" => 5,
        "CASE" => 6,
        "CONJ" => 8,
        "JAR" => 9,
        "PRONOUN" => 10,
        _ => 0,
    }
}

fn is_noun(postag: &str) -> bool {
    postag.starts_with('N')
}

fn is_verb(postag: &str) -> bool {
    postag.starts_with('V')
}

fn is_stopword_tag(postag: &str) -> bool {
    postag.starts_with('P')
}

fn flag_pos(postag: &str, flag_type: &str) -> usize {
    if is_noun(postag) {
        noun_flag_pos(flag_type)
    } else if is_verb(postag) {
        verb_flag_pos(flag_type)
    } else if is_stopword_tag(postag) {
        particle_flag_pos(flag_type)
    } else {
        0
    }
}

pub fn get_flag(postag: &str, flag_type: &str) -> char {
    let chars: Vec<char> = postag.chars().collect();
    let pos = flag_pos(postag, flag_type);
    if pos < chars.len() { chars[pos] } else { '-' }
}

pub fn set_flag(postag: &str, flag_type: &str, flag: char) -> String {
    let mut chars: Vec<char> = postag.chars().collect();
    let pos = flag_pos(postag, flag_type);
    if pos < chars.len() {
        chars[pos] = flag;
    }
    chars.into_iter().collect()
}

fn is_majrour(postag: &str) -> bool {
    let flag = get_flag(postag, "CASE");
    flag == 'I' || flag == '-'
}

pub(crate) fn is_attached(postag: &str) -> bool {
    (is_noun(postag) || is_verb(postag)) && get_flag(postag, "PRONOUN") == 'H'
}

pub(crate) fn is_definite(postag: &str) -> bool {
    is_noun(postag) && get_flag(postag, "PRONOUN") == 'L'
}

pub(crate) fn has_jar(postag: &str) -> bool {
    is_noun(postag) && get_flag(postag, "JAR") != '-'
}

pub(crate) fn has_pronoun(postag: &str) -> bool {
    get_flag(postag, "PRONOUN") == 'H'
}

pub(crate) fn has_conjunction(postag: &str) -> bool {
    let flag = get_flag(postag, "CONJ");
    ((is_noun(postag) || is_verb(postag)) && flag != '-')
        || (is_stopword_tag(postag) && flag != 'W')
}

pub(crate) fn get_definite_prefix(postag: &str) -> String {
    if postag.is_empty() {
        return String::new();
    }
    if is_noun(postag) && get_flag(postag, "PRONOUN") == 'L' {
        if has_jar(postag) && get_jar_prefix(postag) == "ل" {
            return "ل".to_string();
        }
        return "ال".to_string();
    }
    String::new()
}

pub(crate) fn get_jar_prefix(postag: &str) -> String {
    if postag.is_empty() {
        return String::new();
    }
    if is_noun(postag) {
        return match get_flag(postag, "JAR") {
            'L' => "ل".to_string(),
            'K' => "ك".to_string(),
            'B' => "ب".to_string(),
            _ => String::new(),
        };
    }
    String::new()
}

pub(crate) fn get_conjunction_prefix(postag: &str) -> String {
    match get_flag(postag, "CONJ") {
        'F' => "ف".to_string(),
        'W' => "و".to_string(),
        _ => String::new(),
    }
}

pub(crate) fn get_pronoun_suffix(postag: &str) -> String {
    if postag.is_empty() {
        return String::new();
    }
    match get_flag(postag, "PRONOUN") {
        'b' => "ني",
        'c' => "نا",
        'd' => "ك",
        'e' => "كما",
        'f' => "كم",
        'g' => "كن",
        'H' => "ه",
        'i' => "ها",
        'j' => "هما",
        'k' => "هم",
        'n' => "هن",
        _ => "",
    }
    .to_string()
}

pub(crate) fn unify_pronoun_tag(postag: String) -> String {
    if is_attached(&postag) {
        set_flag(&postag, "PRONOUN", 'H')
    } else {
        postag
    }
}

pub(crate) fn set_conjunction(postag: &str, flag: &str) -> String {
    let myflag = match flag {
        "و" | "W" | "ف" | "F" => 'W',
        "-" | "" => '-',
        _ => return postag.to_string(),
    };
    if is_noun(postag) || is_verb(postag) {
        set_flag(postag, "CONJ", myflag)
    } else {
        postag.to_string()
    }
}

pub(crate) fn set_procletic_flags(postag: &str) -> String {
    if postag.is_empty() {
        return String::new();
    }
    if is_verb(postag) {
        let tag = set_flag(postag, "CONJ", '-');
        set_flag(&tag, "ISTIQBAL", '-')
    } else if is_noun(postag) {
        let mut tag = set_flag(postag, "CONJ", '-');
        tag = set_flag(&tag, "JAR", '-');
        if is_definite(&tag) {
            tag = set_flag(&tag, "PRONOUN", '-');
        }
        tag
    } else if is_stopword_tag(postag) {
        let tag = set_flag(postag, "CONJ", '-');
        set_flag(&tag, "JAR", '-')
    } else {
        postag.to_string()
    }
}

pub(crate) fn merge_pos_tag(source: &str, target: &str) -> String {
    if source.is_empty() {
        return target.to_string();
    }
    if target.is_empty() {
        return source.to_string();
    }
    if is_noun(source) && is_noun(target) {
        if source.chars().count() != target.chars().count() {
            return source.to_string();
        }
        return set_flag(source, "CATEGORY", get_flag(target, "CATEGORY"));
    }
    if is_verb(source) && is_verb(target) {
        if source.chars().count() != target.chars().count() {
            return source.to_string();
        }
        let tag = set_flag(source, "CATEGORY", get_flag(target, "CATEGORY"));
        return set_flag(&tag, "TRANS", get_flag(target, "TRANS"));
    }
    if is_stopword_tag(source) && is_stopword_tag(target) {
        if source.chars().count() != target.chars().count() {
            return source.to_string();
        }
        let tag = set_flag(source, "CATEGORY", get_flag(target, "CATEGORY"));
        return set_flag(&tag, "OPTION", get_flag(target, "OPTION"));
    }
    if is_stopword_tag(source) && (is_verb(target) || is_noun(target))
        || (is_verb(source) || is_noun(source)) && is_stopword_tag(target)
    {
        if has_pronoun(source) {
            return set_flag(target, "PRONOUN", get_flag(source, "PRONOUN"));
        }
        return target.to_string();
    }
    if is_verb(source) && is_noun(target) || is_noun(source) && is_verb(target) {
        let mut tag = target.to_string();
        if has_pronoun(source) {
            tag = set_flag(&tag, "PRONOUN", get_flag(source, "PRONOUN"));
        }
        return set_flag(&tag, "CONJ", get_flag(source, "CONJ"));
    }
    target.to_string()
}

fn is_future_tense(postag: &str) -> bool {
    is_verb(postag) && get_flag(postag, "TENSE") == 'f'
}

pub(crate) fn is_unattached_noun(postag: &str) -> bool {
    is_noun(postag) && get_flag(postag, "PRONOUN") != 'H' && !postag.ends_with('X')
}

fn modify_pos_tag(postag: &str, tags: &[String]) -> Option<String> {
    let mut current = postag.to_string();
    for tg in tags {
        current = add_tag(&current, tg)?;
    }
    Some(current)
}

fn add_tag(postag: &str, flag_string: &str) -> Option<String> {
    let mut parts = flag_string.split(';');
    let first = parts.next().unwrap_or("");
    let second = parts.next().unwrap_or("");
    let (flag_type, flag) = if second.is_empty() {
        ("", first)
    } else {
        (first, second)
    };

    let mut result = match flag {
        "W" => set_flag(postag, "CONJ", 'W'),
        "K" => {
            if is_noun(postag) && is_majrour(postag) {
                set_flag(postag, "JAR", 'K')
            } else {
                return None;
            }
        }
        "B" => {
            if is_noun(postag) && is_majrour(postag) {
                set_flag(postag, "JAR", 'B')
            } else {
                return None;
            }
        }
        "L" => {
            if is_noun(postag) {
                if is_majrour(postag) {
                    set_flag(postag, "JAR", 'L')
                } else {
                    return None;
                }
            } else {
                set_flag(postag, "ISTIQBAL", 'L')
            }
        }
        "D" => {
            if is_unattached_noun(postag) {
                set_flag(postag, "PRONOUN", 'L')
            } else {
                return None;
            }
        }
        "S" => {
            if is_future_tense(postag) {
                set_flag(postag, "ISTIQBAL", 'S')
            } else {
                return None;
            }
        }
        _ => postag.to_string(),
    };

    if flag_type == "PRONOUN" && !flag.is_empty() && flag != "D" {
        result = set_flag(&result, flag_type, flag.chars().next().unwrap_or('-'));
    }
    Some(result)
}

pub struct ArabicTagger {
    inner: GenericTagger,
    new_style_pronoun_tag: bool,
}

impl ArabicTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            inner: GenericTagger::from_resource_dir(resource_dir, "arabic"),
            new_style_pronoun_tag: false,
        }
    }

    pub fn with_new_style_pronoun_tag(mut self) -> Self {
        self.new_style_pronoun_tag = true;
        self
    }

    pub fn has_dictionary(&self) -> bool {
        self.inner.has_dictionary()
    }

    pub fn tag_word(&self, word: &str) -> AnalyzedTokenReadings {
        let readings = self.tag_readings(word, 0, word.len());
        if readings.is_empty() {
            return AnalyzedTokenReadings::new(AnalyzedToken::new(word, 0, word.len()));
        }
        let all_tags: Vec<String> = readings
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect();
        let lemma = readings
            .first()
            .and_then(|r| r.lemma().map(str::to_string))
            .unwrap_or_else(|| word.to_string());
        let primary = AnalyzedToken::new(word, 0, word.len())
            .with_pos_tags(all_tags)
            .with_lemma(lemma);
        AnalyzedTokenReadings::new(primary).with_readings(readings)
    }

    fn tag_readings(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let striped = Self::remove_tashkeel(word);
        let mut readings: Vec<AnalyzedToken> = self
            .inner
            .lookup(&striped)
            .into_iter()
            .map(|(lemma, tag)| {
                AnalyzedToken::new(word, start, end)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![tag.to_string()])
            })
            .collect();

        let is_stop = readings
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with('P')));
        if !is_stop {
            readings.extend(self.additional_tags(word, &striped, start, end));
        }
        readings
    }

    pub fn get_proclitic(&self, token: &AnalyzedToken) -> String {
        let postag = token.pos_tags().first().map(String::as_str).unwrap_or("");
        let word = token.token();
        if postag.is_empty() {
            return String::new();
        }
        let chars: Vec<char> = word.chars().collect();
        let prefix_len_chars = if is_verb(postag) {
            let mut len = 0;
            if get_flag(postag, "CONJ") == 'W' {
                len += 1;
            }
            if get_flag(postag, "ISTIQBAL") == 'S' {
                len += 1;
            }
            len
        } else if is_noun(postag) {
            let mut len = 0;
            if get_flag(postag, "CONJ") != '-' {
                len += 1;
            }
            if get_flag(postag, "JAR") != '-' {
                len += 1;
            }
            if is_definite(postag) {
                if get_flag(postag, "JAR") == 'L' {
                    len += 1;
                } else {
                    len += 2;
                }
            }
            len
        } else {
            return String::new();
        };
        chars[..prefix_len_chars.min(chars.len())].iter().collect()
    }

    pub fn get_enclitic(&self, token: &AnalyzedToken) -> String {
        let postag = token.pos_tags().first().map(String::as_str).unwrap_or("");
        let word = token.token();
        if postag.is_empty() {
            return String::new();
        }
        let flag = get_flag(postag, "PRONOUN");
        if flag == '-' {
            return get_pronoun_suffix(postag);
        }
        let suffixes = [
            "\u{647}\u{645}\u{627}",
            "\u{643}\u{645}\u{627}",
            "\u{647}\u{627}",
            "\u{647}\u{645}",
            "\u{647}\u{646}",
            "\u{643}\u{645}",
            "\u{643}\u{646}",
            "\u{646}\u{64a}",
            "\u{646}\u{627}",
            "\u{643}",
            "\u{647}",
        ];
        for suffix in suffixes {
            if word.ends_with(suffix) {
                return suffix.to_string();
            }
        }
        String::new()
    }

    fn remove_tashkeel(word: &str) -> String {
        word.chars()
            .filter(|c| !TASHKEEL_CHARS.contains(c))
            .collect()
    }

    fn tag_token(&self, atr: &AnalyzedTokenReadings) -> AnalyzedTokenReadings {
        let surface = atr.token().token();
        let start = atr.token().start();
        let end = atr.token().end();

        let readings = self.tag_readings(surface, start, end);
        if readings.is_empty() {
            return atr.clone();
        }

        let all_tags: Vec<String> = readings
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect();
        let lemma = readings
            .first()
            .and_then(|r| r.lemma().map(str::to_string))
            .unwrap_or_else(|| surface.to_string());
        let primary = AnalyzedToken::new(surface, start, end)
            .with_pos_tags(all_tags)
            .with_lemma(lemma);
        AnalyzedTokenReadings::new(primary).with_readings(readings)
    }

    fn additional_tags(
        &self,
        word: &str,
        striped: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnalyzedToken> {
        let mut additional = Vec::new();
        let chars: Vec<char> = striped.chars().collect();
        for i in prefix_index_list(&chars) {
            for j in suffix_index_list(&chars) {
                if i == 0 && j == chars.len() {
                    continue;
                }
                let tags = get_tags(&chars, i, j, self.new_style_pronoun_tag);
                for stem in get_stem(&chars, i, j) {
                    for (lemma, postag) in self.inner.lookup(&stem) {
                        if let Some(modified) = modify_pos_tag(postag, &tags) {
                            additional.push(
                                AnalyzedToken::new(word, start, end)
                                    .with_lemma(lemma.to_string())
                                    .with_pos_tags(vec![modified]),
                            );
                        }
                    }
                }
            }
        }
        additional
    }
}

fn prefix_index_list(chars: &[char]) -> Vec<usize> {
    let mut indexes = vec![0usize];
    let word: String = chars.iter().collect();
    let starts_with = |p: &str| word.starts_with(p);

    if starts_with("\u{648}\u{643}\u{627}\u{644}")
        || starts_with("\u{648}\u{628}\u{627}\u{644}")
        || starts_with("\u{641}\u{643}\u{627}\u{644}")
        || starts_with("\u{641}\u{628}\u{627}\u{644}")
    {
        indexes.push(4);
    }

    if starts_with("\u{648}\u{644}\u{644}")
        || starts_with("\u{641}\u{644}\u{644}")
        || starts_with("\u{641}\u{627}\u{644}")
        || starts_with("\u{648}\u{627}\u{644}")
        || starts_with("\u{628}\u{627}\u{644}")
        || starts_with("\u{643}\u{627}\u{644}")
    {
        indexes.push(3);
    }

    if starts_with("\u{644}\u{644}")
        || starts_with("\u{648}\u{643}")
        || starts_with("\u{648}\u{644}")
        || starts_with("\u{648}\u{628}")
        || starts_with("\u{641}\u{643}")
        || starts_with("\u{641}\u{644}")
        || starts_with("\u{641}\u{628}")
        || starts_with("\u{627}\u{644}")
        || starts_with("\u{641}\u{633}\u{623}")
        || starts_with("\u{641}\u{633}\u{646}")
        || starts_with("\u{641}\u{633}\u{64a}")
        || starts_with("\u{641}\u{633}\u{62a}")
        || starts_with("\u{648}\u{633}\u{623}")
        || starts_with("\u{648}\u{633}\u{646}")
        || starts_with("\u{648}\u{633}\u{64a}")
        || starts_with("\u{648}\u{633}\u{62a}")
    {
        indexes.push(2);
    }

    if starts_with("\u{643}")
        || starts_with("\u{644}")
        || starts_with("\u{628}")
        || starts_with("\u{648}")
        || starts_with("\u{641}")
        || starts_with("\u{633}\u{623}")
        || starts_with("\u{633}\u{646}")
        || starts_with("\u{633}\u{64a}")
        || starts_with("\u{633}\u{62a}")
    {
        indexes.push(1);
    }

    indexes
}

fn suffix_index_list(chars: &[char]) -> Vec<usize> {
    let mut indexes = vec![chars.len()];
    let word: String = chars.iter().collect();
    let suffix_len = if word.ends_with('\u{643}') {
        Some(1)
    } else if word.ends_with("\u{647}\u{645}\u{627}") || word.ends_with("\u{643}\u{645}\u{627}") {
        Some(3)
    } else if word.ends_with("\u{647}\u{627}")
        || word.ends_with("\u{647}\u{645}")
        || word.ends_with("\u{647}\u{646}")
        || word.ends_with("\u{643}\u{645}")
        || word.ends_with("\u{643}\u{646}")
        || word.ends_with("\u{646}\u{627}")
    {
        Some(2)
    } else {
        None
    };
    if let Some(len) = suffix_len {
        indexes.push(chars.len() - len);
    }
    indexes
}

fn get_tags(chars: &[char], pos_start: usize, pos_end: usize, new_style: bool) -> Vec<String> {
    let mut tags = Vec::new();
    let mut prefix: String = chars[..pos_start].iter().collect();
    let suffix: String = chars[pos_end..].iter().collect();

    if prefix.starts_with('\u{648}') || prefix.starts_with('\u{641}') {
        tags.push("CONJ;W".to_string());
        prefix = prefix
            .trim_start_matches(['\u{648}', '\u{641}'])
            .to_string();
    }
    if prefix.starts_with('\u{643}') {
        tags.push("JAR;K".to_string());
    } else if prefix.starts_with('\u{644}') {
        tags.push("JAR;L".to_string());
    } else if prefix.starts_with('\u{628}') {
        tags.push("JAR;B".to_string());
    } else if prefix.starts_with('\u{633}') {
        tags.push("ISTIQBAL;S".to_string());
    }
    if prefix.ends_with("\u{627}\u{644}") || prefix.ends_with("\u{644}\u{644}") {
        tags.push("PRONOUN;D".to_string());
    }

    let pronoun_flag = if new_style {
        match suffix.as_str() {
            "\u{646}\u{64a}" => 'b',
            "\u{646}\u{627}" => 'c',
            "\u{643}" => 'd',
            "\u{643}\u{645}\u{627}" => 'e',
            "\u{643}\u{645}" => 'f',
            "\u{643}\u{646}" => 'g',
            "\u{647}" => 'H',
            "\u{647}\u{627}" => 'i',
            "\u{647}\u{645}\u{627}" => 'j',
            "\u{647}\u{645}" => 'k',
            "\u{647}\u{646}" => 'n',
            _ => '\0',
        }
    } else {
        match suffix.as_str() {
            "\u{646}\u{64a}"
            | "\u{646}\u{627}"
            | "\u{643}"
            | "\u{643}\u{645}\u{627}"
            | "\u{643}\u{645}"
            | "\u{643}\u{646}"
            | "\u{647}"
            | "\u{647}\u{627}"
            | "\u{647}\u{645}\u{627}"
            | "\u{647}\u{645}"
            | "\u{647}\u{646}" => 'H',
            _ => '\0',
        }
    };
    if pronoun_flag != '\0' {
        tags.push(format!("PRONOUN;{pronoun_flag}"));
    }

    tags
}

fn get_stem(chars: &[char], pos_start: usize, pos_end: usize) -> Vec<String> {
    let mut stem: String = chars[pos_start..].iter().collect();
    if pos_end != chars.len() {
        stem = PRONOUN_SUFFIX_RE.replace(&stem, "\u{647}").into_owned();
    }
    let prefix: String = chars[..pos_start].iter().collect();
    let mut stems = Vec::new();
    if prefix.ends_with("\u{644}\u{644}") {
        stems.push(format!("\u{644}{stem}"));
    }
    stems.push(stem);
    stems
}

impl og_core::checker::Tagger for ArabicTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            *atr = self.tag_token(atr);
        }
    }
}

impl crate::Tagger for ArabicTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for token_text in tokens {
            let start = byte_offset;
            let end = start + token_text.len();

            let readings = self.tag_readings(token_text, start, end);
            if readings.is_empty() {
                results.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    *token_text,
                    start,
                    end,
                )));
            } else {
                let all_tags: Vec<String> = readings
                    .iter()
                    .flat_map(|r| r.pos_tags().to_vec())
                    .collect();
                let lemma = readings
                    .first()
                    .and_then(|r| r.lemma().map(str::to_string))
                    .unwrap_or_else(|| (*token_text).to_string());
                let primary = AnalyzedToken::new(*token_text, start, end)
                    .with_pos_tags(all_tags)
                    .with_lemma(lemma);
                results.push(AnalyzedTokenReadings::new(primary).with_readings(readings));
            }
            byte_offset = end;
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_direct_lookup_count() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        );
        let t = ArabicTagger::from_resource_dir(dir);
        let toks = ["مليونين".to_string()];
        let refs: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
        let out = crate::Tagger::tag(&t, &refs);
        for atr in &out {
            eprintln!("readings {}", atr.readings().len());
            for r in atr.readings() {
                eprintln!(
                    "  {} [{}]",
                    r.lemma().unwrap_or("-"),
                    r.pos_tags().join(",")
                );
            }
        }
    }

    #[test]
    fn modifies_conjunction_flag_on_prefixed_noun_tag() {
        let base = "NNH;-2A;---";
        let modified = modify_pos_tag(base, &["CONJ;W".to_string()]).expect("compatible");
        let chars: Vec<char> = modified.chars().collect();
        assert_eq!(
            chars[9], 'W',
            "CONJ flag set at noun position 9: {modified}"
        );
    }

    #[test]
    fn rejects_jar_on_non_majrour_noun() {
        let base = "NNH;-2U;---";
        assert!(modify_pos_tag(base, &["JAR;L".to_string()]).is_none());
    }

    #[test]
    fn prefix_and_suffix_index_lists_match_java() {
        let w: Vec<char> = "\u{648}\u{645}\u{626}\u{62a}\u{627}\u{646}"
            .chars()
            .collect();
        assert_eq!(prefix_index_list(&w), vec![0, 1]);
        assert_eq!(suffix_index_list(&w), vec![w.len()]);

        let w2: Vec<char> = "\u{648}\u{627}\u{644}\u{628}\u{64a}\u{62a}"
            .chars()
            .collect();
        assert_eq!(prefix_index_list(&w2), vec![0, 3, 1]);
    }

    #[test]
    fn stem_replaces_attached_pronoun() {
        let chars: Vec<char> = "\u{643}\u{62a}\u{627}\u{628}\u{647}\u{645}"
            .chars()
            .collect();
        let stems = get_stem(&chars, 0, chars.len() - 2);
        assert_eq!(
            stems,
            vec!["\u{643}\u{62a}\u{627}\u{628}\u{647}".to_string()]
        );
    }
}

#[cfg(test)]
mod java_parity_tests {
    use super::*;

    fn ar_dir() -> std::path::PathBuf {
        [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
            "languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("arabic.dict").exists())
        .expect("arabic resources")
    }

    fn readings(word: &str) -> Vec<(String, String)> {
        let tagger = ArabicTagger::from_resource_dir(&ar_dir());
        let out = crate::Tagger::tag(&tagger, &[word]);
        let mut pairs: Vec<(String, String)> = out[0]
            .readings()
            .iter()
            .map(|r| {
                (
                    r.lemma().unwrap_or_default().to_string(),
                    r.pos_tags().first().cloned().unwrap_or_default(),
                )
            })
            .collect();
        pairs.sort();
        pairs
    }

    fn expect(word: &str, expected: &[(&str, &str)]) {
        let mut wanted: Vec<(String, String)> = expected
            .iter()
            .map(|(l, t)| (l.to_string(), t.to_string()))
            .collect();
        wanted.sort();
        assert_eq!(readings(word), wanted, "word {word}");
    }

    #[test]
    fn java_tagger_dictionary_words() {
        expect(
            "الخياريتان",
            &[
                ("خيار", "NA-;F3--;--L"),
                ("خيار", "NJ-;F2--;--L"),
                ("خيار", "NJ-;F3--;--L"),
            ],
        );

        expect(
            "والبلاد",
            &[
                ("بلاد", "NJ-;F3--;W-L"),
                ("بلاد", "NJ-;F3A-;W-L"),
                ("بلاد", "NJ-;F3I-;W-L"),
                ("بلاد", "NJ-;F3U-;W-L"),
                ("بلاد", "NJ-;M1--;W-L"),
                ("بلاد", "NJ-;M1A-;W-L"),
                ("بلاد", "NJ-;M1I-;W-L"),
                ("بلاد", "NJ-;M1U-;W-L"),
            ],
        );

        expect(
            "بلادهما",
            &[
                ("بلاد", "NJ-;F3--;--H"),
                ("بلاد", "NJ-;F3A-;--H"),
                ("بلاد", "NJ-;F3I-;--H"),
                ("بلاد", "NJ-;F3U-;--H"),
                ("بلاد", "NJ-;M1--;--H"),
                ("بلاد", "NJ-;M1A-;--H"),
                ("بلاد", "NJ-;M1I-;--H"),
                ("بلاد", "NJ-;M1U-;--H"),
            ],
        );

        expect(
            "كبلاد",
            &[
                ("بلاد", "NJ-;F3--;-K-"),
                ("بلاد", "NJ-;F3I-;-K-"),
                ("بلاد", "NJ-;M1--;-K-"),
                ("بلاد", "NJ-;M1I-;-K-"),
            ],
        );

        expect(
            "للبلاد",
            &[
                ("بلاد", "NJ-;F3--;-LL"),
                ("بلاد", "NJ-;F3I-;-LL"),
                ("بلاد", "NJ-;M1--;-LL"),
                ("بلاد", "NJ-;M1I-;-LL"),
            ],
        );

        expect(
            "ببلاد",
            &[
                ("بلاد", "NJ-;F3--;-B-"),
                ("بلاد", "NJ-;F3I-;-B-"),
                ("بلاد", "NJ-;M1--;-B-"),
                ("بلاد", "NJ-;M1I-;-B-"),
            ],
        );
    }

    #[test]
    fn java_tagger_sentences_and_verbs() {
        expect(
            "السماء",
            &[
                ("سماء", "NJ-;F1--;--L"),
                ("سماء", "NJ-;F1A-;--L"),
                ("سماء", "NJ-;F1I-;--L"),
                ("سماء", "NJ-;F1U-;--L"),
            ],
        );
        expect(
            "زرقاء",
            &[
                ("زرقاء", "NA-;F1--;---"),
                ("زرقاء", "NA-;F1A-;---"),
                ("زرقاء", "NA-;F1I-;---"),
                ("زرقاء", "NA-;F1U-;---"),
            ],
        );

        expect(
            "فاستعملها",
            &[("اِسْتَعْمَلَ", "V61;M1H-pa-;W-H"), ("اِسْتَعْمَلَ", "V61;M1Y-i--;W-H")],
        );

        expect(
            "سيعملون",
            &[
                ("أَعْمَلَ", "V41;M3H-faU;-S-"),
                ("أَعْمَلَ", "V41;M3H-fpU;-S-"),
                ("عَمِلَ", "V31;M3H-faU;-S-"),
                ("عَمِلَ", "V31;M3H-fpU;-S-"),
                ("ع\u{64e}م\u{651}\u{64e}ل\u{64e}", "V41;M3H-faU;-S-"),
                ("ع\u{64e}م\u{651}\u{64e}ل\u{64e}", "V41;M3H-fpU;-S-"),
            ],
        );

        expect(
            "فسيعملون",
            &[
                ("أَعْمَلَ", "V41;M3H-faU;WS-"),
                ("أَعْمَلَ", "V41;M3H-fpU;WS-"),
                ("عَمِلَ", "V31;M3H-faU;WS-"),
                ("عَمِلَ", "V31;M3H-fpU;WS-"),
                ("ع\u{64e}م\u{651}\u{64e}ل\u{64e}", "V41;M3H-faU;WS-"),
                ("ع\u{64e}م\u{651}\u{64e}ل\u{64e}", "V41;M3H-fpU;WS-"),
            ],
        );

        let mut kitab: Vec<(String, String)> = Vec::new();
        for cat in ["NA-", "NJ-", "NM-"] {
            for infl in ["--", "A-", "I-", "U-"] {
                let number = if cat == "NA-" { "-3" } else { "M1" };
                kitab.push(("كتاب".to_string(), format!("{cat};{number}{infl};---")));
            }
        }
        let mut pairs = readings("كتاب");
        pairs.sort();
        kitab.sort();
        assert_eq!(pairs, kitab);
    }

    #[test]
    fn unknown_word_stays_untagged() {
        let tagger = ArabicTagger::from_resource_dir(&ar_dir());
        let out = crate::Tagger::tag(&tagger, &["العباره"]);
        assert_eq!(out[0].readings().len(), 1);
        assert!(out[0].readings()[0].pos_tags().is_empty());
    }

    #[test]
    fn has_dictionary_and_new_style_flag_round_trip() {
        let tagger = ArabicTagger::from_resource_dir(&ar_dir());
        assert!(tagger.has_dictionary());
        let styled = ArabicTagger::from_resource_dir(&ar_dir()).with_new_style_pronoun_tag();
        let out = crate::Tagger::tag(&styled, &["كتاب"]);
        assert!(!out[0].readings().is_empty());
    }
}

#[cfg(test)]
mod flag_tests {
    use super::*;

    const NOUN: &str = "NJ-;F3--;---";
    const MAJROUR: &str = "NJ-;F3I-;---";
    const VERB: &str = "V41;M3H-paU;---";
    const PART: &str = "PART;---;---";

    #[test]
    fn get_flag_reads_declared_positions() {
        assert_eq!(get_flag(NOUN, "CATEGORY"), 'J');
        assert_eq!(get_flag(NOUN, "GENDER"), 'F');
        assert_eq!(get_flag(NOUN, "NUMBER"), '3');
        assert_eq!(get_flag(NOUN, "CASE"), '-');
        assert_eq!(get_flag(NOUN, "CONJ"), '-');
        assert_eq!(get_flag(VERB, "TRANS"), '1');
        assert_eq!(get_flag(VERB, "TENSE"), 'p');
        assert_eq!(get_flag(VERB, "VOICE"), 'a');
        assert_eq!(get_flag(VERB, "ISTIQBAL"), '-');
        assert_eq!(get_flag(PART, "OPTION"), 'R');
        assert_eq!(get_flag("QAB", "CASE"), 'Q');
        assert_eq!(get_flag("", "CASE"), '-');
        assert_eq!(get_flag(NOUN, "UNKNOWN"), 'N');
    }

    #[test]
    fn set_flag_writes_only_in_range_positions() {
        assert_eq!(set_flag(NOUN, "PRONOUN", 'H'), "NJ-;F3--;--H");
        assert_eq!(set_flag(NOUN, "CONJ", 'W'), "NJ-;F3--;W--");
        assert_eq!(set_flag(NOUN, "JAR", 'K'), "NJ-;F3--;-K-");
        assert_eq!(set_flag(VERB, "CONJ", 'W'), "V41;M3H-paU;W--");
        assert_eq!(set_flag(VERB, "ISTIQBAL", 'L'), "V41;M3H-paU;-L-");
        assert_eq!(set_flag(PART, "PRONOUN", 'H'), "PART;---;-H-");
        assert_eq!(set_flag("Q", "CASE", 'I'), "I");
    }

    #[test]
    fn category_predicates_follow_the_first_char() {
        assert!(is_noun(NOUN));
        assert!(!is_noun(VERB));
        assert!(is_verb(VERB));
        assert!(is_stopword_tag(PART));
        assert!(is_future_tense("V41;M3H-faU;---"));
        assert!(!is_future_tense(VERB));
        assert!(!is_future_tense(NOUN));
        assert!(is_unattached_noun(NOUN));
        assert!(!is_unattached_noun("NJ-;F3--;--H"));
        assert!(!is_unattached_noun("NJ-;F3--;--X"));
    }

    #[test]
    fn majrour_accepts_i_or_missing_case() {
        assert!(is_majrour(MAJROUR));
        assert!(is_majrour(NOUN));
        assert!(!is_majrour("NJ-;F3U-;---"));
        assert!(is_majrour("N"));
    }

    #[test]
    fn attached_definite_jar_pronoun_conjunction_predicates() {
        assert!(is_attached("NJ-;F3--;--H"));
        assert!(!is_attached(NOUN));
        assert!(!is_attached(PART));

        assert!(is_definite("NJ-;F3--;--L"));
        assert!(!is_definite(NOUN));
        assert!(!is_definite(VERB));

        assert!(has_jar("NJ-;F3--;-K-"));
        assert!(!has_jar(NOUN));
        assert!(!has_jar(PART));

        assert!(has_pronoun("NJ-;F3--;--H"));
        assert!(!has_pronoun(NOUN));

        assert!(has_conjunction("NJ-;F3--;W--"));
        assert!(!has_conjunction(NOUN));
        assert!(has_conjunction("V41;M3H-paU;W--"));
        assert!(has_conjunction("PART;---F---"));
        assert!(has_conjunction(PART));
        assert!(!has_conjunction("PART;---W---"));
    }

    #[test]
    fn prefixes_and_suffixes_decode_from_flags() {
        assert_eq!(get_definite_prefix(""), "");
        assert_eq!(get_definite_prefix("NJ-;F3I-;-LL"), "ل");
        assert_eq!(get_definite_prefix("NJ-;F3I-;--L"), "ال");
        assert_eq!(get_definite_prefix(VERB), "");

        assert_eq!(get_jar_prefix(""), "");
        assert_eq!(get_jar_prefix("NJ-;F3I-;-L-"), "ل");
        assert_eq!(get_jar_prefix("NJ-;F3I-;-K-"), "ك");
        assert_eq!(get_jar_prefix("NJ-;F3I-;-B-"), "ب");
        assert_eq!(get_jar_prefix(NOUN), "");
        assert_eq!(get_jar_prefix(PART), "");

        assert_eq!(get_conjunction_prefix("NJ-;F3--;F--"), "ف");
        assert_eq!(get_conjunction_prefix("NJ-;F3--;W--"), "و");
        assert_eq!(get_conjunction_prefix(NOUN), "");
    }

    #[test]
    fn pronoun_suffix_map_matches_java_letters() {
        assert_eq!(get_pronoun_suffix(""), "");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--b"), "ني");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--c"), "نا");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--d"), "ك");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--e"), "كما");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--f"), "كم");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--g"), "كن");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--H"), "ه");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--i"), "ها");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--j"), "هما");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--k"), "هم");
        assert_eq!(get_pronoun_suffix("NJ-;F3--;--n"), "هن");
        assert_eq!(get_pronoun_suffix(NOUN), "");
    }

    #[test]
    fn unify_pronoun_tag_normalizes_attached_readings() {
        assert_eq!(
            unify_pronoun_tag("NJ-;F3--;--d".to_string()),
            "NJ-;F3--;--d"
        );
        assert_eq!(
            unify_pronoun_tag("NJ-;F3--;--H".to_string()),
            "NJ-;F3--;--H"
        );
        assert_eq!(unify_pronoun_tag(NOUN.to_string()), NOUN);
    }

    #[test]
    fn set_conjunction_accepts_waw_faa_and_dashes() {
        assert_eq!(set_conjunction(NOUN, "و"), "NJ-;F3--;W--");
        assert_eq!(set_conjunction(NOUN, "W"), "NJ-;F3--;W--");
        assert_eq!(set_conjunction(NOUN, "ف"), "NJ-;F3--;W--");
        assert_eq!(set_conjunction(NOUN, "F"), "NJ-;F3--;W--");
        assert_eq!(set_conjunction("NJ-;F3--;W--", "-"), NOUN);
        assert_eq!(set_conjunction("NJ-;F3--;W--", ""), NOUN);
        assert_eq!(set_conjunction(NOUN, "Q"), NOUN);
        assert_eq!(set_conjunction(PART, "و"), PART);
    }

    #[test]
    fn set_procletic_flags_clears_prefix_markers_per_category() {
        assert_eq!(set_procletic_flags(""), "");
        assert_eq!(set_procletic_flags("V41;M3H-paU;WS-"), "V41;M3H-paU;---");
        assert_eq!(set_procletic_flags("NJ-;F3--;WLL"), NOUN);
        assert_eq!(set_procletic_flags("NJ-;F3--;WK-"), NOUN);
        assert_eq!(set_procletic_flags("NJ-;F3--;--L"), "NJ-;F3--;---");
        assert_eq!(set_procletic_flags("PART;---W---"), "PART;-------");
        assert_eq!(set_procletic_flags("XYZ"), "XYZ");
    }

    #[test]
    fn merge_pos_tag_combines_same_category_flags() {
        assert_eq!(merge_pos_tag("", NOUN), NOUN);
        assert_eq!(merge_pos_tag(NOUN, ""), NOUN);
        assert_eq!(merge_pos_tag(NOUN, NOUN), NOUN);
        assert_eq!(merge_pos_tag(NOUN, "NP-;F3--;---"), "NP-;F3--;---");
        assert_eq!(merge_pos_tag("NN", NOUN), "NN");
        assert_eq!(merge_pos_tag(VERB, "VBT;M3H-paU;---"), "VBT;M3H-paU;---");
        assert_eq!(merge_pos_tag("VB", "VBT;M3H-paU;---"), "VB");
        assert_eq!(merge_pos_tag(PART, "PBRT;---;---"), "PBRT;---;---");
        assert_eq!(merge_pos_tag("PA", "PBRT;---;---"), "PA");
        assert_eq!(merge_pos_tag(PART, NOUN), NOUN);
        assert_eq!(merge_pos_tag("PART;---;-H-", NOUN), "NJ-;F3--;--H");
        assert_eq!(merge_pos_tag(VERB, NOUN), NOUN);
        assert_eq!(merge_pos_tag("V41;M3H-paU;W-H", NOUN), "NJ-;F3--;W-H");
        assert_eq!(merge_pos_tag(NOUN, VERB), VERB);
    }

    #[test]
    fn modify_pos_tag_applies_and_rejects_arabic_clitics() {
        assert_eq!(
            modify_pos_tag(NOUN, &["W".to_string()]).expect("conj"),
            "NJ-;F3--;W--"
        );
        assert_eq!(
            modify_pos_tag(MAJROUR, &["K".to_string()]).expect("jar k"),
            "NJ-;F3I-;-K-"
        );
        assert!(modify_pos_tag("NJ-;F3U-;---", &["K".to_string()]).is_none());
        assert_eq!(
            modify_pos_tag(MAJROUR, &["B".to_string()]).expect("jar b"),
            "NJ-;F3I-;-B-"
        );
        assert!(modify_pos_tag("NJ-;F3U-;---", &["B".to_string()]).is_none());
        assert_eq!(
            modify_pos_tag(MAJROUR, &["L".to_string()]).expect("jar l"),
            "NJ-;F3I-;-L-"
        );
        assert!(modify_pos_tag("NJ-;F3U-;---", &["L".to_string()]).is_none());
        assert_eq!(
            modify_pos_tag(VERB, &["L".to_string()]).expect("istiqbal"),
            "V41;M3H-paU;-L-"
        );
        assert_eq!(
            modify_pos_tag(NOUN, &["D".to_string()]).expect("definite"),
            "NJ-;F3--;--L"
        );
        assert!(modify_pos_tag("NJ-;F3--;--H", &["D".to_string()]).is_none());
        assert_eq!(
            modify_pos_tag("V41;M3H-faU;---", &["S".to_string()]).expect("future"),
            "V41;M3H-faU;-S-"
        );
        assert!(modify_pos_tag(VERB, &["S".to_string()]).is_none());
        assert_eq!(
            modify_pos_tag(NOUN, &["PRONOUN;H".to_string()]).expect("pronoun"),
            "NJ-;F3--;--H"
        );
        assert_eq!(
            modify_pos_tag(NOUN, &["X".to_string()]).expect("noop"),
            NOUN
        );
        assert!(modify_pos_tag(NOUN, &["W".to_string(), "K".to_string()]).is_some());
    }

    #[test]
    fn proclitic_and_enclitic_slice_the_surface() {
        let tagger = ArabicTagger::from_resource_dir(std::path::Path::new("/nonexistent"));
        let token = |word: &str, tag: &str| {
            AnalyzedToken::new(word, 0, word.len()).with_pos_tags(vec![tag.to_string()])
        };

        let bare = token("كتاب", NOUN);
        assert_eq!(tagger.get_proclitic(&bare), "");

        let definite = token("الكتاب", "NJ-;F3--;--L");
        assert_eq!(tagger.get_proclitic(&definite), "ال");

        let jarred = token("للكتاب", "NJ-;F3I;-LL");
        assert_eq!(tagger.get_proclitic(&jarred), "لل");

        let conjugated = token("وكتاب", "NJ-;F3--;W--");
        assert_eq!(tagger.get_proclitic(&conjugated), "و");

        let future = token("سيكتب", "V41;M3H-faU;WS-");
        assert_eq!(tagger.get_proclitic(&future), "سي");

        let plain_verb = token("يكتب", VERB);
        assert_eq!(tagger.get_proclitic(&plain_verb), "");

        let untagged = token("كتاب", "");
        assert_eq!(tagger.get_proclitic(&untagged), "");
        let particle = token("ثم", PART);
        assert_eq!(tagger.get_proclitic(&particle), "");

        let with_suffix = token("كتابهم", "NJ-;F3--;--H");
        assert_eq!(tagger.get_enclitic(&with_suffix), "هم");
        let with_ha = token("كتابها", "NJ-;F3--;--H");
        assert_eq!(tagger.get_enclitic(&with_ha), "ها");
        let with_k = token("كتابك", "NJ-;F3--;--H");
        assert_eq!(tagger.get_enclitic(&with_k), "ك");
        let no_suffix = token("كتاب", "NJ-;F3--;--H");
        assert_eq!(tagger.get_enclitic(&no_suffix), "");
        let unflagged = token("كتابه", NOUN);
        assert_eq!(tagger.get_enclitic(&unflagged), "");
        assert_eq!(tagger.get_enclitic(&untagged), "");
    }
}
