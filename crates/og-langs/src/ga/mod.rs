use og_core::checker::Disambiguator;
use og_core::rule::{Rule, TextLevelRule};
use og_core::{AnalyzedSentence, RuleMatchContext};
use og_tagger::ChainedDisambiguator;
use std::path::Path;
use std::sync::Arc;

pub(crate) fn context_window(
    sentence: &AnalyzedSentence,
    from: usize,
    to: usize,
    pad: usize,
) -> RuleMatchContext {
    let text = sentence.text();
    let text_len = sentence.start() + text.len();
    let mut ctx_start = from.saturating_sub(pad).max(sentence.start());
    while ctx_start > sentence.start() && !text.is_char_boundary(ctx_start - sentence.start()) {
        ctx_start -= 1;
    }
    let mut ctx_end = (to + pad).min(text_len);
    while ctx_end < text_len && !text.is_char_boundary(ctx_end - sentence.start()) {
        ctx_end += 1;
    }
    let local_start = ctx_start - sentence.start();
    let local_end = ctx_end - sentence.start();
    RuleMatchContext::new(
        text[local_start..local_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

pub mod compound;
pub mod dative_plurals;
pub mod dha_no_beirt;
pub mod replace;
pub mod specific_case;
pub mod utils;

#[cfg(test)]
mod example_suite;

pub use compound::IrishCompoundRule;
pub use dative_plurals::dative_plural_standard_replace_rule;
pub use dha_no_beirt::DhaNoBeirtRule;
pub use replace::{
    english_homophone_rule, irish_fgb_eq_replace_rule, irish_replace_rule, logainm_rule,
    people_rule, prestandard_replace_rule, spaces_rule,
};
pub use specific_case::IrishSpecificCaseRule;

pub fn build_ga_disambiguator(resource_dir: &Path) -> Option<ChainedDisambiguator> {
    let multiwords = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &resource_dir.join("multiwords.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: false,
            allow_all_uppercase: false,
            allow_titlecase: false,
            default_tag: None,
            remove_previous_tags: false,
            add_ignore_spelling: false,
        },
    );
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let xml = og_tagger::XmlDisambiguator::from_xml(&xml).ok()?;
    let mut stages: Vec<Arc<dyn Disambiguator>> = Vec::new();
    if let Some(chunker) = multiwords {
        stages.push(Arc::new(chunker));
    }
    stages.push(Arc::new(xml));
    Some(ChainedDisambiguator::new(stages))
}

pub fn irish_paragraph_repeat_beginning_rule()
-> crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule {
    crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule::new(
        "Same beginning of paragraph",
        "Same beginning as last paragraph",
        "Style",
    )
}

pub struct IrishNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn irish_native_rules() -> IrishNativeRules {
    IrishNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            Arc::new(og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new()),
            Arc::new(logainm_rule()),
            Arc::new(people_rule()),
            Arc::new(spaces_rule()),
            Arc::new(IrishCompoundRule::new()),
            Arc::new(prestandard_replace_rule()),
            Arc::new(irish_replace_rule()),
            Arc::new(irish_fgb_eq_replace_rule()),
            Arc::new(english_homophone_rule()),
            Arc::new(DhaNoBeirtRule::new()),
            Arc::new(dative_plural_standard_replace_rule()),
            Arc::new(IrishSpecificCaseRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
            Arc::new(irish_paragraph_repeat_beginning_rule()),
        ],
    }
}
