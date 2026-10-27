use og_core::rule::{Rule, TextLevelRule};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(test)]
mod example_suite;

pub const SR_RULE_FILES: [&str; 5] = [
    "grammar-barbarism.xml",
    "grammar-logical.xml",
    "grammar-punctuation.xml",
    "grammar-spelling.xml",
    "grammar-style.xml",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerbianVariant {
    Ekavian,
    Jekavian,
}

impl SerbianVariant {
    const fn dictionary_dir(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "ekavian",
            SerbianVariant::Jekavian => "jekavian",
        }
    }

    pub const fn speller_id(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "MORFOLOGIK_RULE_SR_EKAVIAN",
            SerbianVariant::Jekavian => "MORFOLOGIK_RULE_SR_JEKAVIAN",
        }
    }

    pub const fn grammar_replace_id(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE",
            SerbianVariant::Jekavian => "SR_JEKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE",
        }
    }

    pub const fn style_replace_id(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "SR_EKAVIAN_SIMPLE_STYLE_REPLACE_RULE",
            SerbianVariant::Jekavian => "SR_JEKAVIAN_SIMPLE_STYLE_REPLACE_RULE",
        }
    }

    fn grammar_replace_description(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "Провера граматички погрешних речи или израза",
            SerbianVariant::Jekavian => "Провера граматички погрешних ријечи или израза",
        }
    }

    fn grammar_replace_short(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "Граматички погрешна реч тј. израз",
            SerbianVariant::Jekavian => "Граматички погрешна ријеч тј. израз",
        }
    }

    fn style_replace_description(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "Провера стилски лоших речи или израза",
            SerbianVariant::Jekavian => "Провера стилски лоших ријечи или израза",
        }
    }

    fn style_replace_short(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => "Стилски лоша реч тј. израз",
            SerbianVariant::Jekavian => "Стилски лоша ријеч тј. израз",
        }
    }

    fn grammar_replace_message(self) -> &'static str {
        "Не каже се „$match“ него „$suggestions“."
    }

    fn style_replace_message(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => {
                "Уместо израза „$match“ било би боље да користите: $suggestions."
            }
            SerbianVariant::Jekavian => {
                "Умјесто израза „$match“ било би боље да користите: $suggestions."
            }
        }
    }

    fn grammar_replace_data(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => include_str!(
                "../../../../languagetool/languagetool-language-modules/sr/src/main/resources/org/languagetool/rules/sr/ekavian/replace-grammar.txt"
            ),
            SerbianVariant::Jekavian => include_str!(
                "../../../../languagetool/languagetool-language-modules/sr/src/main/resources/org/languagetool/rules/sr/jekavian/replace-grammar.txt"
            ),
        }
    }

    fn style_replace_data(self) -> &'static str {
        match self {
            SerbianVariant::Ekavian => include_str!(
                "../../../../languagetool/languagetool-language-modules/sr/src/main/resources/org/languagetool/rules/sr/ekavian/replace-style.txt"
            ),
            SerbianVariant::Jekavian => include_str!(
                "../../../../languagetool/languagetool-language-modules/sr/src/main/resources/org/languagetool/rules/sr/jekavian/replace-style.txt"
            ),
        }
    }
}

pub fn sr_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("sr/src/main/resources/org/languagetool/resource/sr"))
}

pub fn sr_variant_dictionary_dir(variant: SerbianVariant, resource_root: &Path) -> PathBuf {
    resource_root
        .join("dictionary")
        .join(variant.dictionary_dir())
}

pub fn serbian_tagger(variant: SerbianVariant) -> Option<Arc<og_tagger::GenericTagger>> {
    static CELLS: [og_core::releasable::Releasable<og_tagger::GenericTagger>; 2] = [
        og_core::releasable::Releasable::new(),
        og_core::releasable::Releasable::new(),
    ];
    let cell = &CELLS[variant as usize];
    og_core::releasable::shared_get_or_build(cell, || {
        sr_resource_dir().map(|root| {
            let dir = sr_variant_dictionary_dir(variant, &root);
            Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "serbian"))
        })
    })
}

pub fn build_sr_disambiguator(resource_dir: &Path) -> Option<og_tagger::XmlDisambiguator> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    og_tagger::XmlDisambiguator::from_xml(&xml).ok()
}

pub fn sr_extra_rule_paths(lt_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for file in SR_RULE_FILES {
        let path = lt_root.join(format!(
            "sr/src/main/resources/org/languagetool/rules/sr/{file}"
        ));
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}

pub fn serbian_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\u{201e}", "\u{201e}", "\""],
        vec!["]", ")", "}", "\u{201d}", "\u{201c}", "\""],
    )
}

