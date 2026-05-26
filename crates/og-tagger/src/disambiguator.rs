use og_core::AnalyzedSentence;

pub trait Disambiguator: Send + Sync {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence);
}

pub struct NoOpDisambiguator;

impl Disambiguator for NoOpDisambiguator {
    fn disambiguate(&self, _sentence: &mut AnalyzedSentence) {
        // No-op
    }
}
