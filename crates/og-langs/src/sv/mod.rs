use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

#[cfg(test)]
mod example_suite;

pub const SV_COMPOUNDS: &str = "SV_COMPOUNDS";
pub const SV_WORD_COHERENCY: &str = "SV_WORD_COHERENCY";
pub const SV_SPELLER: &str = "HUNSPELL_RULE";

pub fn sv_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("sv/src/main/resources/org/languagetool/resource/sv"))
}

pub fn swedish_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        sv_resource_dir()
            .map(|dir| Arc::new(og_tagger::GenericTagger::from_resource_dir(&dir, "swedish")))
    })
}

pub fn build_sv_disambiguator(
    resource_dir: &std::path::Path,
) -> Option<Arc<dyn og_core::checker::Disambiguator>> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let disambiguator = og_tagger::XmlDisambiguator::from_xml(&xml).ok()?;
    let core_resource_dir =
        crate::engine::find_lt_core_resource_dir().unwrap_or_else(|| resource_dir.to_path_buf());
    Some(og_tagger::hybrid_disambiguator(
        "sv",
        resource_dir,
        &core_resource_dir,
        disambiguator,
    ))
}

pub fn swedish_compound_rule() -> crate::ro::RomanianCompoundRule {
    crate::ro::RomanianCompoundRule::new(
        SV_COMPOUNDS,
        "Särskrivningar, t.ex. 'e mail' bör skrivas 'e-mail'",
        "Dessa ord skrivs samman med bindestreck.",
        "Dessa ord skrivs samman.",
        "Dessa ord skrivs samman med eller utan bindestreck.",
        None,
        include_str!(
            "../../../../languagetool/languagetool-language-modules/sv/src/main/resources/org/languagetool/resource/sv/compounds.txt"
        ),
    )
}

pub fn swedish_word_coherency_rule() -> crate::en::WordCoherencyRule {
    crate::en::WordCoherencyRule::for_language(
        SV_WORD_COHERENCY,
        "Enhetlig och konsekvent stavning av ord när det finns stavningsvarianter att välja på.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/sv/src/main/resources/org/languagetool/rules/sv/coherency.txt"
        ),
    )
    .with_message_template(
        "Använd endast en av stavningsvarianterna '{word1}' och '{word2}' i en och samma text.",
    )
}

pub fn swedish_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule {
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec!["[", "(", "{", "\"", "'"],
        vec!["]", ")", "}", "\"", "'"],
    )
}

