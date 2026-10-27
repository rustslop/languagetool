mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_belarusian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=be&text=%D0%9F%D1%80%D1%8B%D0%B2%D1%96%D1%82%D0%B0%D0%BD%D0%BD%D0%B5+%D1%81%D1%83%D1%81%D0%B2%D0%B5%D1%82.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("be"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_breton_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=br&text=Lakait+aman%C3%B1+ho+testenn.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("br"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_catalan_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ca&text=La+casa+%C3%A9s+gran.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ca"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_crimean_tatar_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=crh&text=%C3%96z%C3%BC+bir+%C3%A7aqr%C4%B1m+uzaqta%2C+demiryol+ke%C3%A7idi+yan%C4%B1nda+ya%C5%9Fay.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("crh"));
    assert!(json["matches"].is_array());
}
