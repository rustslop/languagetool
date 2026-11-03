use og_core::{AnalyzedSentence, AnalyzedTokenReadings, RuleMatch};
use std::collections::HashMap;

pub struct LanguageFilterCall<'a> {
    pub class: &'a str,
    pub args: &'a HashMap<&'a str, String>,
    pub matched_texts: &'a [String],
    pub matched_positions: &'a [usize],
    pub element_positions: &'a [usize],
    pub tokens: &'a [&'a AnalyzedTokenReadings],
    pub sentence: &'a AnalyzedSentence,
}

pub trait LanguageFilterHook: Send + Sync {
    fn apply_filter(&self, call: LanguageFilterCall<'_>, rm: RuleMatch) -> Option<RuleMatch>;
}
