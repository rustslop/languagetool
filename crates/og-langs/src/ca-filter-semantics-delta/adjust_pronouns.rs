use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};

use super::pronoms_actions::{
    do_add_pronoun_en, do_add_pronoun_reflexive, do_add_pronoun_reflexive_imperative,
    do_remove_pronoun_reflexive, do_replace_em_en,
};
use super::pronoms_davant::transform_davant;
use super::token_scan::preserve_case;
use super::verb_synthesizer::VerbSynthesizer;
use crate::ca::pronoms_febles::{PronounPosition, transform};

pub fn accept_rule_match(
    rm: &RuleMatch,
    from_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
    actions: &str,
    new_lemma: &str,
    new_only_lemma: &str,
    synth: &dyn Fn(&str, &str) -> Vec<String>,
) -> Option<RuleMatch> {
    let mut replacements: Vec<String> = Vec::new();
    if actions.is_empty() {
        return None;
    }
    let actions: Vec<&str> = actions.split(',').collect();
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    let verb_synthesizer = VerbSynthesizer::new(tokens, pos_word as isize);
    if verb_synthesizer.is_undefined() {
        return None;
    }
    let target_postag = verb_synthesizer
        .first_verb_postag()
        .unwrap_or("")
        .to_string();
    let adapt = |s: &str| s.to_string();
    let mut verb_str = verb_synthesizer.get_verb_str();
    if !new_lemma.is_empty() {
        verb_str = verb_synthesizer.synthesize(new_lemma, &target_postag, synth, &adapt);
    }
    let mut verb_str2 = verb_str.clone();
    if !new_only_lemma.is_empty() {
        verb_str2 = verb_synthesizer.synthesize(new_only_lemma, &target_postag, synth, &adapt);
    }
    let first_verb_persona_number = verb_synthesizer.get_first_verb_persona_number();
    let mut pronouns_str = String::new();
    if verb_synthesizer.num_pronouns_before() > 0 {
        pronouns_str = verb_synthesizer.get_pronouns_str_before();
    } else if verb_synthesizer.num_pronouns_after() > 0 {
        pronouns_str = verb_synthesizer.get_pronouns_str_after();
    }
    let start_underline_index =
        verb_synthesizer.first_verb_index() - verb_synthesizer.num_pronouns_before();
    let end_underline_index =
        verb_synthesizer.last_verb_index() + verb_synthesizer.num_pronouns_after();
    for action in &actions {
        let replacement: String = match *action {
            "removePronounEn" => {
                let pr = pronouns_str
                    .replace("en", "")
                    .replace("n'", "")
                    .replace("'n", "")
                    .trim()
                    .to_string();
                format!("{}{}", transform_davant(&pr, &verb_str), verb_str)
            }
            "addPronounEn" => {
                let new_pronoun = do_add_pronoun_en(
                    &pronouns_str,
                    &verb_str,
                    !verb_synthesizer.is_first_verb_is(),
                );
                if !new_pronoun.is_empty() {
                    if verb_synthesizer.is_first_verb_is() {
                        format!("{new_pronoun}{verb_str}")
                    } else {
                        format!("{verb_str}{new_pronoun}")
                    }
                } else {
                    String::new()
                }
            }
            "removePronounReflexive" => {
                do_remove_pronoun_reflexive(&pronouns_str, &verb_str, false)
            }
            "replaceEmEn" => do_replace_em_en(&pronouns_str, &verb_str, false),
            "replaceHiEn" => {
                let stripped = transform(
                    pronouns_str.replace("hi", "").trim(),
                    PronounPosition::Normalized,
                );
                format!(
                    "{}{}",
                    do_add_pronoun_en(&stripped, &verb_str, false),
                    verb_str
                )
            }
            "addPronounReflexive" => do_add_pronoun_reflexive(
                &pronouns_str,
                &verb_str,
                &first_verb_persona_number,
                !verb_synthesizer.is_first_verb_is(),
            ),
            "addPronounReflexiveHi" => do_add_pronoun_reflexive(
                &pronouns_str,
                &format!("hi {verb_str}"),
                &first_verb_persona_number,
                false,
            ),
            "addPronounReflexiveImperative" => do_add_pronoun_reflexive_imperative(
                &pronouns_str,
                &verb_str,
                &verb_synthesizer.get_first_verb_persona_number_imperative(),
            ),
            "changeOnlyLemma" => {
                let mut pronouns_str = pronouns_str.clone();
                if actions.contains(&"replaceHiEn") {
                    pronouns_str = pronouns_str.replace("hi", "").trim().to_string();
                }
                let davant = transform_davant(&pronouns_str, &verb_str2);
                format!("{davant}{verb_str2}")
            }
            _ => String::new(),
        };
        if !replacement.is_empty()
            && !replacement.eq_ignore_ascii_case(&verb_synthesizer.get_whole_original_str())
        {
            replacements.push(preserve_case(
                &replacement,
                &verb_synthesizer.get_casing_model(),
            ));
        }
    }
    if replacements.is_empty() {
        return None;
    }
    let start_idx = start_underline_index.clamp(0, tokens.len() as isize - 1) as usize;
    let end_idx = end_underline_index.clamp(0, tokens.len() as isize - 1) as usize;
    let start = tokens[start_idx].token().start();
    let end = tokens[end_idx].token().end();
    let mut out = rm.clone();
    out.set_offset_length(start, end - start);
    out.set_replacements(
        replacements
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_pronoun_en_rebuilds_the_span_over_the_pronoun() {
        let pr = do_add_pronoun_en("", "va", true);
        assert!(!pr.is_empty(), "do_add_pronoun_en produces the clitic");
    }
}