fn sr_shared_speller_dictionary(
    variant: SerbianVariant,
    resource_root: &Path,
) -> Arc<og_spell::Dictionary> {
    static CELLS: [og_core::releasable::Releasable<og_spell::Dictionary>; 2] = [
        og_core::releasable::Releasable::new(),
        og_core::releasable::Releasable::new(),
    ];
    let cell = &CELLS[variant as usize];
    og_core::releasable::shared_get_or_build(cell, || {
        let dir = sr_variant_dictionary_dir(variant, resource_root);
        let mut dict = og_tagger::morfologik::FsaWordSet::load(
            &dir.join("serbian.dict"),
            &dir.join("serbian.info"),
        )
        .map(std::sync::Arc::new)
        .map(og_spell::Dictionary::from_fsa_word_set)
        .unwrap_or_else(og_spell::Dictionary::new);
        if let Ok(extra) = std::fs::read_to_string(dir.join("spelling.txt")) {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        (!dict.is_empty()).then_some(Arc::new(dict))
    })
    .unwrap_or_else(|| Arc::new(og_spell::Dictionary::new()))
}

fn sr_ignore_words(variant: SerbianVariant, resource_root: &Path) -> HashSet<String> {
    let mut ignore = HashSet::new();
    let dir = sr_variant_dictionary_dir(variant, resource_root);
    if let Ok(words) = std::fs::read_to_string(dir.join("ignored.txt")) {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_lowercase());
            }
        }
    }
    ignore
}

pub fn sr_speller_rule(variant: SerbianVariant) -> og_spell::SpellingCheckRule {
    let resource_root =
        sr_resource_dir().unwrap_or_else(|| PathBuf::from("/nonexistent/sr/resources"));
    og_spell::SpellingCheckRule::new_shared(
        sr_shared_speller_dictionary(variant, &resource_root),
        variant.speller_id(),
        HashSet::new(),
    )
    .with_ignore_words(sr_ignore_words(variant, &resource_root))
}

pub fn sr_grammar_replace_rule(
    variant: SerbianVariant,
) -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        variant.grammar_replace_id(),
        variant.grammar_replace_description(),
        variant.grammar_replace_short(),
        variant.grammar_replace_message(),
    );
    rule.load_data(variant.grammar_replace_data());
    rule
}

pub fn sr_style_replace_rule(
    variant: SerbianVariant,
) -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        variant.style_replace_id(),
        variant.style_replace_description(),
        variant.style_replace_short(),
        variant.style_replace_message(),
    );
    rule.load_data(variant.style_replace_data());
    rule
}

pub struct SerbianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn serbian_native_rules(variant: SerbianVariant) -> SerbianNativeRules {
    SerbianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            Arc::new(sr_speller_rule(variant)),
            Arc::new(sr_grammar_replace_rule(variant)),
            Arc::new(sr_style_replace_rule(variant)),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
            Arc::new(serbian_unpaired_brackets_rule()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        for variant in [SerbianVariant::Ekavian, SerbianVariant::Jekavian] {
            let natives = serbian_native_rules(variant);
            let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
            assert_eq!(
                ids,
                vec![
                    "COMMA_PARENTHESIS_WHITESPACE",
                    "DOUBLE_PUNCTUATION",
                    "UPPERCASE_SENTENCE_START",
                    "WORD_REPEAT_RULE",
                    variant.speller_id(),
                    variant.grammar_replace_id(),
                    variant.style_replace_id(),
                ],
                "Serbian.java:142-167 getRelevantRules order: CommaWhitespaceRule, \
                 DoublePunctuationRule, GenericUnpairedBracketsRule (text), \
                 UppercaseSentenceStartRule, MultipleWhitespaceRule (text), \
                 SentenceWhitespaceRule (text), WordRepeatRule, the variant Morfologik \
                 SpellerRule and the variant Simple Grammar/Style ReplaceRules for {variant:?}"
            );
            let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
            assert_eq!(
                text_ids,
                vec![
                    "WHITESPACE_RULE",
                    "SENTENCE_WHITESPACE",
                    "UNPAIRED_BRACKETS"
                ]
            );
        }
    }

