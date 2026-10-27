use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use og_api::server::{build_app, shared_state};
use serde_json::Value;
use tower::ServiceExt;

#[allow(dead_code)]
pub fn app() -> axum::Router {
    build_app(shared_state())
}

#[allow(dead_code)]
pub async fn check_response(uri: &str) -> Value {
    let response = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[allow(dead_code)]
pub fn has_match_with_rule(json: &Value, rule_id: &str) -> bool {
    json["matches"]
        .as_array()
        .map(|matches| {
            matches
                .iter()
                .any(|m| m["rule"]["id"].as_str() == Some(rule_id))
        })
        .unwrap_or(false)
}
