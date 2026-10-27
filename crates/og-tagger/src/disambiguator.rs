use og_core::AnalyzedSentence;
use og_core::checker::Disambiguator;
use std::sync::Arc;

pub struct NoOpDisambiguator;

impl Disambiguator for NoOpDisambiguator {
    fn disambiguate(&self, _sentence: &mut AnalyzedSentence) {}
}

pub struct ChainedDisambiguator {
    stages: Vec<Arc<dyn Disambiguator>>,
}

impl ChainedDisambiguator {
    pub fn new(stages: Vec<Arc<dyn Disambiguator>>) -> Self {
        Self { stages }
    }
}

impl Disambiguator for ChainedDisambiguator {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence) {
        for stage in &self.stages {
            stage.disambiguate(sentence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chained_disambiguator_runs_all_stages() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        struct Counting(std::sync::Arc<std::sync::Mutex<usize>>);
        impl Disambiguator for Counting {
            fn disambiguate(&self, _sentence: &mut AnalyzedSentence) {
                *self.0.lock().unwrap() += 1;
            }
        }
        let mut s = AnalyzedSentence::new("x", 0, 1);
        let chain = ChainedDisambiguator::new(vec![
            std::sync::Arc::new(Counting(calls.clone())),
            std::sync::Arc::new(Counting(calls.clone())),
        ]);
        chain.disambiguate(&mut s);
        assert_eq!(*calls.lock().unwrap(), 2);
    }
}
