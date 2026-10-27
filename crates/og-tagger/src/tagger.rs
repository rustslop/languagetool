use crate::Tagger;
use og_core::AnalyzedTokenReadings;

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
            .map(|token| {
                use og_core::AnalyzedToken;
                let at = AnalyzedToken::new(*token, 0, token.len());
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_every_token_without_pos_readings() {
        let tagger = DefaultTagger::new();
        let readings = tagger.tag(&["Der", "Hund"]);
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].token().token(), "Der");
        assert_eq!(readings[0].token().pos_tags(), Vec::<String>::new());
        assert_eq!(readings[1].token().token(), "Hund");
        assert_eq!(readings[1].token().pos_tags(), Vec::<String>::new());
    }

    #[test]
    fn tags_an_empty_token_slice_into_no_readings() {
        let tagger = DefaultTagger::default();
        assert!(tagger.tag(&[]).is_empty());
    }
}
