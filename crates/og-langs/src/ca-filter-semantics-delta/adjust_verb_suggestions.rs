use std::cmp::Ordering;

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};

use super::ca_adapt::adapt_suggestion_ca;
use super::consumers::{
    NEEDS_APOSTROPHE_CHANGE, NEEDS_CONTRACTION_CHANGE, any_change_vowel_consonant,
};
use super::pronoms_actions::{
    L_REFLEXIVE_PRONOUNS, convert_pronouns_for_intransitive_verb, do_add_pronoun_en,
    do_add_pronoun_reflexive, do_add_pronoun_reflexive_en, do_add_pronoun_reflexive_imperative,
    do_remove_pronoun_reflexive, do_replace_em_en, fix_apostrophes, get_dative_pronoun,
    get_reflexive_pronoun,
};
use super::pronoms_davant::transform_davant;
use super::token_scan::{has_any_partial_pos_tag, preserve_case, substring, whitespace_before};
use super::verb_synthesizer::VerbSynthesizer;
use crate::ca::pronoms_febles::{PronounPosition, transform, transform_darrere};

pub struct AdjustArgs<'a> {
    pub actions: &'a str,
    pub number_from_next_words: bool,
    pub force_number: &'a str,
}

pub struct AdjustServices<'a> {
    pub tagger: &'a dyn Fn(&str) -> Vec<AnalyzedTokenReadings>,
    pub synth: &'a dyn Fn(&str, &str) -> Vec<String>,
}

pub fn get_target_postag(postags: &[String], target_postag: &str) -> String {
    if postags.is_empty() {
        return target_postag.to_string();
    }
    let mut sorted = postags.to_vec();
    sorted.sort_by(|a, b| postag_comparator(a, b));
    sorted.pop().unwrap_or_default()
}

fn postag_comparator(a: &str, b: &str) -> Ordering {
    if a.chars().count() > 4 && b.chars().count() > 4 {
        if a.contains("3S") && b == "1S" {
            return Ordering::Greater;
        }
        if a.contains("1S") && b.contains("3S") {
            return Ordering::Less;
        }
        if a == "VMIP2P00" && b == "VMIS3S00" {
            return Ordering::Greater;
        }
        if b == "VMIP2P00" && a == "VMIS3S00" {
            return Ordering::Less;
        }
        if char_at(a, 2) == Some('I') && char_at(b, 2) != Some('I') {
            return Ordering::Greater;
        }
        if char_at(b, 2) == Some('I') && char_at(a, 2) != Some('I') {
            return Ordering::Less;
        }
        if char_at(a, 4) == Some('3') && char_at(b, 4) == Some('1') {
            return Ordering::Greater;
        }
        if char_at(b, 4) == Some('1') && char_at(a, 4) == Some('3') {
            return Ordering::Less;
        }
    }
    Ordering::Equal
}

fn char_at(s: &str, i: usize) -> Option<char> {
    s.chars().nth(i)
}

fn apply_desired_number(postag: &str, desired_number: &str) -> String {
    let bytes = postag.as_bytes();
    if !postag.is_ascii()
        || desired_number.is_empty()
        || bytes.len() < 6
        || bytes[2] == b'P'
        || !(bytes[5] == b'S' || bytes[5] == b'P')
    {
        return postag.to_string();
    }
    format!("{}{}{}", &postag[..5], desired_number, &postag[6..])
}

fn apply_desired_persona(postag: &str, desired_persona: &str) -> String {
    let bytes = postag.as_bytes();
    if !postag.is_ascii() || desired_persona.is_empty() || bytes.len() < 5 || bytes[2] == b'P' {
        return postag.to_string();
    }
    let person = bytes[4] as char;
    if !matches!(person, '1' | '2' | '3') {
        return postag.to_string();
    }
    format!("{}{}{}", &postag[..4], desired_persona, &postag[5..])
}

