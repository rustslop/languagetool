use super::apostophation::needs_apostrophation_ms;
use crate::ca::pronoms_febles::{PronounPosition, transform};

pub fn transform_davant(input_pronom: &str, next_word: &str) -> String {
    let next_lower = next_word.to_lowercase();
    if needs_apostrophation_ms(&next_lower) {
        return transform(input_pronom, PronounPosition::DavantApos);
    }
    let pronom = transform(input_pronom, PronounPosition::Davant);
    if pronom == "es "
        && (next_lower.starts_with('s')
            || next_lower.starts_with("ce")
            || next_lower.starts_with("ci"))
    {
        return "se ".to_string();
    }
    pronom
}
