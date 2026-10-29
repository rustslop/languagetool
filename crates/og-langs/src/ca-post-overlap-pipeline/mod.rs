use std::collections::HashSet;
use std::sync::OnceLock;

use og_core::RuleMatch;
use og_core::match_filters::original_error_str;
use og_core::rule_match::SuggestedReplacement;
use regex::Regex;

pub fn catalan_filter_rule_matches_after_overlapping(
    mut matches: Vec<RuleMatch>,
    text: &str,
) -> Vec<RuleMatch> {
    for m in &mut matches {
        m.trim_match_ends(text);
    }
    matches.sort_by_key(|m| m.offset());
    matches
}

pub fn catalan_filter_rule_matches(
    matches: Vec<RuleMatch>,
    text: &str,
    enabled_rules: &HashSet<String>,
) -> Vec<RuleMatch> {
    let mut results: Vec<RuleMatch> = Vec::with_capacity(matches.len());
    let mut ignore_match_in_pos: Option<usize> = None;
    let mut previous: Option<&RuleMatch> = None;
    for (i, rule_match) in matches.iter().enumerate() {
        let id = rule_match.rule().id();
        if id != "MORFOLOGIK_RULE_CA_ES"
            && rule_match.replacements().len() == 1
            && original_error_str(rule_match, text) == rule_match.replacements()[0].value()
        {
            continue;
        }
        if id == "IGNORE_PROPER_NOUNS" {
            let after_morfologik_at_same_pos = previous.is_some_and(|prev| {
                prev.rule().id() == "MORFOLOGIK_RULE_CA_ES" && rule_match.offset() == prev.offset()
            });
            if after_morfologik_at_same_pos {
                results.pop();
                ignore_match_in_pos = None;
                continue;
            }
            ignore_match_in_pos = Some(rule_match.offset());
            continue;
        }
        if id == "MORFOLOGIK_RULE_CA_ES" && ignore_match_in_pos == Some(rule_match.offset()) {
            ignore_match_in_pos = None;
            continue;
        }
        if id == "FALTA_ELEMENT_ENTRE_VERBS" {
            let full_id = falta_full_id(rule_match);
            let mut drop_current = false;
            if full_id == "FALTA_ELEMENT_ENTRE_VERBS[3]"
                || full_id == "FALTA_ELEMENT_ENTRE_VERBS[4]"
            {
                drop_current = matches.get(i + 1).is_some_and(|next| {
                    next.sentence().is_some_and(|s| !s.is_empty())
                        && falta_full_id(next) != "FALTA_ELEMENT_ENTRE_VERBS[5]"
                        && (next.offset() as i64)
                            - (rule_match.offset() as i64 + rule_match.length() as i64)
                            < 20
                });
            } else if full_id == "FALTA_ELEMENT_ENTRE_VERBS[5]"
                && i > 0
                && matches[i - 1].rule().id() == "FALTA_ELEMENT_ENTRE_VERBS"
            {
                drop_current = true;
            }
            if drop_current {
                continue;
            }
        }
        results.push(adjust_catalan_match(rule_match, text, enabled_rules));
        previous = Some(rule_match);
    }
    results.sort_by_key(|m| m.offset());
    results
}

fn adjust_catalan_match(m: &RuleMatch, text: &str, enabled_rules: &HashSet<String>) -> RuleMatch {
    let error_str = original_error_str(m, text);
    let originals: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
    if let Some(fixed) = fix_removed_word_double_space(m, text, &originals) {
        return fixed;
    }
    let has_typo_apostrophe = m
        .sentence()
        .is_some_and(sentence_has_typographic_apostrophe);
    let mut new_replacements: Vec<SuggestedReplacement> = Vec::with_capacity(originals.len());
    for original in &originals {
        let mut value = (*original).to_string();
        if error_str.chars().count() > 2
            && error_str.ends_with('\'')
            && !value.ends_with('\'')
            && !value.ends_with('\u{2019}')
        {
            value.push(' ');
        }
        if !java_equals_ignore_case(&value, "després")
            && enabled_rules.contains("EXIGEIX_ACCENTUACIO_GENERAL")
        {
            if value.contains('é') && originals.contains(&value.replace('é', "è").as_str()) {
                continue;
            }
            if value.contains('É') && originals.contains(&value.replace('É', "È").as_str()) {
                continue;
            }
        } else if enabled_rules.contains("EXIGEIX_ACCENTUACIO_VALENCIANA") {
            if value.contains('è') && originals.contains(&value.replace('è', "é").as_str()) {
                continue;
            }
            if value.contains('È') && originals.contains(&value.replace('È', "É").as_str()) {
                continue;
            }
        }
        if (enabled_rules.contains("APOSTROF_TIPOGRAFIC") || has_typo_apostrophe)
            && value.chars().count() > 1
            && !enabled_rules.contains("APOSTROF_RECTE")
        {
            value = value.replace('\'', "\u{2019}");
        }
        if enabled_rules.contains("EXIGEIX_POSSESSIUS_U") && value.chars().count() > 3 {
            value = possessius_v_lower()
                .replace_all(&value, "${1}u${2}")
                .into_owned();
            value = possessius_v_upper()
                .replace_all(&value, "${1}U${2}")
                .into_owned();
            value = value
                .replace("feina", "faena")
                .replace("feiner", "faener")
                .replace("feinera", "faenera");
        }
        let final_value = if !enabled_rules.contains("DIACRITICS_TRADITIONAL_RULES")
            && ca_old_diacritics().is_match(&value)
        {
            remove_old_diacritics(&value)
        } else {
            value
        };
        if new_replacements.iter().any(|r| r.value() == final_value) {
            continue;
        }
        new_replacements.push(SuggestedReplacement::new(final_value));
    }
    let mut out = m.clone();
    out.set_replacements(new_replacements);
    out
}

