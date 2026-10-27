mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_danish_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=da&text=Inds%C3%A6t+din+egen+tekst+her.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("da"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_greek_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v2/check?language=el&text=%CE%91%CF%85%CF%84%CE%AE+%CE%B5%CE%AF%CE%BD%CE%B1%\
                     CE%B9+%CE%BC%CE%B9%CE%B1+%CE%B4%CE%BF%CE%BA%CE%B9%CE%BC%CE%AE.",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("el"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_esperanto_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=eo&text=La+Mondaj+Ludoj+de+2013+estis+plur-sporta+evento.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("eo"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"HUNSPELL_RULE"),
        "EsperantoTest.java:41-44 expects the hunspell speller to flag 'plur-sporta', got \
         {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_persian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v2/check?language=fa&text=%D9%88%DB%8C+%D8%AD%D8%A7%D8%B8%D8%B1+%D8%A8%D9%87\
                     +%D9%87%D9%85%DA%A9%D8%A7%D8%B1%DB%8C+%D8%B4%D8%AF.",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("fa"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"FA_SIMPLE_REPLACE"),
        "SimpleReplaceRule must flag 'حاظر', got {flagged:?}"
    );
}
