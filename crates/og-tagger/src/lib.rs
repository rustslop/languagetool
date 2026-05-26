pub mod tagger;
pub mod disambiguator;
pub mod english_data;
pub mod english_tagger;
pub mod xml_disambiguator;
pub mod chunker;

pub use tagger::*;
pub use disambiguator::*;
pub use english_tagger::EnglishTagger;
pub use xml_disambiguator::XmlDisambiguator;

use og_core::AnalyzedTokenReadings;

pub trait Tagger: Send + Sync {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings>;
}
