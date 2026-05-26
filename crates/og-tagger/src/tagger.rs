use og_core::AnalyzedTokenReadings;
use crate::Tagger;

pub struct DefaultTagger;

impl DefaultTagger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultTagger {
    fn default() -> Self {
        Self::new()
    }
}

impl Tagger for DefaultTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        tokens
            .iter()
            .enumerate()
            .map(|(i, token)| {
                use og_core::AnalyzedToken;
                let at = AnalyzedToken::new(*token, 0, token.len());
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}
