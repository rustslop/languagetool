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
