pub mod arabic_synthesizer;
pub mod arabic_tagger;
pub mod chunker;
pub mod disambiguator;
pub mod english_tagger;
pub mod esperanto_tagger;
pub mod fdlibm;
pub mod german_chunker;
pub mod german_tagger;
pub mod jwordsplitter;
pub mod morfologik;
pub mod multiword_chunker;
pub mod opennlp;
mod openregex;
pub mod russian_chunker;
pub mod tagger;
pub mod xml_disambiguator;

pub use disambiguator::*;
pub use english_tagger::EnglishTagger;
pub use esperanto_tagger::EsperantoTagger;
pub use german_chunker::GermanChunker;
pub use german_tagger::GermanTagger;
pub mod generic_tagger;
pub use generic_tagger::GenericTagger;
pub use multiword_chunker::{
    ChunkerParams, MultiWordChunker, MultiWordChunkerDisambiguator, hybrid_disambiguator,
};
pub use opennlp::OpenNlpEnglishChunker;
pub use russian_chunker::RussianChunker;
pub use tagger::*;
pub use xml_disambiguator::XmlDisambiguator;

use og_core::AnalyzedTokenReadings;

pub trait Tagger: Send + Sync {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings>;
}
