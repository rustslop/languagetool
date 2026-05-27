pub mod avsan;
pub mod contraction;
pub mod consistent_apostrophe;
pub mod diacritics;
pub mod simple_replace;
pub mod specific_case;

pub use avsan::AvsAnRule;
pub use contraction::ContractionSpellingRule;
pub use consistent_apostrophe::ConsistentApostrophesRule;
pub use diacritics::DiacriticsRule;
pub use simple_replace::SimpleReplaceRule;
pub use specific_case::SpecificCaseRule;
