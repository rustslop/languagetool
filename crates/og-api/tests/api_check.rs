mod common;

use axum::body::Body;
use common::{app, check_response, has_match_with_rule};
use http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_returns_json() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=en-US&text=Hello+world")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["software"]["name"], "OpenGrammar");
    assert_eq!(json["language"]["code"], "en-US");
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_missing_text() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=en-US")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_v2_check_missing_language() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?text=Hello")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_v2_check_post_form_body() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v2/check")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("language=en-US&text=I+have+a+apple+here."))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["matches"].is_array());
    assert!(has_match_with_rule(&json, "EN_A_VS_AN"));
}

#[tokio::test]
async fn test_v2_check_post_query_params_override_body() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v2/check?text=I+have+a+apple+here.")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("language=en-US&text=ignored+text"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(has_match_with_rule(&json, "EN_A_VS_AN"));
}

#[tokio::test]
async fn test_v2_check_post_form_body_without_text_is_rejected() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v2/check?language=en-US")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_v2_check_spelling_match_is_unknown_word_type() {
    let json = check_response("/v2/check?language=en-US&text=I+have+one+appple+here.").await;
    let spelling = json["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["rule"]["id"] == "MORFOLOGIK_RULE_EN_US")
        .expect("spelling rule should fire on 'appple'");
    assert_eq!(spelling["type"]["typeName"], "UnknownWord");
    assert_eq!(spelling["rule"]["issueType"], "misspelling");
}

#[tokio::test]
async fn test_v2_check_style_match_is_hint_type() {
    let json =
        check_response("/v2/check?language=en-US&text=This+is+absolutely+essential+work.").await;
    let m = json["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["rule"]["id"] == "EN_REDUNDANCY")
        .expect("redundancy rule should fire on 'absolutely essential'");
    assert_eq!(m["type"]["typeName"], "Other");
    assert_eq!(m["rule"]["issueType"], "style");
}

#[tokio::test]
async fn test_v2_check_disabled_rules_suppress_matches() {
    let baseline = check_response("/v2/check?language=en-US&text=I+have+a+apple+here.").await;
    assert!(
        has_match_with_rule(&baseline, "EN_A_VS_AN"),
        "baseline should flag 'a apple'"
    );
    let m = baseline["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["rule"]["id"] == "EN_A_VS_AN")
        .unwrap();
    assert!(m["type"].is_object());
    assert_eq!(m["type"]["typeName"], "Other");
    assert_eq!(m["rule"]["category"]["id"], "MISC");
    assert_eq!(m["rule"]["category"]["name"], "Miscellaneous");
    assert!(m["rule"]["category"].get("description").is_none());
    assert_eq!(m["rule"]["issueType"], "misspelling");

    let filtered = check_response(
        "/v2/check?language=en-US&text=I+have+a+apple+here.&disabledRules=EN_A_VS_AN",
    )
    .await;
    assert!(
        !has_match_with_rule(&filtered, "EN_A_VS_AN"),
        "disabledRules must suppress the rule"
    );
}

#[tokio::test]
async fn test_v2_check_level_picky_does_not_activate_default_off_rules() {
    let text = "An+actress%E2%80%99s+role";
    let picky = check_response(&format!("/v2/check?language=en-US&text={text}&level=picky")).await;
    assert!(
        !has_match_with_rule(&picky, "TYPEWRITER_APOSTROPHE"),
        "Java core never auto-enables default-off rules at picky level (JLanguageTool.ignoreRule requires enabledRules)"
    );
    let enabled = check_response(&format!(
        "/v2/check?language=en-US&text={text}&level=picky&enabledRules=TYPEWRITER_APOSTROPHE"
    ))
    .await;
    assert!(
        has_match_with_rule(&enabled, "TYPEWRITER_APOSTROPHE"),
        "enabledRules must activate the default-off picky-tagged rule"
    );
}

#[tokio::test]
async fn test_v2_check_enabled_categories_restrict_matches() {
    let filtered = check_response(
        "/v2/check?language=en-US&text=I+have+a+apple+here.&enabledCategories=GRAMMAR",
    )
    .await;
    assert!(
        !has_match_with_rule(&filtered, "EN_A_VS_AN"),
        "EN_A_VS_AN is in category MISC and must not run with enabledCategories=GRAMMAR"
    );

    let kept =
        check_response("/v2/check?language=en-US&text=I+have+a+apple+here.&enabledCategories=MISC")
            .await;
    assert!(
        has_match_with_rule(&kept, "EN_A_VS_AN"),
        "enabledCategories=MISC must keep the rule"
    );
}

#[tokio::test]
async fn test_v2_check_level_and_mother_tongue_accepted() {
    let json = check_response(
        "/v2/check?language=en-US&text=Hello+world&level=picky&motherTongue=de&picky=true",
    )
    .await;
    assert!(json["matches"].is_array());
    assert_eq!(json["language"]["code"], "en-US");
}

#[tokio::test]
async fn test_v2_check_response_shape() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=en-US&text=This+is+a+test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["software"].is_object());
    assert!(json["software"]["name"].is_string());
    assert!(json["software"]["version"].is_string());
    assert_eq!(json["software"]["apiVersion"], 1);
    assert!(json["language"].is_object());
    assert!(json["language"]["code"].is_string());
    assert!(json["language"]["detectedLanguage"].is_object());
    assert!(json["language"]["detectedLanguage"]["code"].is_string());
    assert!(json["language"]["detectedLanguage"]["name"].is_string());
    assert!(json["language"]["detectedLanguage"]["confidence"].is_number());
    assert!(json["language"].get("detected").is_none());
    assert!(json["matches"].is_array());
    assert_eq!(json["warnings"]["incompleteResults"], false);
}

#[tokio::test]
async fn test_v2_check_auto_detects_german() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=auto&text=Der+schnelle+braune+Fuchs+springt+%C3%BCber+den+faulen+Hund+und+rennt+schnell+davon.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["language"]["detectedLanguage"]["code"].as_str(),
        Some("de-DE"),
        "expected German detection: {json}"
    );
    assert!(
        json["language"]["detectedLanguage"]["confidence"]
            .as_f64()
            .unwrap_or(0.0)
            > 0.9
    );
}

#[tokio::test]
async fn test_v2_check_auto_detects_english() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=auto&text=The+quick+brown+fox+jumps+over+the+lazy+dog+and+then+it+runs+away+quickly.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["language"]["detectedLanguage"]["code"].as_str(),
        Some("en-US"),
        "expected English detection: {json}"
    );
}

#[tokio::test]
async fn test_v2_check_variant_code_falls_back_to_base_engine() {
    let json = check_response("/v2/check?language=nl-BE&text=Hallo+wereld.").await;
    assert_eq!(json["language"]["code"].as_str(), Some("nl-BE"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_simple_german_code_falls_back_to_the_german_engine() {
    let json = check_response(
        "/v2/check?language=de-DE-x-simple-language&text=Ich+habe+ein+Fehler+gemachtt.",
    )
    .await;
    assert_eq!(
        json["language"]["code"].as_str(),
        Some("de-DE-x-simple-language")
    );
    assert!(
        has_match_with_rule(&json, "MORFOLOGIK_RULE_DE_DE"),
        "the base German speller must fire on 'gemachtt' via the short-code fallback: {json}"
    );
}
