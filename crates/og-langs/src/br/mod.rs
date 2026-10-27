pub mod normalize;
pub mod speller;
pub mod tagger;
pub mod topo_replace;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;
#[cfg(test)]
pub(crate) mod trace;

pub use normalize::{normalize_br_disambiguation_xml, normalize_br_rules_xml};
pub use speller::BretonSpellerRule;
pub use tagger::BretonTagger;
pub use topo_replace::TopoReplaceRule;

pub fn breton_topo_replace_rule() -> TopoReplaceRule {
    TopoReplaceRule::new()
}

pub fn build_br_disambiguator(
    resource_dir: &std::path::Path,
) -> Option<og_tagger::XmlDisambiguator> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    og_tagger::XmlDisambiguator::from_xml(&normalize::normalize_br_disambiguation_xml(&xml)).ok()
}
