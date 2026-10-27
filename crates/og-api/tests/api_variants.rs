mod common;

use common::check_response;

#[tokio::test]
async fn test_v2_check_english_gb_variant_uses_gb_speller() {
    let json = check_response("/v2/check?language=en-GB&text=I+have+one+appple+here.").await;
    assert_eq!(json["language"]["code"].as_str(), Some("en-GB"));
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"MORFOLOGIK_RULE_EN_GB"),
        "en-GB must swap in its own speller rule, got {flagged:?}"
    );
    assert!(
        !flagged.contains(&"MORFOLOGIK_RULE_EN_US"),
        "the US speller must not fire on the en-GB engine, got {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_german_at_variant_uses_austrian_speller() {
    let json = check_response("/v2/check?language=de-AT&text=Ich+habe+einn+Fehler.").await;
    assert_eq!(json["language"]["code"].as_str(), Some("de-AT"));
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"AUSTRIAN_GERMAN_SPELLER_RULE"),
        "de-AT must swap in its own speller rule on the non-word 'einn', got {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_portuguese_variants_build_their_own_engines() {
    let br = check_response("/v2/check?language=pt-BR&text=Eu+tenho+umma+casa.").await;
    assert_eq!(br["language"]["code"].as_str(), Some("pt-BR"));
    assert!(br["matches"].as_array().is_some_and(|m| !m.is_empty()));

    let pt = check_response("/v2/check?language=pt&text=Eu+tenho+umma+casa.").await;
    assert_eq!(pt["language"]["code"].as_str(), Some("pt"));
    assert!(pt["matches"].as_array().is_some_and(|m| !m.is_empty()));
}
