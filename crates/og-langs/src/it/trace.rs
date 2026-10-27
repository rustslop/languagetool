use crate::engine::LanguageEngine;

fn check_words(engine: &LanguageEngine, text: &str) -> Vec<String> {
    engine
        .check(text)
        .matches
        .iter()
        .map(|m| {
            let start = m.offset();
            format!(
                "[{} {}..{} {:?}]",
                m.rule().id(),
                start,
                start + m.length(),
                m.replacements()
                    .iter()
                    .map(|r| r.value())
                    .collect::<Vec<_>>()
            )
        })
        .collect()
}

#[test]
#[ignore]
fn probe_demo_text_languagetool_speller_fp() {
    let _release_guard = crate::release_guard();
    let engine = LanguageEngine::for_code("it").expect("it engine");
    let text = "Inserite qui il vostro testo... Oppure controlate direttamente questo ed avrete un assaggio di quali errori possono essere identificati con LanguageTool.";
    println!("demo text matches: {:?}", check_words(&engine, text));
    for word in [
        "LanguageTool",
        "LanguageTooler",
        "languagetool",
        "Languagetool",
    ] {
        println!(
            "{word}: engine matches {:?}",
            check_words(&engine, &format!("Nel testo compare {word} due volte."))
        );
    }
    println!(
        "Java MorfologikSpeller.isMisspelled (MorfologikSpeller.java:98-101) returns false \
         for exactly 'LanguageTool' and 'LanguageTooler'; og-spell SpellingCheckRule::is_known_word \
         (crates/og-spell/src/speller.rs:75) has no such exception, so the engine flags \
         'LanguageTool' at 140..152 where ItalianTest.java:35-40 expects no third match"
    );
}

#[test]
#[ignore]
fn probe_speller_suggestions_lack_morfologik_candidates() {
    let _release_guard = crate::release_guard();
    let engine = LanguageEngine::for_code("it").expect("it engine");
    println!(
        "'esmpio': {:?}",
        check_words(&engine, "La parola esmpio qui.")
    );
    println!("'esmpio' alone: {:?}", check_words(&engine, "esmpio"));
    let resource_dir = super::example_suite::it_resource_dir().expect("it resource dir");
    let view = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "italian")
        .expect("italian.dict loads");
    let dict = og_spell::Dictionary::from_shared_words(std::sync::Arc::new(
        og_spell::morfologik_view::MorfologikView::new(view),
    ));
    let rule = og_spell::SpellingCheckRule::new_shared(
        std::sync::Arc::new(dict),
        "MORFOLOGIK_RULE_IT_IT",
        Default::default(),
    );
    println!(
        "dictionary word_list len: {} (Dictionary::word_list at crates/og-spell/src/dictionary.rs:115 \
         returns only add_word entries, never the shared morfologik view)",
        rule.shared_dictionary().word_list().len()
    );
    println!(
        "suggestions for 'esmpio': {:?}",
        rule.get_suggestions("esmpio", 5)
    );
    println!(
        "misspelled('esmpio')={}, known('esempio')={}",
        !rule.is_known_word("esmpio"),
        rule.is_known_word("esempio")
    );
    println!(
        "Java MorfologikItalianSpellerRuleTest.java:35-39 expects 1 match on 'esmpio' with \
         suggestions [empio, esempio, espio, espiò]; crate::it::speller::order_suggestions is the \
         Italian ordering delta (MorfologikItalianSpellerRule.java:46-63) and must be wired once \
         og-spell produces candidates from morfologik dictionaries"
    );
}
