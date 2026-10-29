use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use regex::Regex;

pub fn reading_with_tag_regex<'a>(
    atr: &'a AnalyzedTokenReadings,
    re: &Regex,
) -> Option<&'a AnalyzedToken> {
    reading_and_tag_matching(atr, re).map(|(reading, _)| reading)
}

pub fn reading_and_tag_matching<'a>(
    atr: &'a AnalyzedTokenReadings,
    re: &Regex,
) -> Option<(&'a AnalyzedToken, &'a str)> {
    atr.readings().iter().find_map(|reading| {
        reading
            .pos_tags()
            .iter()
            .find(|tag| re.is_match(tag))
            .map(|tag| (reading, tag.as_str()))
    })
}

pub fn matches_pos_tag_regex(atr: &AnalyzedTokenReadings, re: &Regex) -> bool {
    reading_with_tag_regex(atr, re).is_some()
}

pub fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

pub fn has_any_partial_pos_tag(atr: &AnalyzedTokenReadings, partials: &[&str]) -> bool {
    atr.readings().iter().any(|r| {
        r.pos_tags()
            .iter()
            .any(|t| partials.iter().any(|p| t.contains(p)))
    })
}

pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

pub fn whitespace_before(tokens: &[&AnalyzedTokenReadings], idx: usize) -> bool {
    idx > 0 && idx < tokens.len() && tokens[idx].token().start() > tokens[idx - 1].token().end()
}

pub fn substring(s: &str, from: usize, to: usize) -> String {
    s.chars().skip(from).take(to.saturating_sub(from)).collect()
}

pub fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => s.to_string(),
    }
}

pub fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(char::is_uppercase)
        && chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
}

pub fn is_all_uppercase_word(s: &str) -> bool {
    !s.chars().any(|c| c.is_alphabetic() && c.is_lowercase())
}

pub fn preserve_case(input: &str, model: &str) -> String {
    if model.is_empty() {
        return input.to_string();
    }
    if is_capitalized_word(model) {
        return uppercase_first_char(&input.to_lowercase());
    }
    if is_all_uppercase_word(model) {
        return input.to_uppercase();
    }
    input.to_string()
}

pub fn preserve_case_word_by_word(input: &str, model: &str) -> String {
    let input_words: Vec<&str> = input.split(' ').collect();
    let model_words: Vec<&str> = model.split(' ').collect();
    if input_words.len() != model_words.len() {
        return preserve_case(input, model);
    }
    input_words
        .iter()
        .zip(model_words.iter())
        .map(|(i, m)| preserve_case(i, m))
        .collect::<Vec<_>>()
        .join(" ")
}
