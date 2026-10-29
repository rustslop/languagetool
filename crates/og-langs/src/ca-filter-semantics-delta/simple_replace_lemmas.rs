use std::collections::HashMap;

use og_core::AnalyzedToken;

pub const SENTENCE_END_TAGNAME: &str = "SENT_END";

pub fn matched_lemma_and_postag<'a>(
    readings: &'a [AnalyzedToken],
    wrong_words: &HashMap<String, Vec<String>>,
) -> Option<(&'a str, Option<&'a str>)> {
    readings.iter().find_map(|at| {
        let postag = at.pos_tags().first().map(|p| p.as_str());
        let is_end_tag = postag == Some(SENTENCE_END_TAGNAME);
        let lemma = at.lemma()?;
        if !is_end_tag && wrong_words.contains_key(lemma) {
            Some((lemma, postag))
        } else {
            None
        }
    })
}
