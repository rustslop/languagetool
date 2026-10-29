use std::sync::LazyLock;

use regex::Regex;

use super::pronoms_davant::transform_davant;
use crate::ca::pronoms_febles::{PronounPosition, transform, transform_darrere};

const REFLEXIVE_PRONOUNS: [(&str, &str); 6] = [
    ("1S", "em"),
    ("2S", "et"),
    ("3S", "es"),
    ("1P", "ens"),
    ("2P", "us"),
    ("3P", "es"),
];

const DATIVE_PRONOUNS: [(&str, &str); 7] = [
    ("1S", "em"),
    ("2S", "et"),
    ("3S", "li"),
    ("3C", "li"),
    ("1P", "ens"),
    ("2P", "us"),
    ("3P", "els"),
];

pub const L_REFLEXIVE_PRONOUNS: [&str; 6] = ["em", "et", "es", "ens", "us", "vos"];

pub fn get_reflexive_pronoun(key: &str) -> &'static str {
    REFLEXIVE_PRONOUNS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

pub fn get_dative_pronoun(key: &str) -> &'static str {
    DATIVE_PRONOUNS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

static P_CONTAINS_REFLEXIVE_PRONOUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.*([mts][e']|[e'][mts]|vos|us|ens|-nos|-vos).*$").unwrap());

fn contains_reflexive_regex_match(pronouns_str: &str) -> bool {
    P_CONTAINS_REFLEXIVE_PRONOUN.is_match(&pronouns_str.to_lowercase())
}

fn contains_any_reflexive_pronoun(pronouns_str: &str) -> bool {
    transform(&pronouns_str.to_lowercase(), PronounPosition::Normalized)
        .split(' ')
        .any(|pronoun| L_REFLEXIVE_PRONOUNS.contains(&pronoun))
}

pub fn do_add_pronoun_en(pronouns_str: &str, verb_str: &str, pronouns_after: bool) -> String {
    let mut pronoun_normalized = transform(pronouns_str, PronounPosition::Normalized);
    if pronoun_normalized.ends_with("hi") {
        pronoun_normalized = pronoun_normalized.replace("hi", "en hi");
    } else {
        pronoun_normalized.push_str(" en");
    }
    if pronouns_after {
        transform_darrere(&pronoun_normalized, verb_str)
    } else {
        transform_davant(&pronoun_normalized, verb_str)
    }
}

static REFLEXIVE_FIRST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(em|et|es|ens|us|vos)").unwrap());

pub fn do_remove_pronoun_reflexive(
    pronouns_str: &str,
    verb_str: &str,
    pronouns_after: bool,
) -> String {
    let pronouns_replacement = REFLEXIVE_FIRST
        .replacen(
            &transform(&pronouns_str.to_lowercase(), PronounPosition::Normalized),
            1,
            "",
        )
        .trim()
        .to_string();
    if pronouns_after {
        let darrere = transform_darrere(&pronouns_replacement, verb_str);
        if darrere.is_empty() {
            return verb_str.to_string();
        }
        format!("{verb_str}{darrere}")
    } else {
        let davant = transform_davant(&pronouns_replacement, verb_str);
        if davant.is_empty() {
            return verb_str.to_string();
        }
        format!("{davant}{verb_str}")
    }
}

pub fn do_add_pronoun_reflexive(
    pronouns_str: &str,
    verb_str: &str,
    first_verb_persona_number: &str,
    pronouns_after: bool,
) -> String {
    if pronouns_after {
        if contains_reflexive_regex_match(pronouns_str) {
            return format!("{verb_str}{}", transform_darrere(pronouns_str, verb_str));
        }
        if verb_str.ends_with('r') || verb_str.ends_with("re") {
            return format!("{verb_str}{}", transform_darrere("-se", verb_str));
        }
        return verb_str.to_string();
    }
    let mut pronoun_to_add = transform(pronouns_str, PronounPosition::Normalized);
    if !contains_any_reflexive_pronoun(pronouns_str) {
        pronoun_to_add = format!(
            "{} {}",
            get_reflexive_pronoun(first_verb_persona_number),
            pronoun_to_add
        );
    }
    format!("{}{verb_str}", transform_davant(&pronoun_to_add, verb_str))
}

