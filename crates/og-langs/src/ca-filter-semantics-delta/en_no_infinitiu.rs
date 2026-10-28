use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};

use super::pronoms_davant::transform_davant;
use super::token_scan::{eq_ignore_case, preserve_case};
use super::verb_synthesizer::VerbSynthesizer;

const TEMPS_VERBALS_PRESENT: [&str; 2] = ["IP", "IF"];

pub fn accept_rule_match(
    rm: &RuleMatch,
    sentence_text: &str,
    from_pos: usize,
    to_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
    synth: &dyn Fn(&str, &str) -> Vec<String>,
    adapt: &dyn Fn(&str) -> String,
) -> Option<RuleMatch> {
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    let verb_synth_infinitiu = VerbSynthesizer::new(tokens, pos_word as isize);
    if verb_synth_infinitiu.is_undefined()
        || tokens[verb_synth_infinitiu.last_verb_index() as usize]
            .token()
            .end()
            > to_pos
    {
        return None;
    }
    let verb_after = VerbSynthesizer::new(tokens, verb_synth_infinitiu.last_index() + 1);
    let verb_before = VerbSynthesizer::with_search_backward(
        tokens,
        verb_synth_infinitiu.first_verb_index() - 1,
        true,
    );
    let verb_after_postag = verb_after.get_first_verb_is_postag();
    let verb_before_postag = verb_before.get_first_verb_is_postag();
    let before_defined = !verb_before.is_undefined() && verb_before_postag.is_some();
    let (postag_temps_verbal, is_passat_perifrastic) =
        if !verb_after.is_undefined() && verb_after_postag.is_some() {
            (
                verb_after_postag.clone().unwrap_or_default(),
                verb_after.is_passat_perifrastic(),
            )
        } else if before_defined {
            (
                verb_before_postag.clone().unwrap_or_default(),
                verb_before.is_passat_perifrastic(),
            )
        } else {
            return None;
        };
    let postag_chars: Vec<char> = postag_temps_verbal.chars().collect();
    if postag_chars.len() < 6 {
        return None;
    }
    let temps: String = postag_chars[2..4].iter().collect();
    let person_number: String = postag_chars[4..6].iter().collect();
    let tail: String = postag_chars[6..].iter().collect();
    let postag_prefix = if TEMPS_VERBALS_PRESENT.contains(&temps.as_str()) && !is_passat_perifrastic
    {
        "VMIP"
    } else {
        "VMII"
    };
    let mut synth_verbs: Vec<String> = Vec::new();
    if person_number != "3S" {
        let postag = format!("{postag_prefix}3S{tail}");
        synth_verbs.push(verb_synth_infinitiu.synthesize(
            verb_synth_infinitiu.last_verb_lemma().unwrap_or(""),
            &postag,
            synth,
            adapt,
        ));
    }
    let postag = format!("{postag_prefix}{person_number}{tail}");
    synth_verbs.push(verb_synth_infinitiu.synthesize(
        verb_synth_infinitiu.last_verb_lemma().unwrap_or(""),
        &postag,
        synth,
        adapt,
    ));
    let mut start_pos = verb_synth_infinitiu.first_verb_index() - 2;
    if start_pos < 0 {
        start_pos = 0;
    } else if eq_ignore_case(tokens[start_pos as usize].token().token(), "l") {
        start_pos -= 1;
    }
    let end_pos = verb_synth_infinitiu.last_index();
    let original_str = &sentence_text
        [tokens[start_pos as usize].token().start()..tokens[end_pos as usize].token().end()];
    let mut suggestions: Vec<String> = Vec::new();
    for synth_verb in &synth_verbs {
        let mut suggestion = String::new();
        if before_defined {
            suggestion.push_str("perquè no ");
        } else {
            suggestion.push_str("com que no ");
        }
        let pronouns_after = verb_synth_infinitiu.get_pronouns_str_after();
        if !pronouns_after.is_empty() {
            suggestion.push_str(&transform_davant(&pronouns_after, synth_verb));
        }
        suggestion.push_str(synth_verb);
        let suggestion_str = preserve_case(&suggestion, original_str);
        if !suggestions.contains(&suggestion_str) {
            suggestions.push(suggestion_str);
        }
    }
    if suggestions.is_empty() {
        return None;
    }
    let mut out = rm.clone();
    out.set_offset_length(
        tokens[start_pos as usize].token().start(),
        tokens[end_pos as usize].token().end() - tokens[start_pos as usize].token().start(),
    );
    out.set_replacements(
        suggestions
            .iter()
            .map(|s| SuggestedReplacement::new(s.clone()))
            .collect(),
    );
    Some(out)
}
