use og_core::Language;

pub struct LanguageRegistry {
    languages: Vec<Language>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            languages: Language::all_languages(),
        }
    }

    pub fn all_languages(&self) -> &[Language] {
        &self.languages
    }

    pub fn get_language(&self, code: &str) -> Option<&Language> {
        self.languages.iter().find(|l| l.code() == code)
    }

    pub fn supported_codes(&self) -> Vec<&str> {
        self.languages.iter().map(|l| l.code()).collect()
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_languages_and_finds_them_by_code() {
        let registry = LanguageRegistry::default();
        let codes = registry.supported_codes();
        assert!(!codes.is_empty());
        assert!(codes.contains(&"en-US"));
        let en = registry.get_language("en-US").expect("en-US registered");
        assert_eq!(en.code(), "en-US");
        assert!(registry.get_language("zz").is_none());
        assert_eq!(registry.all_languages().len(), codes.len());
    }
}
