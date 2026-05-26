use serde::Deserialize;
use og_core::Language;

#[derive(Debug, Clone, Deserialize)]
pub struct CheckParams {
    pub text: Option<String>,
    pub data: Option<String>,
    pub language: String,
    pub mother_tongue: Option<String>,
    #[serde(rename = "enabledRules")]
    pub enabled_rules: Option<String>,
    #[serde(rename = "disabledRules")]
    pub disabled_rules: Option<String>,
    #[serde(rename = "enabledCategories")]
    pub enabled_categories: Option<String>,
    #[serde(rename = "disabledCategories")]
    pub disabled_categories: Option<String>,
    pub level: Option<String>,
    #[serde(default)]
    pub picky: bool,
}

impl CheckParams {
    pub fn get_text(&self) -> Option<&str> {
        self.text.as_deref().or(self.data.as_deref())
    }

    pub fn get_language(&self) -> Option<Language> {
        Language::from_code(&self.language)
    }

    pub fn get_enabled_rules(&self) -> Vec<String> {
        self.enabled_rules
            .as_deref()
            .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
            .unwrap_or_default()
    }

    pub fn get_disabled_rules(&self) -> Vec<String> {
        self.disabled_rules
            .as_deref()
            .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
            .unwrap_or_default()
    }

    pub fn get_enabled_categories(&self) -> Vec<String> {
        self.enabled_categories
            .as_deref()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default()
    }

    pub fn get_disabled_categories(&self) -> Vec<String> {
        self.disabled_categories
            .as_deref()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default()
    }
}
