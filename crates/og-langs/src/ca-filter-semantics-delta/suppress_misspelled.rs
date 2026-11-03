pub struct FullAnalysis {
    pub has_incorrect_verb_chunk: bool,
    pub speller_flags: bool,
}

pub fn is_misspelled(
    s: &str,
    raw_speller_flags: &dyn Fn(&str) -> bool,
    full_analysis: &dyn Fn(&str) -> FullAnalysis,
) -> bool {
    if !s.contains(' ') && raw_speller_flags(s) {
        return true;
    }
    let analysis = full_analysis(s);
    analysis.has_incorrect_verb_chunk || analysis.speller_flags
}