    #[test]
    fn extra_rule_paths_cover_the_full_java_file_set() {
        let lt_root = crate::engine::find_lt_resource_dir().expect("lt resource dir");
        let extra = sr_extra_rule_paths(&lt_root);
        let names: Vec<String> = extra
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "grammar-barbarism.xml",
                "grammar-logical.xml",
                "grammar-punctuation.xml",
                "grammar-spelling.xml",
                "grammar-style.xml",
            ],
            "Serbian.java:67-74 RULE_FILES: grammar.xml itself loads through the generic \
             grammar.xml/style.xml loop, the five extras through this list"
        );
    }

    #[test]
    fn taggers_pin_the_java_variant_dictionaries() {
        use og_core::checker::Tagger as _;
        let readings_for = |variant: SerbianVariant, word: &str| -> Vec<(String, String)> {
            let tagger = serbian_tagger(variant).expect("serbian dict tagger");
            let mut readings = vec![og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new("<S>", 0, 0)
                    .with_pos_tags(vec!["SENT_START".to_string()]),
            )];
            readings.push(og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new(word, 0, word.len()),
            ));
            tagger.tag(&mut readings);
            readings[1]
                .readings()
                .iter()
                .map(|r| (r.lemma().unwrap_or("").to_string(), r.pos_tags().join("+")))
                .collect()
        };
        let radim = readings_for(SerbianVariant::Ekavian, "радим");
        assert!(
            radim
                .iter()
                .any(|(lemma, pos)| lemma == "радити" && pos == "GL:GV:PZ:1L:0J"),
            "EkavianTaggerTest.testTaggerRaditi pins радим/[радити]GL:GV:PZ:1L:0J, got {radim:?}"
        );
        let je = readings_for(SerbianVariant::Ekavian, "је");
        assert!(
            je.iter()
                .any(|(lemma, pos)| lemma == "јесам" && pos == "GL:PM:PZ:3L:0J"),
            "EkavianTaggerTest.testTaggerJesam pins је/[јесам]GL:PM:PZ:3L:0J, got {je:?}"
        );
        assert!(
            readings_for(SerbianVariant::Ekavian, "лијеп").is_empty(),
            "the ekavian dictionary must not tag the jekavian лијеп \
             (EkavianTaggerTest 'Proof that Ekavian tagger does not tag Jekavian words')"
        );
        let cvijete = readings_for(SerbianVariant::Jekavian, "цвијете");
        assert!(
            cvijete
                .iter()
                .any(|(lemma, pos)| lemma == "цвијет" && pos == "IM:ZA:MU:0J:VO"),
            "JekavianTaggerTest.testTaggerSvijet pins цвијете/[цвијет]IM:ZA:MU:0J:VO, got \
             {cvijete:?}"
        );
        assert!(
            readings_for(SerbianVariant::Jekavian, "онде").is_empty(),
            "the jekavian dictionary must not tag the ekavian онде (JekavianTaggerTest)"
        );
    }

    #[test]
    fn disambiguator_loads_the_module_disambiguation() {
        let resource_dir = sr_resource_dir().expect("sr resource dir");
        assert!(
            build_sr_disambiguator(&resource_dir).is_some(),
            "disambiguation.xml (RIMSKI_BROJEVI, ignore_spelling) must parse"
        );
        assert!(
            std::fs::read_to_string(resource_dir.join("multiwords.txt"))
                .is_ok_and(|t| t.trim().is_empty()),
            "multiwords.txt is empty, so SerbianHybridDisambiguator's MultiWordChunker stage \
             is a no-op and the XmlRuleDisambiguator alone is full parity"
        );
    }

    #[test]
    fn unpaired_brackets_track_the_java_serbian_quote_symbols() {
        let rule = serbian_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        let text = "Ово је реченица са \u{201e}наводником.";
        let sentences = crate::sr::example_suite::sr_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "\u{201e}"),
            "the unclosed Serbian low quote must flag: {matches:?}"
        );
        let text = "Ово је реченица са \u{201e}наводником\u{201d}.";
        let sentences = crate::sr::example_suite::sr_pipeline().analyze_text(text);
        assert!(
            rule.match_text(text, &sentences).is_empty(),
            "\u{201e} closed by \u{201d} must not flag"
        );
    }

    #[test]
    fn replace_rules_pin_the_java_test_corpus() {
        let grammar = sr_grammar_replace_rule(SerbianVariant::Ekavian);
        assert_eq!(grammar.id(), "SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE");
        let pipeline = crate::sr::example_suite::sr_pipeline();
        let sentence = pipeline.analyze_text("Син је вишљи од оца.").pop().unwrap();
        let matches = grammar.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "SimpleGrammarEkavianReplaceRuleTest");
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "виши"),
            "got {:?}",
            matches[0].replacements()
        );
        let sentence = pipeline
            .analyze_text("У то оправдано сумљам.")
            .pop()
            .unwrap();
        let matches = grammar.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "сумњам")
        );
        let sentence = pipeline.analyze_text("Данас је диван дан.").pop().unwrap();
        assert!(grammar.match_sentence(&sentence).is_empty());

        let style = sr_style_replace_rule(SerbianVariant::Ekavian);
        assert_eq!(style.id(), "SR_EKAVIAN_SIMPLE_STYLE_REPLACE_RULE");
        let sentence = pipeline.analyze_text("Она је дебела.").pop().unwrap();
        let matches = style.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "SimpleStyleEkavianReplaceRuleTest");
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "елегантно попуњена"),
            "the multiword style suggestion must survive: {:?}",
            matches[0].replacements()
        );
        assert!(
            matches[0].message().contains("Уместо израза „дебела“"),
            "the style message template must render, got: {}",
            matches[0].message()
        );
        let sentence = pipeline.analyze_text("Он је добар.").pop().unwrap();
        assert!(style.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn spellers_pin_the_java_variant_dictionaries() {
        let ekavian = sr_speller_rule(SerbianVariant::Ekavian);
        assert_eq!(ekavian.id(), "MORFOLOGIK_RULE_SR_EKAVIAN");
        assert!(ekavian.is_known_word("леп"));
        assert!(!ekavian.is_known_word("Misspelled"));
        let jekavian = sr_speller_rule(SerbianVariant::Jekavian);
        assert_eq!(jekavian.id(), "MORFOLOGIK_RULE_SR_JEKAVIAN");
        assert!(jekavian.is_known_word("лијеп"));
        assert!(jekavian.is_known_word("цвијет"));
        assert!(!jekavian.is_known_word("Misspelled"));
    }
}
