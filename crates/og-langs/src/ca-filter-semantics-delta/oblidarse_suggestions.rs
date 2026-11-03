use std::sync::LazyLock;

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::apostophation::needs_apostrophation_ms;
use super::ca_adapt::adapt_suggestion_ca;
use super::consumers::oblidarse_word_after_suffix;
use super::token_scan::{
    eq_ignore_case, preserve_case, reading_and_tag_matching, reading_with_tag_regex, substring,
    whitespace_before,
};
use super::verb_synthesizer::P_VERB;

static P_ANY_PRONOUN: LazyLock<Regex> = LazyLock::new(|| Regex::new("^P.*$").unwrap());
static P_WORD_AFTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^(D.*|V.N.*|P[DI].*|NC.*)$").unwrap());

fn add_reflexive(key: &str, with_en: bool, vowel: bool) -> Option<&'static str> {
    Some(match (with_en, vowel, key) {
        (true, true, "1S") => "me n'",
        (true, true, "2S") => "te n'",
        (true, true, "3S") => "se n'",
        (true, true, "1P") => "ens n'",
        (true, true, "2P") => "us n'",
        (true, true, "3P") => "se n'",
        (true, false, "1S") => "me'n ",
        (true, false, "2S") => "te'n ",
        (true, false, "3S") => "se'n ",
        (true, false, "1P") => "ens en ",
        (true, false, "2P") => "us en ",
        (true, false, "3P") => "se'n ",
        (false, true, "1S") => "m'",
        (false, true, "2S") => "t'",
        (false, true, "3S") => "s'",
        (false, true, "1P") => "ens ",
        (false, true, "2P") => "us ",
        (false, true, "3P") => "s'",
        (false, false, "1S") => "em ",
        (false, false, "2S") => "et ",
        (false, false, "3S") => "es ",
        (false, false, "1P") => "ens ",
        (false, false, "2P") => "us ",
        (false, false, "3P") => "es ",
        _ => return None,
    })
}

fn has_any_lemma(atr: &AnalyzedTokenReadings, lemmas: &[&str]) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.lemma().is_some_and(|l| lemmas.contains(&l)))
}

pub fn accept_rule_match(
    rm: &RuleMatch,
    from_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
    synth: &dyn Fn(&str, &str) -> Vec<String>,
) -> Option<RuleMatch> {
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    if pos_word + 2 >= tokens.len() {
        return None;
    }
    let (_, pronom_postag) = reading_and_tag_matching(tokens[pos_word + 1], &P_ANY_PRONOUN)?;
    let pronom_gender_number = format!(
        "{}{}",
        substring(pronom_postag, 2, 3),
        substring(pronom_postag, 4, 5)
    );
    let mut index_main_verb = pos_word + 2;
    loop {
        if index_main_verb >= tokens.len() {
            return None;
        }
        if has_any_lemma(tokens[index_main_verb], &["oblidar", "descuidar", "passar"]) {
            break;
        }
        index_main_verb += 1;
    }
    let (_, verb_postag) = reading_and_tag_matching(tokens[pos_word + 2], &P_VERB)?;
    let verb_lemma = reading_with_tag_regex(tokens[pos_word + 2], &P_VERB)?.lemma();
    let lemma = if verb_lemma == Some("passar") {
        "descuidar"
    } else {
        verb_lemma.unwrap_or("")
    };
    if verb_postag.chars().count() < 8 {
        return None;
    }
    let synth_postag = format!(
        "{}{}{}",
        substring(verb_postag, 0, 4),
        pronom_gender_number,
        substring(verb_postag, 6, 8)
    );
    let synth_forms = synth(lemma, &synth_postag);
    let mut new_verb = synth_forms.first()?.clone();
    for i in (pos_word + 3)..=(index_main_verb) {
        if whitespace_before(tokens, i) {
            new_verb.push(' ');
        }
        new_verb.push_str(
            &tokens[i]
                .token()
                .token()
                .replace("passar", "descuidar")
                .replace("passat", "descuidat")
                .replace("passant", "descuidant"),
        );
    }
    let verb_vowel = needs_apostrophation_ms(&new_verb);
    let mut word_after = String::new();
    if index_main_verb + 1 < tokens.len() {
        if let Some(reading) = reading_with_tag_regex(tokens[index_main_verb + 1], &P_WORD_AFTER) {
            word_after = reading.token().to_string();
        }
        let next_token = tokens[index_main_verb + 1].token().token();
        let next_lower = next_token.to_lowercase();
        if ["com", "de", "d'", "que"].contains(&next_lower.as_str()) {
            word_after = next_token.to_string();
        }
    }
    let use_en = word_after.is_empty()
        && !eq_ignore_case(&word_after, "de")
        && !eq_ignore_case(&word_after, "d'")
        && !eq_ignore_case(&word_after, "que");
    let mut sugg = String::new();
    sugg.push_str(add_reflexive(&pronom_gender_number, use_en, verb_vowel)?);
    sugg.push_str(&new_verb);
    let mut characters_after_correction = 0usize;
    if eq_ignore_case(&word_after, "el") || eq_ignore_case(&word_after, "els") {
        sugg.push_str(" d");
        sugg.push_str(&word_after.to_lowercase());
        characters_after_correction = word_after.chars().count() + 1;
    } else if !word_after.is_empty()
        && !eq_ignore_case(&word_after, "de")
        && !eq_ignore_case(&word_after, "d'")
        && !eq_ignore_case(&word_after, "que")
    {
        let (suffix, correction) = oblidarse_word_after_suffix(&word_after);
        sugg.push_str(suffix);
        characters_after_correction = correction;
    }
    let model_token = tokens[pos_word].token().token();
    let mut replacements = vec![preserve_case(&sugg, model_token)];
    for original in rm.replacements() {
        let mut s = original.value().to_string();
        if characters_after_correction == 1 {
            s.push(' ');
        }
        replacements.push(adapt_suggestion_ca(&s, model_token));
    }
    let mut out = rm.clone();
    let start = tokens[pos_word].token().start();
    let end = tokens[index_main_verb].token().end() + characters_after_correction;
    out.set_offset_length(start, end.saturating_sub(start));
    out.set_message(rm.message().replace("passar", "descuidar"));
    out.set_replacements(
        replacements
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(out)
}
