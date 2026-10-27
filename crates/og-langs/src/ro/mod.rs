use og_core::rule::{Rule, TextLevelRule};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod compound;
pub mod word_repeat_beginning;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

pub use compound::RomanianCompoundRule;
pub use word_repeat_beginning::{
    ROMANIAN_WORD_REPEAT_BEGINNING_RULE, RomanianWordRepeatBeginningRule,
};
pub use word_tokenizer::RomanianWordTokenizer;

pub fn ro_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ro/src/main/resources/org/languagetool/resource/ro"))
}

pub fn romanian_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        ro_resource_dir().map(|dir| {
            Arc::new(og_tagger::GenericTagger::from_resource_dir(
                &dir, "romanian",
            ))
        })
    })
}

pub fn build_ro_disambiguator(resource_dir: &Path) -> Option<og_tagger::XmlDisambiguator> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    og_tagger::XmlDisambiguator::from_xml(&xml).ok()
}

pub fn romanian_replace_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "RO_SIMPLE_REPLACE",
        "Cuvinte sau grupuri de cuvinte incorecte sau ieșite din uz",
        "Cuvânt incorect sau ieșit din uz",
        "'$match' este incorect sau ieșit din uz, folosiți $suggestions",
    )
    .with_separator(" sau ");
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/ro/src/main/resources/org/languagetool/rules/ro/replace.txt"
    ));
    rule
}

pub fn romanian_compound_rule() -> RomanianCompoundRule {
    RomanianCompoundRule::new(
        "RO_COMPOUND",
        "Greșeală de scriere (cuvinte scrise legat sau cu cratimă)",
        "Cuvântul se scrie cu cratimă.",
        "Cuvântul se scrie legat.",
        "Cuvântul se scrie legat sau cu cratimă.",
        Some("Problemă de scriere (cratimă, spațiu, etc.)"),
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ro/src/main/resources/org/languagetool/resource/ro/compounds.txt"
        ),
    )
}

pub fn romanian_word_repeat_beginning_rule() -> RomanianWordRepeatBeginningRule {
    RomanianWordRepeatBeginningRule::new()
}

pub fn romanian_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "„", "«", "»"],
        vec!["]", ")", "}", "”", "»", "«"],
    )
}

fn romanian_wordlist_dictionary(resource_dir: &Path) -> og_spell::Dictionary {
    let mut dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "romanian")
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

fn romanian_ignore_words(resource_dir: &Path) -> HashSet<String> {
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

pub fn romanian_speller_rule() -> og_spell::SpellingCheckRule {
    let resource_dir =
        ro_resource_dir().unwrap_or_else(|| PathBuf::from("/nonexistent/ro/resources"));
    og_spell::SpellingCheckRule::new_shared(
        Arc::new(romanian_wordlist_dictionary(&resource_dir)),
        "MORFOLOGIK_RULE_RO_RO",
        HashSet::new(),
    )
    .with_ignore_words(romanian_ignore_words(&resource_dir))
}

pub struct RomanianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn romanian_native_rules() -> RomanianNativeRules {
    RomanianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            Arc::new(romanian_speller_rule()),
            Arc::new(romanian_replace_rule()),
            Arc::new(romanian_compound_rule()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(romanian_unpaired_brackets_rule()),
            Arc::new(romanian_word_repeat_beginning_rule()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = romanian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
                "MORFOLOGIK_RULE_RO_RO",
                "RO_SIMPLE_REPLACE",
                "RO_COMPOUND",
            ],
            "Romanian.java:76-92 getRelevantRules order: CommaWhitespaceRule, \
             DoublePunctuationRule, UppercaseSentenceStartRule, MultipleWhitespaceRule (text), \
             GenericUnpairedBracketsRule (text), WordRepeatRule, \
             MorfologikRomanianSpellerRule, RomanianWordRepeatBeginningRule (text), \
             SimpleReplaceRule, CompoundRule"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_ids,
            vec![
                "WHITESPACE_RULE",
                "UNPAIRED_BRACKETS",
                "ROMANIAN_WORD_REPEAT_BEGINNING_RULE"
            ]
        );
    }

    #[test]
    fn replace_rule_loads_the_full_data_file() {
        let rule = romanian_replace_rule();
        assert_eq!(rule.id(), "RO_SIMPLE_REPLACE");
        assert_eq!(
            rule.description(),
            "Cuvinte sau grupuri de cuvinte incorecte sau ieșite din uz"
        );
    }

    #[test]
    fn replace_rule_matches_the_javadoc_phrase_example() {
        let rule = romanian_replace_rule();
        let sentences =
            crate::ro::example_suite::ro_pipeline().analyze_text("A pictat în tehnica aqua forte.");
        let matches = rule.match_sentence(&sentences[0]);
        assert_eq!(
            matches.len(),
            1,
            "AbstractSimpleReplaceRule2's own javadoc cites the ro phrase 'aqua forte' -> \
             'acvaforte', which the 3-token en::SimpleReplaceRule used before could never load"
        );
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "acvaforte")
        );
        assert!(
            matches[0]
                .message()
                .contains(" este incorect sau ieșit din uz"),
            "the ro message template must render, got: {}",
            matches[0].message()
        );
    }

    #[test]
    fn tagger_and_disambiguator_load() {
        let resource_dir = ro_resource_dir().expect("ro resource dir");
        assert!(
            build_ro_disambiguator(&resource_dir).is_some(),
            "disambiguation.xml (3 rules, no DOCTYPE entities) must parse"
        );
        assert!(romanian_tagger().is_some(), "romanian.dict must load");
    }
}
