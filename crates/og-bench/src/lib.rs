//! The perf campaign's measurement harness for the English engine.
//!
//! The `og-bench` binary times a fixed workload and prints exactly one JSON
//! line on stdout; `tests/parity.rs` pins the engine's output on the same
//! corpora against `golden/<corpus>.json`. The crate is std only: its only
//! dependencies are the workspace crates under measurement.

pub mod measure;
pub mod ngram;
pub mod signature;

pub use measure::*;
pub use ngram::*;
pub use signature::*;

/// Two grammatically correct sentences: the cost of a check that finds
/// nothing. "Their" is used correctly, so the confusion rules still run.
pub const CLEAN_TEXT: &str = "The children walked to school together this morning. \
Their teacher was waiting for them at the gate.";

/// Dense errors: misspellings that need speller suggestions, a their/there
/// confusion and agreement errors.
pub const ERR_TEXT: &str = "Their is a probleme with this sentense. \
Teh guick brown fox jumpd over the lazy dogg. \
I has tree cats and they runs fast.";

/// A twelve-sentence paragraph: seven clean sentences, two misspellings
/// ("wonderfull", "colected") and three their/there/they're confusions.
pub const MULTI_TEXT: &str = "Last summer we drove to the coast for a long weekend. \
The weather was warm and the sea was calm. \
Their was a small restaurant near the harbor that served fresh fish. \
We ordered the soup, and it tasted wonderfull. \
My brother wanted to go swimming, but there towels were still in the car. \
In the afternoon we walked along the beach and colected shells. \
The children built a sandcastle with a tall tower. \
Later, a sudden storm forced us to run back to the hotel. \
I think there going to remember that trip for years. \
The next morning the sky was clear again. \
We packed our bags and drove home before lunch. \
It was the best holiday we have had in a long time.";

/// A named, fixed input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Corpus {
    pub name: &'static str,
    pub text: &'static str,
}

/// The bench corpora, in the order the metrics line reports them.
pub static CORPORA: [Corpus; 3] = [
    Corpus {
        name: "clean",
        text: CLEAN_TEXT,
    },
    Corpus {
        name: "err",
        text: ERR_TEXT,
    },
    Corpus {
        name: "multi",
        text: MULTI_TEXT,
    },
];

/// Looks a corpus up by name.
pub fn corpus(name: &str) -> Option<&'static Corpus> {
    CORPORA.iter().find(|corpus| corpus.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence_count(text: &str) -> usize {
        text.split_terminator(". ").count()
    }

    #[test]
    fn corpora_have_the_specified_shape() {
        assert_eq!(
            ERR_TEXT,
            "Their is a probleme with this sentense. Teh guick brown fox jumpd over the lazy \
             dogg. I has tree cats and they runs fast."
        );
        assert_eq!(sentence_count(CLEAN_TEXT), 2);
        assert_eq!(sentence_count(ERR_TEXT), 3);
        assert_eq!(sentence_count(MULTI_TEXT), 12);
        for corpus in &CORPORA {
            assert!(corpus.text.is_ascii(), "{}: byte offsets equal char offsets", corpus.name);
            assert!(!corpus.text.contains("  "), "{}: single spaces only", corpus.name);
        }
    }

    #[test]
    fn corpora_are_found_by_name() {
        assert_eq!(corpus("err").map(|c| c.text), Some(ERR_TEXT));
        assert_eq!(
            CORPORA.map(|c| c.name),
            ["clean", "err", "multi"],
            "metrics line order"
        );
        assert!(corpus("missing").is_none());
    }
}
