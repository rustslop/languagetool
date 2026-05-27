use og_api::server::{AppState, build_app};
use axum::body::Body;
use http_body_util::BodyExt;
use http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

fn app() -> axum::Router {
    let engines = og_api::build_engines();
    let state = Arc::new(AppState {
        engines: Arc::new(engines),
    });
    build_app(state)
}

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
async fn test_v2_languages() {
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
    assert!(json.iter().any(|l| l["longCode"] == "de-DE"));
    assert!(json.iter().any(|l| l["code"] == "en" && l["longCode"] == "en-US"));
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

    // Verify LT-compatible response structure
    assert!(json["software"].is_object());
    assert!(json["software"]["name"].is_string());
    assert!(json["software"]["version"].is_string());
    assert!(json["software"]["apiVersion"].is_number());
    assert!(json["language"].is_object());
    assert!(json["language"]["code"].is_string());
    assert!(json["language"]["detected"].is_object());
    assert!(json["language"]["detected"]["code"].is_string());
    assert!(json["language"]["detected"]["name"].is_string());
    assert!(json["matches"].is_array());
    assert!(json["warnings"].is_object());
}