pub fn do_add_pronoun_reflexive_en(
    pronouns_str: &str,
    verb_str: &str,
    first_verb_persona_number: &str,
    pronouns_after: bool,
) -> String {
    if pronouns_after {
        if contains_reflexive_regex_match(pronouns_str) {
            return format!(
                "{verb_str}{}",
                transform_darrere(&format!("{pronouns_str}'n"), verb_str)
            );
        }
        return format!("{verb_str}{}", transform_darrere("-se'n", verb_str));
    }
    if pronouns_str.is_empty() {
        let pronoun_to_add = transform_davant(
            &format!("{} en", get_reflexive_pronoun(first_verb_persona_number)),
            verb_str,
        );
        return format!("{pronoun_to_add}{verb_str}");
    }
    let pronoun_to_add = transform_davant(
        &format!(
            "es {} en",
            transform(pronouns_str, PronounPosition::Normalized)
        ),
        verb_str,
    );
    if !pronoun_to_add.is_empty() {
        format!("{pronoun_to_add}{verb_str}")
    } else {
        format!("{}{verb_str}", transform_davant(pronouns_str, verb_str))
    }
}

pub fn do_add_pronoun_reflexive_imperative(
    pronouns_str: &str,
    verb_str: &str,
    first_verb_persona_number: &str,
) -> String {
    if pronouns_str.is_empty() {
        let pronoun_to_add =
            transform_darrere(get_reflexive_pronoun(first_verb_persona_number), verb_str);
        return format!("{verb_str}{pronoun_to_add}");
    }
    String::new()
}

pub fn do_replace_em_en(pronouns_str: &str, verb_str: &str, _pronouns_after: bool) -> String {
    if pronouns_str.eq_ignore_ascii_case("em") {
        return format!("en {verb_str}");
    }
    if pronouns_str.eq_ignore_ascii_case("m'") {
        return format!("n'{verb_str}");
    }
    if pronouns_str.eq_ignore_ascii_case("m'hi") {
        return format!("n'hi {verb_str}");
    }
    String::new()
}

pub fn convert_pronouns_for_intransitive_verb(s: &str) -> String {
    s.replace("-se'l", "-se-li")
        .replace("se'l ", "se li ")
        .replace("l'", "li ")
        .replace("-lo", "-li")
        .replace("-la", "-li")
        .replace("la ", "li ")
        .replace("el ", "li ")
        .replace("ho", "hi")
}

static DE_WRONG_APOSTROPHATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^.*d'[^aeiouh].*$").unwrap());
static PRONOUN_MISSING_APOSTROPHATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.*)\be([stm]) (h?[aeiouh].*)$").unwrap());
static PRONOUN_WRONG_APOSTROPHATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([mts])'([^aeiouh].*)$").unwrap());
static PRONOUN_WRONG_HYPHEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(.*)(-[stm])e-(h[oi])$").unwrap());

pub fn fix_apostrophes(s: &str) -> String {
    let mut s = s.to_string();
    if DE_WRONG_APOSTROPHATION.is_match(&s) {
        s = s.replace("d'", "de ");
    }
    if let Some(c) = PRONOUN_MISSING_APOSTROPHATION.captures(&s) {
        s = format!("{}{}'{}", &c[1], &c[2], &c[3]);
    }
    if let Some(c) = PRONOUN_WRONG_APOSTROPHATION.captures(&s) {
        s = format!("e{} {}", &c[1], &c[2]);
    }
    if let Some(c) = PRONOUN_WRONG_HYPHEN.captures(&s) {
        s = format!("{}{}'{}", &c[1], &c[2], &c[3]);
    }
    s
}
