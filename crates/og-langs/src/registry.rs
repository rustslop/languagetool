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
