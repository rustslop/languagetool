use std::sync::LazyLock;

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::apostophation::needs_apostrophation_ms;
use super::pronoms_davant::transform_davant;
use super::token_scan::{has_any_partial_pos_tag, preserve_case, reading_and_tag_matching};
use crate::ca::pronoms_febles::transform_darrere;

static PX_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^PX.*$").unwrap());
static P_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^P.*$").unwrap());

fn dative_pronoun(key: &str) -> &'static str {
    match key {
        "1S" => "em",
        "2S" => "et",
        "3S" => "li",
        "3C" => "li",
        "1P" => "ens",
        "2P" => "us",
        "3P" => "els",
        _ => "",
    }
}

fn substring(s: &str, from: usize, len: usize) -> String {
    s.chars().skip(from).take(len).collect()
}

pub fn accept_rule_match(
    rm: &mut RuleMatch,
    pattern_token_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
) -> Option<()> {
    let mut pos_possessive = pattern_token_pos;
    while pos_possessive < tokens.len() && !has_any_partial_pos_tag(tokens[pos_possessive], &["PX"])
    {
        pos_possessive += 1;
    }
    if pos_possessive >= tokens.len() {
        return None;
    }
    let (_, possessive_tag) = reading_and_tag_matching(tokens[pos_possessive], &PX_TAGS)?;
    if possessive_tag.chars().count() < 7 {
        return None;
    }
    let number = substring(possessive_tag, 6, 1);
    let persona = substring(possessive_tag, 2, 1);

    let mut pos_verb = pattern_token_pos as isize - 1;
    while pos_verb > 0 && tokens[pos_verb as usize].has_chunk("GV") {
        pos_verb -= 1;
    }
    pos_verb += 1;

    let mut pronoun_found = false;
    let mut has_some_pronoun = false;
    let mut pos_pronoun = pos_verb - 1;
    while !pronoun_found && pos_pronoun > 0 && pronoun_at(tokens, pos_pronoun) {
        has_some_pronoun = true;
        if let Some((_, tag)) = reading_and_tag_matching(tokens[pos_pronoun as usize], &P_TAGS)
            && tag.chars().count() >= 5
        {
            let pronoun_persona = substring(tag, 2, 1);
            let pronoun_number = substring(tag, 4, 1);
            pronoun_found =
                pronoun_persona == persona && (number == "C" || pronoun_number == number);
        }
        pos_pronoun -= 1;
    }
    let mut pos_pronoun = pattern_token_pos as isize + 1;
    while !pronoun_found && pos_pronoun < tokens.len() as isize && pronoun_at(tokens, pos_pronoun) {
        has_some_pronoun = true;
        if let Some((_, tag)) = reading_and_tag_matching(tokens[pos_pronoun as usize], &P_TAGS)
            && tag.chars().count() >= 5
        {
            let pronoun_persona = substring(tag, 2, 1);
            let pronoun_number = substring(tag, 4, 1);
            pronoun_found =
                pronoun_persona == persona && (number == "C" || pronoun_number == number);
        }
        pos_pronoun += 1;
    }

    if pos_possessive == 0 || pos_possessive + 1 >= tokens.len() {
        return None;
    }
    let apostrophe_needed =
        has_any_partial_pos_tag(tokens[pos_possessive - 1], &["DA0MS0", "DA0FS0"])
            && needs_apostrophation_ms(tokens[pos_possessive + 1].token().token());
    if pronoun_found {
        if apostrophe_needed {
            let start = tokens[pos_possessive - 1].token().start();
            let end = tokens[pos_possessive + 1].token().end();
            rm.set_offset_length(start, end - start);
            rm.set_replacements(vec![SuggestedReplacement::new(format!(
                "l'{}",
                tokens[pos_possessive + 1].token().token()
            ))]);
        } else {
            let start = tokens[pos_possessive].token().start();
            let end = tokens[pos_possessive].token().end();
            rm.set_offset_length(start, end - start);
            rm.set_replacements(vec![SuggestedReplacement::new("")]);
        }
        return Some(());
    }
    if !has_some_pronoun {
        let pos_verb_idx = pos_verb.max(0) as usize;
        let verb_token = tokens[pos_verb_idx].token().token();
        let mut suggestion = String::new();
        if has_any_partial_pos_tag(tokens[pos_verb_idx], &["VMN", "VMG"]) {
            let pronoun_sugg =
                transform_darrere(dative_pronoun(&format!("{persona}{number}")), verb_token);
            suggestion.push_str(verb_token);
            suggestion.push_str(&pronoun_sugg);
        } else {
            let pronoun_sugg =
                transform_davant(dative_pronoun(&format!("{persona}{number}")), verb_token);
            suggestion.push_str(&preserve_case(&pronoun_sugg, verb_token));
            suggestion.push_str(&verb_token.to_lowercase());
        }
        let mut i = pos_verb + 1;
        while i <= pos_possessive as isize - 2 {
            let idx = i.max(0) as usize;
            if idx > 0 && tokens[idx].token().start() > tokens[idx - 1].token().end() {
                suggestion.push(' ');
            }
            suggestion.push_str(&tokens[idx].token().token().to_lowercase());
            i += 1;
        }
        if apostrophe_needed {
            suggestion.push(' ');
            suggestion.push_str(&format!("l'{}", tokens[pos_possessive + 1].token().token()));
        } else {
            let mut i = pos_possessive as isize - 1;
            while i <= pos_possessive as isize + 1 {
                if i == pos_possessive as isize {
                    i += 1;
                    continue;
                }
                let idx = i.max(0) as usize;
                if idx > 0 && tokens[idx].token().start() > tokens[idx - 1].token().end() {
                    suggestion.push(' ');
                }
                suggestion.push_str(tokens[idx].token().token());
                i += 1;
            }
        }
        let start = tokens[pos_verb_idx].token().start();
        let end = tokens[pos_possessive + 1].token().end();
        rm.set_offset_length(start, end - start);
        rm.set_replacements(vec![SuggestedReplacement::new(suggestion)]);
        return Some(());
    }
    None
}

fn pronoun_at(tokens: &[&AnalyzedTokenReadings], i: isize) -> bool {
    if i < 0 || i >= tokens.len() as isize {
        return false;
    }
    let t = tokens[i as usize];
    super::token_scan::has_pos_tag_starting_with(t, "PP")
        || super::token_scan::has_pos_tag_starting_with(t, "P0")
}
