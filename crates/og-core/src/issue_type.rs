use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum IssueType {
    #[serde(rename = "grammar")]
    #[default]
    Grammar,
    #[serde(rename = "spelling")]
    Spelling,
    #[serde(rename = "typographical")]
    Typographical,
    #[serde(rename = "style")]
    Style,
    #[serde(rename = "whitespace")]
    Whitespace,
    #[serde(rename = "punctuation")]
    Punctuation,
    #[serde(rename = "capitalization")]
    Capitalization,
    #[serde(rename = "misspelling")]
    Misspelling,
    #[serde(rename = "other")]
    Other(String),
    #[serde(rename = "semantic")]
    Semantic,
    #[serde(rename = "redundancy")]
    Redundancy,
    #[serde(rename = "structure")]
    Structure,
    #[serde(rename = "locale-violation")]
    LocaleViolation,
    #[serde(rename = "inconsistency")]
    Inconsistency,
    #[serde(rename = "uncategorized")]
    Uncategorized,
    #[serde(rename = "non-conformance")]
    NonConformance,
    #[serde(rename = "compounding")]
    Compounding,
    #[serde(rename = "colloquialism")]
    Colloquialism,
    #[serde(rename = "false-friend")]
    FalseFriend,
    #[serde(rename = "readability")]
    Readability,
    #[serde(rename = "de")]
    De,
}

impl IssueType {
    pub fn as_str(&self) -> &str {
        match self {
            IssueType::Grammar => "grammar",
            IssueType::Spelling => "spelling",
            IssueType::Typographical => "typographical",
            IssueType::Style => "style",
            IssueType::Whitespace => "whitespace",
            IssueType::Punctuation => "punctuation",
            IssueType::Capitalization => "capitalization",
            IssueType::Misspelling => "misspelling",
            IssueType::Semantic => "semantic",
            IssueType::Redundancy => "redundancy",
            IssueType::Structure => "structure",
            IssueType::LocaleViolation => "locale-violation",
            IssueType::Inconsistency => "inconsistency",
            IssueType::Uncategorized => "uncategorized",
            IssueType::NonConformance => "non-conformance",
            IssueType::Compounding => "compounding",
            IssueType::Colloquialism => "colloquialism",
            IssueType::FalseFriend => "false-friend",
            IssueType::Readability => "readability",
            IssueType::De => "de",
            IssueType::Other(s) => s.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_grammar() {
        assert_eq!(IssueType::default(), IssueType::Grammar);
    }

    #[test]
    fn as_str_matches_java_issue_type_names() {
        let cases = [
            (IssueType::Grammar, "grammar"),
            (IssueType::Spelling, "spelling"),
            (IssueType::Typographical, "typographical"),
            (IssueType::Style, "style"),
            (IssueType::Whitespace, "whitespace"),
            (IssueType::Punctuation, "punctuation"),
            (IssueType::Capitalization, "capitalization"),
            (IssueType::Misspelling, "misspelling"),
            (IssueType::Semantic, "semantic"),
            (IssueType::Redundancy, "redundancy"),
            (IssueType::Structure, "structure"),
            (IssueType::LocaleViolation, "locale-violation"),
            (IssueType::Inconsistency, "inconsistency"),
            (IssueType::Uncategorized, "uncategorized"),
            (IssueType::NonConformance, "non-conformance"),
            (IssueType::Compounding, "compounding"),
            (IssueType::Colloquialism, "colloquialism"),
            (IssueType::FalseFriend, "false-friend"),
            (IssueType::Readability, "readability"),
            (IssueType::De, "de"),
        ];
        for (issue_type, expected) in cases {
            assert_eq!(issue_type.as_str(), expected);
        }
    }

    #[test]
    fn other_keeps_its_custom_name() {
        assert_eq!(IssueType::Other("grm".to_string()).as_str(), "grm");
    }

    #[test]
    fn unit_variants_round_trip_through_json() {
        for issue_type in [
            IssueType::Grammar,
            IssueType::Misspelling,
            IssueType::LocaleViolation,
            IssueType::NonConformance,
            IssueType::FalseFriend,
        ] {
            let json = serde_json::to_string(&issue_type).unwrap();
            assert_eq!(json, format!(r#""{}""#, issue_type.as_str()));
            let back: IssueType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, issue_type);
        }
    }
}
