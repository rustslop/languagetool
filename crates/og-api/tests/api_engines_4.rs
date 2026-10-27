mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_irish_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ga&text=Seo%20dh%C3%A1%20ab%20d%C3%A9ag.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ga"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"GA_DHA_NO_BEIRT"),
        "DhaNoBeirtRuleTest.java:35 expects GA_DHA_NO_BEIRT on 'dhá ab déag', got {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_galician_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=gl&text=O+problema+%C3%A9+acuciante.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("gl"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"GL_CAST_WORDS"),
        "GalicianSimpleReplaceRule must flag the castelanismo 'acuciante', got {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_icelandic_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=is&text=%C3%9Eetta+er+qvist+setning.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("is"));
    assert!(json["matches"].is_array());
    let speller: Vec<&serde_json::Value> = json["matches"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .filter(|m| m["rule"]["id"] == "HUNSPELL_NO_SUGGEST_RULE")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        speller.len(),
        1,
        "HunspellNoSuggestionRule must flag the non-word 'qvist', got {json:?}"
    );
    assert!(
        speller[0]["replacements"]
            .as_array()
            .is_none_or(|r| r.is_empty()),
        "HunspellNoSuggestionRule.java:73-76 never offers replacements"
    );
}

#[tokio::test]
async fn test_v2_check_japanese_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ja&text=%E3%81%97%E3%81%9F%E3%81%8C%E3%81%A3%E3%81%A6%E3%80%82")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ja"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"SITAGATTE_END"),
        "SITAGATTE_END = [したがって][SENT_END] must fire through the engine pipeline, got \
         {flagged:?}"
    );
}
