use std::sync::LazyLock;

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::pronoms_davant::transform_davant;
use super::token_scan::{preserve_case, reading_and_tag_matching, substring, whitespace_before};
use super::verb_synthesizer::VerbSynthesizer;

static P_VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.*$").unwrap());
static P_GERUND: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.G.*$").unwrap());
static P_INFINITIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.N.*$").unwrap());

fn has_chunk(token: &AnalyzedTokenReadings, tag: &str) -> bool {
    token.chunks().iter().any(|c| c == tag)
}

fn has_pos_starting(token: &AnalyzedTokenReadings, prefix: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
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
    let verb_postag = reading_and_tag_matching(tokens.get(pos_word)?, &P_VERB)?
        .1
        .to_string();
    let new_postag = format!(
        "{}[30][S0].{}",
        substring(&verb_postag, 0, 4),
        substring(&verb_postag, 7, 8)
    );
    let mut suggestion = String::new();
    let synth_forms = synth("fer", &new_postag);
    let first = synth_forms.first()?.clone();
    suggestion.push_str(&first);
    let mut i = pos_word + 1;
    while i < tokens.len() && has_chunk(tokens[i], "PTime") {
        if whitespace_before(tokens, i) {
            suggestion.push(' ');
        }
        suggestion.push_str(tokens[i].token().token());
        i += 1;
    }
    let last_token_pos = i;
    if last_token_pos + 1 >= tokens.len() {
        return None;
    }
    let mut adjust_end_pos: isize = 0;
    let last_token = tokens[last_token_pos];
    let last_text = last_token.token().token();
    if last_text == "que" {
        suggestion.push_str(" que");
    } else if has_pos_starting(last_token, "VMG") || has_pos_starting(last_token, "VSG") {
        suggestion.push_str(" que ");
        let verb_synth = VerbSynthesizer::new(tokens, last_token_pos as isize);
        let pronoms = verb_synth.get_pronouns_str_after();
        adjust_end_pos += verb_synth.num_pronouns_after() as isize;
        let lemma = reading_and_tag_matching(last_token, &P_GERUND)?
            .0
            .lemma()?
            .to_string();
        let postag2 = format!("V.I{}", substring(&verb_postag, 3, 8));
        let forms2 = synth(&lemma, &postag2);
        let form2 = forms2.first()?.clone();
        if !pronoms.is_empty() {
            suggestion.push_str(&transform_davant(&pronoms, &form2));
        }
        suggestion.push_str(&form2);
    } else if last_text == "sense"
        && (has_pos_starting(tokens[last_token_pos + 1], "VSN")
            || has_pos_starting(tokens[last_token_pos + 1], "VMN"))
    {
        suggestion.push_str(" que no ");
        adjust_end_pos += 1;
        let verb_synth = VerbSynthesizer::new(tokens, (last_token_pos + 1) as isize);
        let pronoms = verb_synth.get_pronouns_str_after();
        adjust_end_pos += verb_synth.num_pronouns_after() as isize;
        let lemma = reading_and_tag_matching(tokens[last_token_pos + 1], &P_INFINITIVE)?
            .0
            .lemma()?
            .to_string();
        let postag2 = format!("V.I{}", substring(&verb_postag, 3, 8));
        let forms2 = synth(&lemma, &postag2);
        let form2 = forms2.first()?.clone();
        if !pronoms.is_empty() {
            suggestion.push_str(&transform_davant(&pronoms, &form2));
        }
        suggestion.push_str(&form2);
    } else if [
        "així", "a", "en", "ací", "aquí", "ahí", "allí", "allà", "de",
    ]
    .contains(&last_text)
        || has_pos_starting(last_token, "AQ")
        || has_pos_starting(last_token, "VMP")
    {
        let postag2 = format!("V.I{}", substring(&verb_postag, 3, 8));
        let forms2 = synth("estar", &postag2);
        let form2 = forms2.first()?.clone();
        suggestion.push_str(&format!(" que {form2}"));
        adjust_end_pos -= 1;
    } else {
        return None;
    }
    let replacement = preserve_case(&suggestion, tokens[pos_word].token().token());
    if replacement.is_empty() {
        return None;
    }
    let end_idx = (last_token_pos as isize + adjust_end_pos).clamp(0, tokens.len() as isize - 1);
    let start = tokens[pos_word].token().start();
    let end = tokens[end_idx as usize].token().end();
    let mut out = rm.clone();
    out.set_offset_length(start, end - start);
    out.set_replacements(vec![SuggestedReplacement::new(replacement)]);
    Some(out)
}
