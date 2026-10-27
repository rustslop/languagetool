use og_core::rule::Rule;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
mod example_suite;

pub const SK_COMPOUNDS: &str = "SK_COMPOUNDS";
pub const SK_SPELLER: &str = "MORFOLOGIK_RULE_SK_SK";

pub fn sk_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("sk/src/main/resources/org/languagetool/resource/sk"))
}

pub fn slovak_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        sk_resource_dir()
            .map(|dir| Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "slovak")))
    })
}

pub fn slovak_compound_rule() -> crate::en::CompoundRule {
    crate::en::CompoundRule::for_language(
        SK_COMPOUNDS,
        "Slová so spojovníkom napr. použite „česko-slovenský” namiesto „česko slovenský”",
        "Toto slovo sa zvyčajne píše so spojovníkom.",
        "Toto slovo sa obvykle píše bez spojovníka.",
        "Tento výraz sa bežne píše s alebo bez spojovníka.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/sk/src/main/resources/org/languagetool/resource/sk/compounds.txt"
        ),
    )
}

pub fn slovak_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\u{201e}", "\u{bb}", "\u{ab}", "\""],
        vec!["]", ")", "}", "\u{201c}", "\u{ab}", "\u{bb}", "\""],
    )
}

pub fn normalize_sk_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    match xml.strip_prefix('\u{feff}') {
        Some(stripped) => std::borrow::Cow::Owned(stripped.to_string()),
        None => std::borrow::Cow::Borrowed(xml),
    }
}

pub fn sk_extra_rule_paths(lt_root: &std::path::Path) -> Vec<PathBuf> {
    let typography =
        lt_root.join("sk/src/main/resources/org/languagetool/rules/sk/grammar-typography.xml");
    if typography.is_file() {
        vec![typography]
    } else {
        Vec::new()
    }
}

pub struct SlovakNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
}

pub fn slovak_native_rules() -> SlovakNativeRules {
    SlovakNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            Arc::new(slovak_compound_rule()),
        ],
    }
}

fn slovak_wordlist_dictionary(resource_dir: &std::path::Path) -> og_spell::Dictionary {
    let mut dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "slovak")
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

fn slovak_ignore_words(resource_dir: &std::path::Path) -> std::collections::HashSet<String> {
    let mut ignore = std::collections::HashSet::new();
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

pub fn slovak_speller_rule() -> og_spell::SpellingCheckRule {
    let resource_dir =
        sk_resource_dir().unwrap_or_else(|| PathBuf::from("/nonexistent/sk/resources"));
    og_spell::SpellingCheckRule::new_shared(
        Arc::new(slovak_wordlist_dictionary(&resource_dir)),
        SK_SPELLER,
        std::collections::HashSet::new(),
    )
    .with_ignore_words(slovak_ignore_words(&resource_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::TextLevelRule as _;

    #[test]
    fn natives_carry_the_java_ids_missing_from_the_generic_path() {
        let natives = slovak_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec!["UPPERCASE_SENTENCE_START", "WORD_REPEAT_RULE", SK_COMPOUNDS],
            "Slovak.java:85-99 getRelevantRules minus the five rules the generic engine path \
             already installs (CommaWhitespace, DoublePunctuation, the sk-symbol unpaired \
             brackets, MultipleWhitespace, the wordlist speller)"
        );
    }

    #[test]
    fn compound_rule_pins_the_java_description_example() {
        let rule = slovak_compound_rule();
        assert_eq!(rule.id(), SK_COMPOUNDS);
        assert_eq!(
            rule.description(),
            "Slová so spojovníkom napr. použite „česko-slovenský” namiesto „česko slovenský”"
        );
        let pipeline = crate::sk::example_suite::sk_pipeline();
        let text = "Toto je česko slovenský projekt.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.iter().any(
                |m| &text[m.offset()..m.offset() + m.length()] == "česko slovenský"
                    && m.replacements()
                        .iter()
                        .any(|r| r.value() == "česko-slovenský")
            ),
            "the spaced form of the description's example must suggest the hyphenated form: \
             {matches:?}"
        );
        let text = "Toto je česko-slovenský projekt.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "the hyphenated form is accepted"
        );
    }

    #[test]
    fn unpaired_brackets_track_the_java_quote_symbols() {
        let rule = slovak_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        let text = "Toto je veta s \u{201e}úvodzovkou.";
        let sentences = crate::sk::example_suite::sk_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "\u{201e}"),
            "the unclosed Slovak low quote must flag: {matches:?}"
        );
        let text = "Toto je veta s \u{201e}úvodzovkou\u{201c}.";
        let sentences = crate::sk::example_suite::sk_pipeline().analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn normalize_sk_rules_xml_strips_only_the_leading_bom() {
        assert!(matches!(
            normalize_sk_rules_xml("<rules lang='sk'/>"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(
            normalize_sk_rules_xml("\u{feff}<rules lang='sk'/>").as_ref(),
            "<rules lang='sk'/>"
        );
        assert_eq!(
            normalize_sk_rules_xml("<a>\u{feff}</a>").as_ref(),
            "<a>\u{feff}</a>",
            "an inner BOM-shaped character must survive"
        );
    }

    #[test]
    fn typography_rule_path_resolves_and_speller_uses_the_slovak_wordlist() {
        let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
        let extra = sk_extra_rule_paths(&lt_root);
        assert_eq!(
            extra.len(),
            1,
            "Slovak.java:41-43 adds grammar-typography.xml"
        );
        assert!(
            extra[0]
                .to_string_lossy()
                .ends_with("grammar-typography.xml"),
            "{extra:?}"
        );
        let rule = slovak_speller_rule();
        assert_eq!(rule.id(), SK_SPELLER);
        assert!(
            rule.is_known_word("slovenský"),
            "the wordlist speller draws on slovak.dict"
        );
        assert!(!rule.is_known_word("xqzvję"));
    }

    #[test]
    fn slovak_tagger_loads_the_morfologik_dictionary() {
        let tagger = slovak_tagger().expect("slovak.dict tagger");
        use og_core::checker::Tagger as _;
        let mut readings = vec![
            og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new("<S>", 0, 0)
                    .with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                "slovenský",
                0,
                "slovenský".len(),
            )),
        ];
        tagger.tag(&mut readings);
        assert!(
            readings[1]
                .readings()
                .iter()
                .any(|r| r.lemma().is_some() && !r.pos_tags().is_empty()),
            "slovak.dict must return lemma+pos readings: {:?}",
            readings[1].readings()
        );
    }
}