pub fn accept_rule_match(
    rm: &RuleMatch,
    sentence_text: &str,
    from_pos: usize,
    to_pos: usize,
    args: &AdjustArgs<'_>,
    tokens: &[&AnalyzedTokenReadings],
    services: &AdjustServices<'_>,
) -> Option<RuleMatch> {
    let mut replacements: Vec<String> = Vec::new();
    let default_actions = "removePronounReflexive".to_string();
    let actions: Vec<&str> = if args.actions.is_empty() {
        vec![default_actions.as_str()]
    } else {
        args.actions.split(',').collect()
    };
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    let verb_synthesizer = VerbSynthesizer::new(tokens, pos_word as isize);
    if verb_synthesizer.is_undefined()
        || tokens[verb_synthesizer.last_verb_index() as usize]
            .token()
            .end()
            > to_pos
    {
        return None;
    }
    for original_suggestion in rm.replacements() {
        let mut original_suggestion = original_suggestion.value().to_lowercase();
        let mut make_intransitive = false;
        let mut desired_number = String::new();
        let mut desired_persona = String::new();
        let mut action = actions[0].to_string();
        if let Some(stripped) = original_suggestion.strip_suffix(" [intr]") {
            original_suggestion = stripped.to_string();
            make_intransitive = true;
        }
        if let Some(stripped) = original_suggestion.strip_suffix(" [3s]") {
            original_suggestion = stripped.to_string();
            desired_number = "S".to_string();
            desired_persona = "3".to_string();
        }
        if let Some(stripped) = original_suggestion.strip_prefix("[datiu] ") {
            original_suggestion = stripped.to_string();
            action = "addPronounDative".to_string();
        }
        let (mut new_lemma, after_lemma) = match original_suggestion.find(' ') {
            Some(i) => (&original_suggestion[..i], &original_suggestion[i + 1..]),
            None => (original_suggestion.as_str(), ""),
        };
        if !after_lemma.is_empty() && args.number_from_next_words {
            let first_word = after_lemma.split(' ').next().unwrap_or(after_lemma);
            desired_number = if (services.tagger)(first_word)
                .first()
                .is_some_and(|atr| has_any_partial_pos_tag(atr, &["S"]))
            {
                "S".to_string()
            } else {
                "P".to_string()
            };
        }
        if new_lemma.contains("haver-hi") {
            desired_number = "S".to_string();
        }
        if !args.force_number.is_empty() {
            desired_number = args.force_number.to_string();
        }
        if let Some(stripped) = new_lemma.strip_suffix("-se'n") {
            new_lemma = stripped;
            action = "addPronounReflexiveEn".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("-se") {
            new_lemma = stripped;
            action = "addPronounReflexive".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("'s") {
            new_lemma = stripped;
            action = "addPronounReflexive".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("-hi") {
            new_lemma = stripped;
            action = "addPronounHi".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("-s'ho") {
            new_lemma = stripped;
            action = "addPronounReflexiveHo".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("-se-les") {
            new_lemma = stripped;
            action = "addPronounReflexiveLes".to_string();
        } else if let Some(stripped) = new_lemma.strip_suffix("-s'hi") {
            new_lemma = stripped;
            action = "addPronounReflexiveHi".to_string();
        }
        let mut postags: Vec<String> = Vec::new();
        let first_verb_index = verb_synthesizer.first_verb_index() as usize;
        for reading in tokens[first_verb_index].readings() {
            for postag in reading.pos_tags() {
                if !postag.starts_with('V') {
                    continue;
                }
                let with_number = apply_desired_number(postag, &desired_number);
                postags.push(apply_desired_persona(&with_number, &desired_persona));
            }
        }
        let target_postag = get_target_postag(&postags, "");
        if target_postag.is_empty() {
            return None;
        }
        let verb_str =
            verb_synthesizer.synthesize(new_lemma, &target_postag, services.synth, &|s| {
                adapt_suggestion_ca(s, "")
            });
        let mut pronouns_str = String::new();
        let is_pronouns_after =
            verb_synthesizer.num_pronouns_after() > 0 || !verb_synthesizer.is_first_verb_is();
        if verb_synthesizer.num_pronouns_before() > 0 {
            pronouns_str = verb_synthesizer.get_pronouns_str_before();
        } else if verb_synthesizer.num_pronouns_after() > 0 {
            pronouns_str = verb_synthesizer.get_pronouns_str_after();
        }
        pronouns_str = pronouns_str.to_lowercase();
        let first_verb_persona_number = if action == "addPronounDative" {
            verb_synthesizer.get_first_verb_persona_number()
        } else {
            substring(&target_postag, 4, 6)
        };
        let mut replacement = String::new();
        match action.as_str() {
            "addPronounEn" => {
                let new_pronoun = do_add_pronoun_en(
                    &pronouns_str,
                    &verb_str,
                    !verb_synthesizer.is_first_verb_is(),
                );
                if !new_pronoun.is_empty() {
                    replacement = if verb_synthesizer.is_first_verb_is() {
                        format!("{new_pronoun}{verb_str}")
                    } else {
                        format!("{verb_str}{new_pronoun}")
                    };
                }
            }
            "removePronounReflexive" => {
                replacement =
                    do_remove_pronoun_reflexive(&pronouns_str, &verb_str, is_pronouns_after)
            }
            "addPronounReflexiveEn" => {
                replacement = do_add_pronoun_reflexive_en(
                    &pronouns_str,
                    &verb_str,
                    &first_verb_persona_number,
                    is_pronouns_after,
                )
            }
            "replaceEmEn" => {
                replacement = do_replace_em_en(&pronouns_str, &verb_str, is_pronouns_after)
            }
            "addPronounReflexive" => {
                replacement = do_add_pronoun_reflexive(
                    &pronouns_str,
                    &verb_str,
                    &first_verb_persona_number,
                    is_pronouns_after,
                )
            }
            "addPronounReflexiveHi" => {
                replacement = do_add_pronoun_reflexive(
                    "",
                    &format!("hi {verb_str}"),
                    &first_verb_persona_number,
                    is_pronouns_after,
                )
            }
            "addPronounReflexiveLes" => {
                replacement = do_add_pronoun_reflexive(
                    &format!(
                        "{} les",
                        transform(&pronouns_str.to_lowercase(), PronounPosition::Normalized)
                    ),
                    &verb_str,
                    &first_verb_persona_number,
                    is_pronouns_after,
                )
            }
            "addPronounDative" => {
                let dative_pronoun = get_dative_pronoun(&first_verb_persona_number);
                if is_pronouns_after {
                    replacement =
                        format!("{verb_str}{}", transform_darrere(dative_pronoun, &verb_str));
                } else {
                    replacement =
                        format!("{}{verb_str}", transform_davant(dative_pronoun, &verb_str));
                }
            }
            "addPronounReflexiveHo" => {
                let mut reflexive_pronoun =
                    get_reflexive_pronoun(&first_verb_persona_number).to_string();
                if reflexive_pronoun.is_empty() && !pronouns_str.is_empty() {
                    let rp = transform(&pronouns_str, PronounPosition::Normalized);
                    if L_REFLEXIVE_PRONOUNS.contains(&rp.as_str()) {
                        reflexive_pronoun = rp;
                    }
                }
                if reflexive_pronoun.is_empty() {
                    reflexive_pronoun = "es".to_string();
                }
                let pronouns_normalized = format!("{reflexive_pronoun} ho");
                if is_pronouns_after {
                    replacement = format!(
                        "{verb_str}{}",
                        transform_darrere(&pronouns_normalized, &verb_str)
                    );
                } else {
                    replacement = format!(
                        "{}{verb_str}",
                        transform_davant(&pronouns_normalized, &verb_str)
                    );
                }
            }
            "addPronounHi" => {
                replacement = format!("hi {verb_str}");
            }
            "addPronounReflexiveImperative" => {
                replacement = do_add_pronoun_reflexive_imperative(
                    &pronouns_str,
                    &verb_str,
                    &first_verb_persona_number,
                )
            }
            "None" => {
                if is_pronouns_after {
                    replacement =
                        format!("{verb_str}{}", transform_darrere(&pronouns_str, &verb_str));
                } else {
                    replacement =
                        format!("{}{verb_str}", transform_davant(&pronouns_str, &verb_str));
                }
            }
            _ => {}
        }
        if !replacement.is_empty() {
            if make_intransitive {
                replacement = convert_pronouns_for_intransitive_verb(&replacement);
            }
            replacement = fix_apostrophes(&replacement);
            replacement = format!("{replacement} {after_lemma}").trim().to_string();
            replacements.push(preserve_case(
                &replacement,
                &verb_synthesizer.get_casing_model(),
            ));
        }
    }
    if replacements.is_empty() {
        return None;
    }
    let mut pos_start_underline =
        verb_synthesizer.first_verb_index() - verb_synthesizer.num_pronouns_before();
    if verb_synthesizer.num_pronouns_before() == 0 && pos_start_underline > 1 {
        let prev_token = tokens[pos_start_underline as usize - 1]
            .token()
            .token()
            .to_lowercase();
        if NEEDS_APOSTROPHE_CHANGE.contains(&prev_token.as_str())
            && any_change_vowel_consonant(&verb_synthesizer.get_verb_str(), &replacements)
        {
            let mut prefix = String::new();
            let prev2_token = tokens[pos_start_underline as usize - 2]
                .token()
                .token()
                .to_lowercase();
            if pos_start_underline > 2 && NEEDS_CONTRACTION_CHANGE.contains(&prev2_token.as_str()) {
                prefix.push_str(
                    &verb_synthesizer
                        .get_string_from_to(pos_start_underline - 2, pos_start_underline - 1)
                        .to_lowercase(),
                );
                if whitespace_before(tokens, pos_start_underline as usize) {
                    prefix.push(' ');
                }
                pos_start_underline -= 2;
            } else {
                prefix.push_str(&prev_token);
                if whitespace_before(tokens, pos_start_underline as usize) {
                    prefix.push(' ');
                }
                pos_start_underline -= 1;
            }
            for replacement in &mut replacements {
                *replacement = format!("{prefix}{replacement}");
            }
        }
    }
    let ending_pos = to_pos.max(tokens[verb_synthesizer.last_index() as usize].token().end());
    let mut out = rm.clone();
    let start = tokens[pos_start_underline as usize].token().start();
    out.set_offset_length(start, ending_pos.saturating_sub(start));
    let to_clamped = to_pos.min(sentence_text.len());
    let original_str = if start <= to_clamped && to_clamped <= sentence_text.len() {
        &sentence_text[start..to_clamped]
    } else {
        ""
    };
    out.set_replacements(
        replacements
            .iter()
            .map(|s| SuggestedReplacement::new(adapt_suggestion_ca(s, original_str)))
            .collect(),
    );
    Some(out)
}
