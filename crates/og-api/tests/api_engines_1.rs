mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_french_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=fr&text=Bonjour+le+monde.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("fr"));
}

#[tokio::test]
async fn test_v2_check_italian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=it&text=Buongiorno+il+mondo.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("it"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_arabic_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ar&text=%D9%85%D8%B1%D8%AD%D8%A8%D8%A7+%D8%A8%D8%A7%D9%84%D8%B9%D8%A7%D9%84%D9%85.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ar"));
    assert!(json["matches"].is_array());
}

#[tokio::test]
async fn test_v2_check_asturian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ast&text=La+casa+ye+grande.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ast"));
    assert!(json["matches"].is_array());
}
