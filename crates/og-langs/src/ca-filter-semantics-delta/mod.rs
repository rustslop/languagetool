pub mod adjust_pronouns;
pub mod adjust_verb_suggestions;
pub mod apostophation;
pub mod ca_adapt;
pub mod ca_synthesizer;
pub mod consumers;
pub mod convert_gender_number;
pub mod diacritics_check;
pub mod dispatch;
pub mod en_no_infinitiu;
pub mod oblidarse_suggestions;
pub mod portar_gerundi;
pub mod portar_temps;
pub mod possessius_redundants;
pub mod pronoms_actions;
pub mod pronoms_davant;
pub mod que_inici;
pub mod simple_replace_lemmas;
pub mod suppress_misspelled;
pub mod token_scan;
pub mod verb_classifier;
pub mod verb_synthesizer;

#[cfg(test)]
mod tests;
