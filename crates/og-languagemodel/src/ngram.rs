// N-gram language model support for traditional statistical features
// No AI/LLM - this is for n-gram probability-based rules only

pub struct LanguageModel {
    // Will be populated with n-gram data in later phases
}

impl LanguageModel {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_probability(&self, _words: &[&str]) -> f64 {
        // Placeholder - will implement n-gram lookup
        0.0
    }
}

impl Default for LanguageModel {
    fn default() -> Self {
        Self::new()
    }
}
