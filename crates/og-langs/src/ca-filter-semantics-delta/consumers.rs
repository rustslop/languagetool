use super::apostophation::needs_apostrophation_ms;

pub const NEEDS_APOSTROPHE_CHANGE: [&str; 5] = ["de", "d'", "l", "l'", "el"];
pub const NEEDS_CONTRACTION_CHANGE: [&str; 4] = ["a", "de", "per", "pe"];

pub fn any_change_vowel_consonant(original_verb: &str, replacements: &[String]) -> bool {
    let original_verb_needs_apostrophe = needs_apostrophation_ms(original_verb);
    replacements
        .iter()
        .any(|replacement| needs_apostrophation_ms(replacement) != original_verb_needs_apostrophe)
}

pub fn find_suggestions_es_verb3_allowed(verb_token: &str) -> bool {
    !needs_apostrophation_ms(verb_token)
}

pub fn oblidarse_word_after_suffix(word_after: &str) -> (&'static str, usize) {
    if needs_apostrophation_ms(word_after) {
        (" d'", 1)
    } else {
        (" de", 0)
    }
}
