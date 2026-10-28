use std::sync::LazyLock;

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::pronoms_davant::transform_davant;
use super::token_scan::{preserve_case, reading_and_tag_matching};
use super::verb_synthesizer::VerbSynthesizer;

static VERB_IS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[IS].*$").unwrap());
static VERB_GERUND: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.G.*$").unwrap());

pub fn accept_rule_match(
    rm: &RuleMatch,
    from_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
    new_lemma: &str,
    synth: &dyn Fn(&str, &str) -> Vec<String>,
) -> Option<RuleMatch> {
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    let (_, atr1_tag) = reading_and_tag_matching(tokens.get(pos_word)?, &VERB_IS)?;
    let next = tokens.get(pos_word + 1)?;
    let atr2 = reading_and_tag_matching(next, &VERB_GERUND)?;
    let lemma = if new_lemma.is_empty() {
        atr2.0.lemma().unwrap_or("").to_string()
    } else {
        new_lemma.to_string()
    };
    let mut replacements: Vec<String> = Vec::new();
    let synth_forms1 = synth("haver", &format!("VA{}", skip_chars(atr1_tag, 2)));
    let synth_forms2 = synth(&lemma, "V.P..SM.");
    if !synth_forms1.is_empty() && !synth_forms2.is_empty() {
        for form1 in &synth_forms1 {
            for form2 in &synth_forms2 {
                replacements.push(format!("{form1} {form2}"));
            }
        }
    }
    let synth_forms3 = synth(&lemma, &format!("V.{}", skip_chars(atr1_tag, 2)));
    if let Some(first) = synth_forms3.first() {
        replacements.push(first.clone());
    }
    if replacements.is_empty() {
        return None;
    }
    let verb_synthesizer = VerbSynthesizer::new(tokens, pos_word as isize);
    let mut correct_start_index: isize = 0;
    let mut correct_end_index: isize = 0;
    for replacement in &mut replacements {
        let mut pronouns_suggestion = String::new();
        if verb_synthesizer.num_pronouns_after() > 0 {
            pronouns_suggestion =
                transform_davant(&verb_synthesizer.get_pronouns_str_after(), replacement);
            correct_end_index = verb_synthesizer.num_pronouns_after();
        } else if verb_synthesizer.num_pronouns_before() > 0 {
            pronouns_suggestion =
                transform_davant(&verb_synthesizer.get_pronouns_str_before(), replacement);
            correct_start_index = -verb_synthesizer.num_pronouns_before();
        }
        let model = tokens[(pos_word as isize + correct_start_index).max(0) as usize]
            .token()
            .token();
        *replacement = preserve_case(&format!("{pronouns_suggestion}{replacement}"), model);
    }
    let mut out = rm.clone();
    let start = tokens[(pos_word as isize + correct_start_index).max(0) as usize]
        .token()
        .start();
    let end_idx = (pos_word as isize + 1 + correct_end_index).clamp(0, tokens.len() as isize - 1);
    out.set_offset_length(start, tokens[end_idx as usize].token().end() - start);
    out.set_replacements(
        replacements
            .iter()
            .map(|r| SuggestedReplacement::new(r.clone()))
            .collect(),
    );
    Some(out)
}

fn skip_chars(s: &str, n: usize) -> String {
    s.chars().skip(n).collect()
}
