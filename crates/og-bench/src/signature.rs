//! Match signatures: the parts of a check result the parity goldens pin.
//!
//! The JSON is written by hand (the crate is std only) in a fixed layout,
//! one match per line, so goldens compare byte-for-byte and diff by line.

use crate::Corpus;
use og_core::RuleMatch;
use std::fmt::Write as _;
use std::path::PathBuf;

/// Setting this to `1` makes `tests/parity.rs` rewrite the goldens.
pub const UPDATE_GOLDEN_ENV: &str = "UPDATE_GOLDEN";

/// Where the golden for a corpus lives: `crates/og-bench/golden/<name>.json`.
pub fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("golden")
        .join(format!("{name}.json"))
}

/// Renders the signature of `matches`, the engine's output on `corpus`.
///
/// Per match, in result order: rule id, byte offset, length, the covered
/// text (derived from offset and length, for review), message and the full
/// ordered replacement list. `en_ngram_data` records whether the English
/// n-gram data was loaded, because the n-gram rules only exist with it: a
/// golden generated with the data cannot pass on a host without it, and
/// this line says so instead of a wall of missing matches.
pub fn signature_json(corpus: &Corpus, en_ngram_data: bool, matches: &[RuleMatch]) -> String {
    let mut out = String::with_capacity(256 + 192 * matches.len());
    out.push_str("{\n  \"corpus\": ");
    push_json_string(&mut out, corpus.name);
    out.push_str(",\n  \"text\": ");
    push_json_string(&mut out, corpus.text);
    let _ = write!(
        out,
        ",\n  \"en_ngram_data\": {en_ngram_data},\n  \"matches\": ["
    );
    for (index, m) in matches.iter().enumerate() {
        out.push_str(if index == 0 { "\n    " } else { ",\n    " });
        out.push_str("{\"rule_id\": ");
        push_json_string(&mut out, m.rule().id());
        let _ = write!(
            out,
            ", \"offset\": {}, \"length\": {}, \"covered\": ",
            m.offset(),
            m.length()
        );
        match corpus.text.get(m.offset()..m.offset() + m.length()) {
            Some(covered) => push_json_string(&mut out, covered),
            None => out.push_str("null"),
        }
        out.push_str(", \"message\": ");
        push_json_string(&mut out, m.message());
        out.push_str(", \"replacements\": [");
        for (position, replacement) in m.replacements().iter().enumerate() {
            if position > 0 {
                out.push_str(", ");
            }
            push_json_string(&mut out, replacement.value());
        }
        out.push_str("]}");
    }
    if !matches.is_empty() {
        out.push_str("\n  ");
    }
    out.push_str("]\n}\n");
    out
}

/// Appends `value` as a JSON string literal.
pub fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The first line where `actual` departs from `expected`, as
/// `(1-based line, expected line, actual line)`; `None` when identical.
pub fn first_difference(expected: &str, actual: &str) -> Option<(usize, String, String)> {
    if expected == actual {
        return None;
    }
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    let mut line = 1;
    loop {
        match (expected_lines.next(), actual_lines.next()) {
            (Some(want), Some(got)) if want == got => line += 1,
            (None, None) => {
                return Some((
                    line,
                    "<line endings differ>".to_string(),
                    "<line endings differ>".to_string(),
                ));
            }
            (want, got) => {
                return Some((
                    line,
                    want.unwrap_or("<end of file>").to_string(),
                    got.unwrap_or("<end of file>").to_string(),
                ));
            }
        }
    }
}

/// One-line verdict of `matches` against the committed golden, for the
/// binary's stderr: a timing is only comparable if the behavior is too.
pub fn golden_status(corpus: &Corpus, en_ngram_data: bool, matches: &[RuleMatch]) -> String {
    let path = golden_path(corpus.name);
    let actual = signature_json(corpus, en_ngram_data, matches);
    match std::fs::read_to_string(&path) {
        Ok(expected) => match first_difference(&expected, &actual) {
            None => "output matches its golden".to_string(),
            Some((line, ..)) => format!(
                "output DIFFERS from {} at line {line} \
                 (cargo test -p og-bench --test parity shows the diff)",
                path.display()
            ),
        },
        Err(err) => format!("golden {} unreadable: {err}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{RuleMatchContext, RuleMatchRule, SuggestedReplacement};

    const CORPUS: Corpus = Corpus {
        name: "t",
        text: "Teh cat sat.",
    };

    fn rule_match(id: &str, offset: usize, length: usize, replacements: &[&str]) -> RuleMatch {
        RuleMatch::new(
            format!("Message with \"quotes\" for {id}"),
            offset,
            length,
            RuleMatchRule::new(id, "description"),
            RuleMatchContext::new(CORPUS.text, 0, CORPUS.text.len()),
        )
        .with_replacements(
            replacements
                .iter()
                .map(|value| SuggestedReplacement::new(*value))
                .collect(),
        )
    }

    #[test]
    fn signature_layout_is_one_match_per_line() {
        let matches = [
            rule_match("SPELL", 0, 3, &["The", "Tea"]),
            rule_match("OUT_OF_RANGE", 10, 9, &[]),
        ];
        assert_eq!(
            signature_json(&CORPUS, true, &matches),
            "{\n  \"corpus\": \"t\",\n  \"text\": \"Teh cat sat.\",\n  \"en_ngram_data\": true,\n  \
             \"matches\": [\n    \
             {\"rule_id\": \"SPELL\", \"offset\": 0, \"length\": 3, \"covered\": \"Teh\", \
             \"message\": \"Message with \\\"quotes\\\" for SPELL\", \
             \"replacements\": [\"The\", \"Tea\"]},\n    \
             {\"rule_id\": \"OUT_OF_RANGE\", \"offset\": 10, \"length\": 9, \"covered\": null, \
             \"message\": \"Message with \\\"quotes\\\" for OUT_OF_RANGE\", \
             \"replacements\": []}\n  ]\n}\n"
        );
    }

    #[test]
    fn signature_without_matches_has_an_empty_list() {
        assert_eq!(
            signature_json(&CORPUS, false, &[]),
            "{\n  \"corpus\": \"t\",\n  \"text\": \"Teh cat sat.\",\n  \
             \"en_ngram_data\": false,\n  \"matches\": []\n}\n"
        );
    }

    #[test]
    fn json_strings_escape_quotes_backslashes_and_controls() {
        let mut out = String::new();
        push_json_string(&mut out, "a\"b\\c\nd\te\u{1}f\u{8}\u{c}\r’");
        assert_eq!(out, "\"a\\\"b\\\\c\\nd\\te\\u0001f\\b\\f\\r’\"");
    }

    #[test]
    fn first_difference_reports_the_first_diverging_line() {
        assert_eq!(first_difference("a\nb\n", "a\nb\n"), None);
        assert_eq!(
            first_difference("a\nb\nc\n", "a\nx\nc\n"),
            Some((2, "b".to_string(), "x".to_string()))
        );
        assert_eq!(
            first_difference("a\n", "a\nb\n"),
            Some((2, "<end of file>".to_string(), "b".to_string()))
        );
        assert_eq!(
            first_difference("a\n", "a"),
            Some((
                2,
                "<line endings differ>".to_string(),
                "<line endings differ>".to_string()
            ))
        );
    }

    #[test]
    fn goldens_live_in_the_crate() {
        let path = golden_path("err");
        assert!(path.ends_with("og-bench/golden/err.json"), "{path:?}");
    }
}
