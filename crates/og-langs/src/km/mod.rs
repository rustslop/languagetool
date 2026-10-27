use og_core::rule::{Rule, TextLevelRule};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod replace;
pub mod space_before;
pub mod unpaired_brackets;
pub mod word_repeat;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

pub use replace::{KM_SIMPLE_REPLACE, KhmerSimpleReplaceRule};
pub use space_before::{KM_SPACE_BEFORE_CONJUNCTION, KhmerSpaceBeforeRule};
pub use unpaired_brackets::{KM_UNPAIRED_BRACKETS_ID, KhmerUnpairedBracketsRule};
pub use word_repeat::{KM_WORD_REPEAT_RULE, KhmerWordRepeatRule};
pub use word_tokenizer::KhmerWordTokenizer;

pub use KM_UNPAIRED_BRACKETS_ID as KM_UNPAIRED_BRACKETS;

pub fn km_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("km/src/main/resources/org/languagetool/resource/km"))
}

pub fn khmer_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        km_resource_dir()
            .map(|dir| Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "khmer")))
    })
}

pub fn build_km_disambiguator(resource_dir: &Path) -> Option<og_tagger::XmlDisambiguator> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    og_tagger::XmlDisambiguator::from_xml(&xml).ok()
}

fn khmer_wordlist_dictionary(resource_dir: &Path) -> og_spell::Dictionary {
    let mut dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "khmer")
        .map(|shared| {
            og_spell::Dictionary::from_shared_words(Arc::new(
                og_spell::morfologik_view::MorfologikView::new(shared),
            ))
        })
        .unwrap_or_default();
    if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
        for line in extra.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.add_word(word);
            }
        }
    }
    dict
}

fn khmer_ignore_words(resource_dir: &Path) -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt")) {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_lowercase());
            }
        }
    }
    ignore
}

pub fn khmer_hunspell_rule() -> og_spell::SpellingCheckRule {
    let resource_dir =
        km_resource_dir().unwrap_or_else(|| PathBuf::from("/nonexistent/km/resources"));
    og_spell::SpellingCheckRule::new_shared(
        Arc::new(khmer_wordlist_dictionary(&resource_dir)),
        "HUNSPELL_RULE",
        HashSet::new(),
    )
    .with_ignore_words(khmer_ignore_words(&resource_dir))
}

pub use replace::khmer_simple_replace_rule;

pub fn khmer_unpaired_brackets_rule() -> KhmerUnpairedBracketsRule {
    KhmerUnpairedBracketsRule::new()
}

pub struct KhmerNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn khmer_native_rules() -> KhmerNativeRules {
    KhmerNativeRules {
        rules: vec![
            Arc::new(khmer_hunspell_rule()),
            Arc::new(khmer_simple_replace_rule()),
            Arc::new(KhmerWordRepeatRule::new()),
            Arc::new(KhmerSpaceBeforeRule::new()),
        ],
        text_level_rules: vec![Arc::new(khmer_unpaired_brackets_rule())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = khmer_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "HUNSPELL_RULE",
                "KM_SIMPLE_REPLACE",
                "KM_WORD_REPEAT_RULE",
                "KM_SPACE_BEFORE_CONJUNCTION",
            ],
            "Khmer.java:82-89 getRelevantRules order: KhmerHunspellRule, KhmerSimpleReplaceRule, \
             KhmerWordRepeatRule, KhmerUnpairedBracketsRule (text-level here), \
             KhmerSpaceBeforeRule"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(text_ids, vec!["KM_UNPAIRED_BRACKETS"]);
    }

    #[test]
    fn unpaired_brackets_rule_carries_the_java_symbol_pairs() {
        let rule = khmer_unpaired_brackets_rule();
        assert_eq!(rule.id(), KM_UNPAIRED_BRACKETS);
        let text = "គាត់\u{200B}បាន\u{200B}និយាយ\u{200B}ថា «\u{200B}ខ្ញុំ\u{200B}សប្បាយ\u{200B}ចិត្ត។";
        let sentences = crate::km::example_suite::km_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "the unclosed « must flag (KhmerUnpairedBracketsRule.java:31)"
        );
    }

    #[test]
    fn simple_replace_rule_pins_the_java_configuration() {
        let rule = khmer_simple_replace_rule();
        assert_eq!(rule.id(), KM_SIMPLE_REPLACE);
        assert_eq!(
            rule.description(),
            "Words or groups of words that are incorrect or obsolete"
        );
        assert_eq!(rule.category().name(), "របស់ផ្សេងៗ");
    }

    #[test]
    fn hunspell_rule_honors_ignore_txt_and_the_khmer_wordlist() {
        let rule = khmer_hunspell_rule();
        assert_eq!(rule.id(), "HUNSPELL_RULE");
        assert!(
            rule.is_known_word("ការ"),
            "khmer.dict contains ការ, so the wordlist speller accepts it"
        );
        assert!(
            rule.is_known_word("ខាងឯវិញ្ញាណ"),
            "hunspell/ignore.txt words are never flagged"
        );
        assert!(!rule.is_known_word("ភាសាខ្មែរ"));
    }
}