fn fix_removed_word_double_space(
    m: &RuleMatch,
    text: &str,
    originals: &[&str],
) -> Option<RuleMatch> {
    if originals.len() != 1 || !originals[0].is_empty() {
        return None;
    }
    let sentence = m.sentence()?;
    if sentence.is_empty() {
        return None;
    }
    let from = m.offset();
    let to = m.offset() + m.length();
    if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
        return None;
    }
    if text.as_bytes().get(to) != Some(&b' ') {
        return None;
    }
    let starts_sentence = sentence.starts_with(original_error_str(m, text));
    if from > 0 && !starts_sentence && !text[..from].ends_with(' ') {
        return None;
    }
    let mut out = m.clone();
    out.set_offset_length(from, m.length() + 1);
    out.set_replacements(vec![SuggestedReplacement::new("")]);
    Some(out)
}

fn falta_full_id(m: &RuleMatch) -> String {
    let id = m.rule().id();
    if id != "FALTA_ELEMENT_ENTRE_VERBS" {
        return id.to_owned();
    }
    let sub_id = serde_json::to_value(m.rule())
        .ok()
        .and_then(|v| v.get("subId").and_then(|s| s.as_str()).map(str::to_owned));
    match sub_id {
        Some(sub) => format!("{id}[{sub}]"),
        None => id.to_owned(),
    }
}

fn sentence_has_typographic_apostrophe(sentence: &str) -> bool {
    sentence
        .split_whitespace()
        .any(|t| t.chars().count() > 1 && t.contains('\u{2019}'))
}

fn java_equals_ignore_case(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x == y || char_ci_eq(x, y))
}

fn char_ci_eq(x: char, y: char) -> bool {
    let mut xu = x.to_uppercase();
    let mut yu = y.to_uppercase();
    if xu.next() == yu.next() && xu.next().is_none() && yu.next().is_none() {
        return true;
    }
    let mut xl = x.to_lowercase();
    let mut yl = y.to_lowercase();
    xl.next() == yl.next() && xl.next().is_none() && yl.next().is_none()
}

fn possessius_v_lower() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([mtsMTS]e)v(a|es)\b").unwrap())
}

fn possessius_v_upper() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([MTS]E)V(A|ES)\b").unwrap())
}

fn ca_old_diacritics() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^.*\b(sóc|dóna|dónes|vénen|véns|fóra|adéu|féu|desféu|vés|contrapèl)\b.*$")
            .unwrap()
    })
}

const OLD_DIACRITICS: &[(&str, &str)] = &[
    ("contrapèl", "contrapel"),
    ("Contrapèl", "Contrapel"),
    ("vés", "ves"),
    ("féu", "feu"),
    ("desféu", "desfeu"),
    ("adéu", "adeu"),
    ("dóna", "dona"),
    ("dónes", "dones"),
    ("sóc", "soc"),
    ("vénen", "venen"),
    ("véns", "vens"),
    ("fóra", "fora"),
    ("Vés", "Ves"),
    ("Féu", "Feu"),
    ("Desféu", "Desfeu"),
    ("Adéu", "Adeu"),
    ("Dóna", "Dona"),
    ("Dónes", "Dones"),
    ("Sóc", "Soc"),
    ("Vénen", "Venen"),
    ("Véns", "Vens"),
    ("Fóra", "Fora"),
];

fn remove_old_diacritics(s: &str) -> String {
    let mut out = s.to_string();
    for (from, to) in OLD_DIACRITICS {
        if out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out
}

#[cfg(test)]
mod tests;
