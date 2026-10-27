use og_core::rule::Rule as _;
use std::sync::Arc;

#[test]
fn registry_exposes_the_language_set_by_full_code() {
    let registry = og_langs::registry::LanguageRegistry::new();
    let codes = registry.supported_codes();
    assert!(!codes.is_empty());
    assert!(codes.contains(&"en-US"));
    let en = registry.get_language("en-US").expect("en-US present");
    assert_eq!(en.name(), "English (US)");
    assert!(registry.get_language("xx-XX").is_none());
    assert_eq!(registry.all_languages().len(), codes.len());
}

#[test]
fn resource_dir_helpers_locate_the_languagetool_checkout() {
    let root = og_langs::engine::find_lt_resource_dir().expect("lt resource dir");
    assert!(
        root.join("en/src/main/resources/org/languagetool/resource/en")
            .exists(),
        "unexpected root {root:?}"
    );
    let core = og_langs::engine::find_lt_core_resource_dir().expect("core resource dir");
    assert!(
        core.ends_with("org/languagetool/resource") && core.exists(),
        "unexpected core dir {core:?}"
    );
}

#[test]
fn language_engine_checks_text_with_an_added_rule() {
    let mut engine = og_langs::engine::LanguageEngine::new(og_core::Language::English);
    let mut rule = og_langs::en::avsan::AvsAnRule::new();
    rule.load_data("euphemism\neuropean\nunicorn\n", "hour\nheir\n8\n");
    assert_eq!(rule.id(), "EN_A_VS_AN");
    engine.add_rule(Arc::new(rule));
    assert_eq!(engine.rule_count(), 1);
    assert_eq!(engine.rule_ids(), vec!["EN_A_VS_AN"]);
    assert!(engine.text_level_rule_ids().is_empty());
    assert_eq!(engine.language().code(), "en-US");

    let result = engine.check("a elephant");
    let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
    assert_eq!(ids, vec!["EN_A_VS_AN"], "a before vowel must flag");
    let m = &result.matches[0];
    assert_eq!(&"a elephant"[m.offset()..m.offset() + m.length()], "a");
    assert_eq!(m.replacements()[0].value(), "an");

    let clean = engine.check("an elephant");
    assert!(
        clean.matches.iter().all(|m| m.rule().id() != "EN_A_VS_AN"),
        "an before vowel must stay clean"
    );

    let repeat: Arc<dyn og_core::rule::Rule> =
        Arc::new(og_langs::en::word_repeat::EnglishWordRepeatRule::new());
    engine.add_rules(vec![repeat]);
    assert_eq!(engine.rule_count(), 2);
    let repeated = engine.check("I saw the the elephant.");
    let ids: Vec<&str> = repeated.matches.iter().map(|m| m.rule().id()).collect();
    assert!(
        ids.contains(&"ENGLISH_WORD_REPEAT_RULE"),
        "doubled word must flag, got {ids:?}"
    );
}
