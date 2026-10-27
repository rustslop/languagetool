mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_languages_lists_only_implementable_languages() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/languages")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(!json.is_empty());
    assert!(json.iter().any(|l| l["longCode"] == "en-US"));
    assert!(json.iter().any(|l| l["longCode"] == "en-GB"));
    assert!(
        json.iter()
            .any(|l| l["code"] == "en" && l["longCode"] == "en-US")
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "de" && l["longCode"] == "de-DE"),
        "/v2/languages must advertise the implemented German engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "fr" && l["longCode"] == "fr"),
        "/v2/languages must advertise the implemented French engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "es" && l["longCode"] == "es"),
        "/v2/languages must advertise the implemented Spanish engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "pt" && l["longCode"] == "pt-PT"),
        "/v2/languages must advertise the implemented Portuguese engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "pt" && l["longCode"] == "pt-BR"),
        "/v2/languages must advertise the implemented pt-BR engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "ru" && l["longCode"] == "ru"),
        "/v2/languages must advertise the implemented Russian engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "nl" && l["longCode"] == "nl"),
        "/v2/languages must advertise the implemented Dutch engine"
    );
    assert!(
        json.iter()
            .any(|l| l["code"] == "ca" && l["longCode"] == "ca"),
        "/v2/languages must advertise the implemented Catalan engine"
    );
}

#[tokio::test]
async fn test_v2_version() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["name"], "OpenGrammar");
    assert_eq!(json["apiVersion"], 2);
}

#[tokio::test]
async fn test_v2_languages_lists_all_registered_engines() {
    let json = common::check_response("/v2/languages").await;
    let codes: Vec<&str> = json
        .as_array()
        .map(|langs| {
            langs
                .iter()
                .filter_map(|l| l["longCode"].as_str())
                .collect()
        })
        .unwrap_or_default();
    for code in [
        "en-US", "de-DE", "fr", "es", "pt-BR", "pt-PT", "pt-AO", "pt-MZ", "nl", "pl", "ru", "it",
        "ar", "ast", "be", "br", "ca", "crh", "da", "el", "eo", "fa", "ga", "gl", "is", "ja", "km",
        "lt", "ml", "ro", "sk", "sl", "sr", "sv", "ta", "tl", "uk", "zh",
    ] {
        assert!(codes.contains(&code), "missing {code} in {codes:?}");
    }
}
