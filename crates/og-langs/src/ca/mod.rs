pub mod compound_rule;
pub mod date_check_filter;
pub mod gv_chunker;
pub mod match_filter;
pub mod native_rules;
pub mod pronom_feble_duplicate;
pub mod pronoms_febles;
pub mod punctuation_paragraph_end;
pub mod remote_rules;
pub mod simple_replace;
pub mod tagger;
pub mod unpaired_brackets;
pub mod word_repeat_beginning;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

#[cfg(test)]
pub(crate) mod trace;

pub use tagger::CatalanTagger;

use og_core::AnalyzedSentence;
use og_core::rule::{Rule, TextLevelRule};
use std::sync::Arc;

pub struct CatalanNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub(crate) fn sentence_frame_bases(sentences: &[AnalyzedSentence]) -> Vec<usize> {
    let mut base = 0usize;
    sentences
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let b = if s.start() > 0 || i == 0 { 0 } else { base };
            if s.start() == 0 {
                base += s.text().len();
            }
            b
        })
        .collect()
}

pub fn catalan_unpaired_brackets_rule() -> unpaired_brackets::CatalanUnpairedBracketsRule {
    unpaired_brackets::CatalanUnpairedBracketsRule::new()
}

pub fn catalan_native_rules() -> CatalanNativeRules {
    CatalanNativeRules {
        rules: vec![
            Arc::new(native_rules::CatalanWordRepeatRule::new()),
            Arc::new(native_rules::CatalanWrongWordInContextRule::new()),
            Arc::new(simple_replace::catalan_simple_replace_verbs_rule()),
            Arc::new(simple_replace::catalan_simple_replace_balearic_rule()),
            Arc::new(simple_replace::catalan_simple_replace_rule()),
            Arc::new(simple_replace::catalan_simple_replace_multiwords_rule()),
            Arc::new(simple_replace::replace_operation_names_rule()),
            Arc::new(simple_replace::catalan_simple_replace_diacritics_iec_rule()),
            Arc::new(simple_replace::catalan_simple_replace_anglicism_rule()),
            Arc::new(pronom_feble_duplicate::PronomFebleDuplicateRule::new()),
            Arc::new(native_rules::CheckCaseRule::new()),
            Arc::new(simple_replace::catalan_simple_replace_adverbs_ment_rule()),
            Arc::new(compound_rule::CompoundRule::new()),
            Arc::new(simple_replace::catalan_simple_replace_dnv_rule()),
            Arc::new(simple_replace::catalan_simple_replace_dnv_colloquial_rule()),
            Arc::new(simple_replace::catalan_simple_replace_dnv_secondary_rule()),
        ],
        text_level_rules: vec![
            Arc::new(native_rules::CatalanUnpairedMarksRule::question()),
            Arc::new(native_rules::CatalanUnpairedMarksRule::exclamation()),
            Arc::new(word_repeat_beginning::CatalanWordRepeatBeginningRule::new()),
            Arc::new(native_rules::catalan_word_coherency_rule()),
            Arc::new(punctuation_paragraph_end::CatalanPunctuationMarkAtParagraphEnd::new()),
            Arc::new(remote_rules::CatalanRemoteRule::new()),
            Arc::new(remote_rules::CatalanSplitLongSentenceRule::new(60)),
            Arc::new(native_rules::IgnoreProperNouns),
        ],
    }
}

