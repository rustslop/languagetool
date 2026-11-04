use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};

const OPENING_QUOTES: [char; 8] = ['«', '“', '"', '‘', '\'', '„', '¿', '¡'];

fn is_punctuation_or_symbol(token: &str) -> bool {
    token
        .chars()
        .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn is_opening_quote(token: &str) -> bool {
    token.chars().count() == 1 && OPENING_QUOTES.contains(&token.chars().next().unwrap())
}

fn has_space_before(tokens: &[&AnalyzedTokenReadings], i: usize) -> bool {
    match i.checked_sub(1) {
        Some(prev) => tokens[prev].token().end() < tokens[i].token().start(),
        None => tokens.first().is_some_and(|t| t.token().start() > 0),
    }
}

pub fn add_commas_filter(
    tokens: &[&AnalyzedTokenReadings],
    sentence_base: usize,
    sentence_text: &str,
    mut rm: RuleMatch,
    args: &str,
) -> Option<RuleMatch> {
    let suggest_semicolon = args
        .split_whitespace()
        .any(|part| part.eq_ignore_ascii_case("suggestSemicolon:true"));
    let from = rm.offset();
    let to = rm.offset() + rm.length();
    let rel = |abs: usize| abs - sentence_base;
    let mut postag_from = 1usize;
    while postag_from < tokens.len() && tokens[postag_from].token().start() < from {
        postag_from += 1;
    }
    let mut postag_to = postag_from;
    while postag_to < tokens.len() && tokens[postag_to].token().end() < to {
        postag_to += 1;
    }
    let before_ok = postag_from == 1
        || is_punctuation_or_symbol(tokens[postag_from - 1].token().token())
        || is_capitalized_word(tokens[postag_from].token().token());
    let after_ok = tokens.get(postag_to + 1).is_some_and(|n| {
        is_punctuation_or_symbol(n.token().token())
            && !(has_space_before(tokens, postag_to + 1) && is_opening_quote(n.token().token()))
    });
    if before_ok && after_ok {
        return None;
    }
    let matched = &sentence_text[rel(from)..rel(to)];
    if suggest_semicolon
        && postag_from >= 1
        && postag_from - 1 < tokens.len()
        && tokens[postag_from - 1].token().token() == ","
        && !after_ok
    {
        let start = tokens[postag_from - 1].token().start();
        let end = tokens[postag_to].token().end();
        rm.set_offset_length(start, end - start);
        rm.set_replacements(vec![
            SuggestedReplacement::new(format!("; {matched},")),
            SuggestedReplacement::new(format!(", {matched},")),
        ]);
        return Some(rm);
    }
    if before_ok && !after_ok {
        let start = tokens[postag_to].token().start();
        rm.set_offset_length(start, to - start);
        rm.set_replacements(vec![SuggestedReplacement::new(format!(
            "{},",
            tokens[postag_to].token().token()
        ))]);
        return Some(rm);
    }
    if !before_ok && after_ok {
        let mut start = tokens[postag_from].token().start();
        if has_space_before(tokens, postag_from) {
            start = start.saturating_sub(1);
        }
        let end = tokens[postag_from].token().end();
        rm.set_offset_length(start, end - start);
        rm.set_replacements(vec![SuggestedReplacement::new(format!(
            ", {}",
            tokens[postag_from].token().token()
        ))]);
        return Some(rm);
    }
    let mut start = tokens[postag_from].token().start();
    if has_space_before(tokens, postag_from) {
        start = start.saturating_sub(1);
    }
    let end = tokens[postag_to].token().end();
    rm.set_offset_length(start, end - start);
    rm.set_replacements(vec![SuggestedReplacement::new(format!(", {matched},"))]);
    Some(rm)
}