fn swedish_wordlist_dictionary(resource_dir: &std::path::Path) -> og_spell::Dictionary {
    let mut dict = og_tagger::generic_tagger::shared_dictionary(resource_dir, "swedish")
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

fn swedish_ignore_words(resource_dir: &std::path::Path) -> std::collections::HashSet<String> {
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

pub fn swedish_speller_rule() -> og_spell::SpellingCheckRule {
    let resource_dir = sv_resource_dir().unwrap_or_else(|| PathBuf::from("/nonexistent/sv"));
    og_spell::SpellingCheckRule::new_shared(
        Arc::new(swedish_wordlist_dictionary(&resource_dir)),
        SV_SPELLER,
        std::collections::HashSet::new(),
    )
    .with_ignore_words(swedish_ignore_words(&resource_dir))
}

pub fn normalize_sv_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    static POSSESSIVE: OnceLock<regex::Regex> = OnceLock::new();
    let re = POSSESSIVE.get_or_init(|| {
        regex::Regex::new(r"\{(\d+(?:,\d+)?)\}\+").expect("possessive quantifier pattern")
    });
    match re.is_match(xml) {
        true => std::borrow::Cow::Owned(re.replace_all(xml, "{${1}}").into_owned()),
        false => std::borrow::Cow::Borrowed(xml),
    }
}

pub struct SwedishNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn swedish_native_rules() -> SwedishNativeRules {
    SwedishNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            Arc::new(og_rules::native_rules::DoublePunctuationRule::new()),
            Arc::new(swedish_speller_rule()),
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            Arc::new(swedish_compound_rule()),
        ],
        text_level_rules: vec![
            Arc::new(swedish_unpaired_brackets_rule()),
            Arc::new(og_rules::text_level_rules::LongParagraphRule::new(150)),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(40)),
            Arc::new(swedish_word_coherency_rule()),
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn natives_carry_the_java_inventory_order() {
        let natives = swedish_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "COMMA_PARENTHESIS_WHITESPACE",
                "DOUBLE_PUNCTUATION",
                SV_SPELLER,
                "UPPERCASE_SENTENCE_START",
                "WORD_REPEAT_RULE",
                SV_COMPOUNDS,
            ],
            "the sentence-level half of Swedish.java:97-113 getRelevantRules in Java order"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_ids,
            vec![
                "UNPAIRED_BRACKETS",
                "TOO_LONG_PARAGRAPH",
                "TOO_LONG_SENTENCE",
                SV_WORD_COHERENCY,
                "WHITESPACE_RULE",
                "SENTENCE_WHITESPACE",
            ],
            "the text-level half of Swedish.java:97-113 getRelevantRules in Java order"
        );
    }

    #[test]
    fn long_rules_pin_the_java_thresholds() {
        let natives = swedish_native_rules();
        let long_sentence = natives
            .text_level_rules
            .iter()
            .find(|r| r.id() == "TOO_LONG_SENTENCE")
            .expect("LongSentenceRule(40) is Swedish.java:107");
        let text = (0..41).map(|_| "ord").collect::<Vec<_>>().join(" ") + ".";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(&text);
        assert!(
            long_sentence
                .match_text(&text, &sentences)
                .iter()
                .any(|m| m.rule().id() == "TOO_LONG_SENTENCE"),
            "LongSentenceRule(40) fires on a 41-word sentence"
        );
        let short = "En kort svensk mening om ord.".to_string();
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(&short);
        assert!(long_sentence.match_text(&short, &sentences).is_empty());
    }

    #[test]
    fn compound_rule_pins_the_java_test_corpus() {
        let rule = swedish_compound_rule();
        let pipeline = crate::sv::example_suite::sv_pipeline();
        let check = |text: &str| {
            let sentence = pipeline.analyze_range(text, 0, text.len());
            rule.match_sentence(&sentence)
        };
        for clean in ["IP-Adress", "moll-tonart", "e-mail"] {
            let text = format!("Vi använder {clean} idag.");
            assert!(
                check(&text).is_empty(),
                "'{clean}' is accepted (CompoundRuleTest)"
            );
        }
        let matches = check("Vi använder skit bra idag.");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "skitbra")
        );
        let matches = check("Vi använder skit-bra idag.");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "skitbra")
        );
        let matches = check("Vi använder IP Adress idag.");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "IP-Adress")
        );
        let matches = check("Vi använder moll tonart idag.");
        assert_eq!(matches.len(), 1);
        let offered: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(offered.contains(&"moll-tonart") && offered.contains(&"molltonart"));
        let matches = check("Vi använder e mail idag.");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "e-mail")
        );
    }

    #[test]
    fn coherency_rule_pins_the_java_message_and_data() {
        let rule = swedish_word_coherency_rule();
        let text = "Han skickade ett e-mejl. Sedan skickade han ett e-mail.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1, "a variant pair fires once");
        assert!(
            matches[0]
                .message()
                .contains("Använd endast en av stavningsvarianterna"),
            "{}",
            matches[0].message()
        );
        assert!(
            matches[0].message().contains("e-mejl") && matches[0].message().contains("e-mail"),
            "{}",
            matches[0].message()
        );
        let text = "Han skickade ett e-mejl. Sedan skickade han ett till e-mejl.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn unpaired_brackets_carry_the_java_default_five_pairs() {
        let rule = swedish_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        let text = "Han sa 'hej.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        assert_eq!(
            rule.match_text(text, &sentences)
                .iter()
                .filter(|m| &text[m.offset()..m.offset() + m.length()] == "'")
                .count(),
            1,
            "the unclosed straight quote flags once (Java GenericUnpairedBracketsRule default)"
        );
        let text = "En (parentes sluten aldrig.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        assert!(
            rule.match_text(text, &sentences)
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "(")
        );
        let text = "En (parentes) sluten.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
        let text = "Han sa 'hej'.";
        let sentences = crate::sv::example_suite::sv_pipeline().analyze_text(text);
        let quote_flags = rule
            .match_text(text, &sentences)
            .iter()
            .filter(|m| &text[m.offset()..m.offset() + m.length()] == "'")
            .count();
        assert_eq!(
            quote_flags, 2,
            "known shared-engine divergence: a symbol at the same index in both symbol lists is \
             always-opening, so balanced straight quotes flag twice"
        );
    }

    #[test]
    fn speller_rule_uses_the_swedish_wordlist() {
        let rule = swedish_speller_rule();
        assert_eq!(rule.id(), SV_SPELLER);
        assert!(rule.is_known_word("klubba"));
        assert!(!rule.is_known_word("xqzvjq"));
    }

    #[test]
    fn normalize_sv_rules_xml_strips_only_possessive_quantifiers() {
        assert!(matches!(
            normalize_sv_rules_xml("<token regexp='yes'>abc</token>"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(
            normalize_sv_rules_xml(r"\p{Lu}{2}+[i]*x{1,4}+ och \p{Lu}+").as_ref(),
            r"\p{Lu}{2}[i]*x{1,4} och \p{Lu}+",
            "Java possessive {{n}}+/{{n,m}}+ become their greedy bounds while plain + survives"
        );
    }

    #[test]
    fn swedish_tagger_pins_the_java_reading_corpus() {
        let tagger = swedish_tagger().expect("swedish.dict tagger");
        use og_core::checker::Tagger as _;
        let tag = |word: &str| -> Vec<(Option<String>, String)> {
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
                .map(|r| (r.lemma().map(str::to_string), r.pos_tags().join(":")))
                .collect()
        };
        let fn_readings = tag("FN");
        assert!(
            fn_readings
                .iter()
                .any(|(lemma, pos)| lemma.as_deref() == Some("FN") && pos == "PM:NOM:ACR"),
            "FN/[FN]PM:NOM:ACR (SwedishTaggerTest): {fn_readings:?}"
        );
        let masarnas = tag("masarnas");
        assert!(
            masarnas
                .iter()
                .any(|(lemma, pos)| lemma.as_deref() == Some("mas") && pos == "NN:BF:PLU:GEN:UTR"),
            "masarnas/[mas]NN:BF:PLU:GEN:UTR (SwedishTaggerTest): {masarnas:?}"
        );
        let det = tag("Det");
        assert!(
            det.iter()
                .any(|(lemma, pos)| lemma.as_deref() == Some("det") && pos == "PN"),
            "Det/[det]PN (SwedishTaggerTest): {det:?}"
        );
    }
}