impl og_core::checker::WordTokenizer for word_tokenizer::CatalanWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<og_core::AnalyzedTokenReadings> {
        og_tokenizer::WordTokenizer::tokenize(self, text)
            .into_iter()
            .map(|token| {
                og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                    token.text(),
                    token.start() + offset,
                    token.end() + offset,
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rules_carry_unique_java_ids() {
        let native = catalan_native_rules();
        let mut ids: Vec<&str> = native.rules.iter().map(|r| r.id()).collect();
        ids.extend(native.text_level_rules.iter().map(|r| r.id()));
        let expected = [
            "CATALAN_WORD_REPEAT_RULE",
            "CA_UNPAIRED_QUESTION",
            "CATALAN_WRONG_WORD_IN_CONTEXT",
            "CA_SIMPLE_REPLACE_VERBS",
            "CA_SIMPLE_REPLACE_BALEARIC",
            "CA_SIMPLE_REPLACE_SIMPLE",
            "CA_SIMPLE_REPLACE_MULTIWORDS",
            "NOMS_OPERACIONS",
            "CA_SIMPLE_REPLACE_DIACRITICS_IEC",
            "CA_SIMPLE_REPLACE_ANGLICISM",
            "PRONOMS_FEBLES_DUPLICATS",
            "CA_CHECKCASE",
            "ADVERBIS_MENT",
            "CA_COMPOUNDS",
            "CA_SIMPLE_REPLACE_DNV",
            "CA_SIMPLE_REPLACE_DNV_COLLOQUIAL",
            "CA_SIMPLE_REPLACE_DNV_SECONDARY",
            "CATALAN_WORD_REPEAT_BEGINNING_RULE",
            "CA_UNPAIRED_EXCLAMATION",
            "CA_WORD_COHERENCY",
            "PUNCTUATION_PARAGRAPH_END",
            "CA_REMOTE_RULE",
            "CA_SPLIT_LONG_SENTENCE",
            "IGNORE_PROPER_NOUNS",
        ];
        for id in expected {
            assert!(ids.contains(&id), "missing native rule {id} in {ids:?}");
        }
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ids.len(),
            "no duplicate registrations: {ids:?}"
        );
    }

    #[test]
    fn native_rules_follow_the_java_relevant_rules_order() {
        let native = catalan_native_rules();
        let ids: Vec<&str> = native.rules.iter().map(|r| r.id()).collect();
        let pos = |id: &str| ids.iter().position(|x| *x == id).unwrap();
        assert!(
            pos("CA_SIMPLE_REPLACE_ANGLICISM") < pos("PRONOMS_FEBLES_DUPLICATS"),
            "Catalan.java:137 before :138"
        );
        assert!(
            pos("PRONOMS_FEBLES_DUPLICATS") < pos("CA_CHECKCASE"),
            "Catalan.java:138 before :139"
        );
        assert!(
            pos("ADVERBIS_MENT") < pos("CA_COMPOUNDS"),
            "Catalan.java:140 before :142"
        );
        assert!(
            pos("CA_COMPOUNDS") < pos("CA_SIMPLE_REPLACE_DNV"),
            "Catalan.java:142 before :144"
        );
        let text_ids: Vec<&str> = native.text_level_rules.iter().map(|r| r.id()).collect();
        let tpos = |id: &str| text_ids.iter().position(|x| *x == id).unwrap();
        assert!(
            tpos("CATALAN_WORD_REPEAT_BEGINNING_RULE") < tpos("CA_WORD_COHERENCY"),
            "Catalan.java:141 before :147"
        );
        assert!(
            tpos("CA_WORD_COHERENCY") < tpos("PUNCTUATION_PARAGRAPH_END"),
            "Catalan.java:147 before :148"
        );
        assert!(
            tpos("PUNCTUATION_PARAGRAPH_END") < tpos("CA_REMOTE_RULE"),
            "Catalan.java:148 before :149"
        );
        assert!(
            tpos("CA_REMOTE_RULE") < tpos("CA_SPLIT_LONG_SENTENCE"),
            "Catalan.java:149 before :150"
        );
        assert_eq!(text_ids.last(), Some(&"IGNORE_PROPER_NOUNS"));
    }

    #[test]
    fn core_tokenizer_impl_shifts_offsets() {
        use og_core::checker::WordTokenizer as _;
        let tokenizer = word_tokenizer::CatalanWordTokenizer::new();
        let tokens = tokenizer.tokenize("bonic", 7);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token().start(), 7);
        assert_eq!(tokens[0].token().end(), 12);
    }

    #[test]
    fn frame_bases_accumulate_only_for_local_frames() {
        let s1 = AnalyzedSentence::new("aa", 0, 2);
        let s2 = AnalyzedSentence::new("bb", 0, 2);
        assert_eq!(sentence_frame_bases(&[s1, s2]), vec![0, 2]);
        let s1 = AnalyzedSentence::new("aa", 0, 2);
        let s2 = AnalyzedSentence::new("bb", 5, 7);
        assert_eq!(sentence_frame_bases(&[s1, s2]), vec![0, 0]);
    }
}
