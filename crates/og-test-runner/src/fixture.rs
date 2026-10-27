use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenFixture {
    pub language: String,
    pub input_text: String,
    pub expected_matches: Vec<ExpectedMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedMatch {
    pub rule_id: String,
    pub message: String,
    pub offset: usize,
    pub length: usize,
    pub replacements: Vec<String>,
}

impl GoldenFixture {
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let fixture: GoldenFixture = serde_json::from_str(&content)?;
        Ok(fixture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_parses_the_golden_fixture_fields() {
        let path = std::env::temp_dir().join("og-test-runner-fixture-roundtrip.json");
        std::fs::write(
            &path,
            r#"{
                "language": "de",
                "input_text": "Ein Test.",
                "expected_matches": [
                    {
                        "rule_id": "DE_CASE",
                        "message": "großgeschrieben",
                        "offset": 4,
                        "length": 4,
                        "replacements": ["Test"]
                    }
                ]
            }"#,
        )
        .unwrap();
        let fixture = GoldenFixture::load(&path).expect("fixture loads");
        assert_eq!(fixture.language, "de");
        assert_eq!(fixture.input_text, "Ein Test.");
        assert_eq!(fixture.expected_matches.len(), 1);
        let m = &fixture.expected_matches[0];
        assert_eq!(m.rule_id, "DE_CASE");
        assert_eq!(m.offset, 4);
        assert_eq!(m.length, 4);
        assert_eq!(m.replacements, ["Test"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_rejects_malformed_fixtures() {
        let path = std::env::temp_dir().join("og-test-runner-fixture-malformed.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(GoldenFixture::load(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
