mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_ukrainian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=uk&text=%D0%97%D0%BD%D0%B0%D0%B9%D1%82%D0%B8%20%D0%B2%D1%81%D1%96%20%D0%BF%D0%BE%D0%BC%D0%B8%D0%BB%D0%BA%D0%B8%20%D0%B4%D0%BB%D1%8F%20LanguageTool%20%D1%94%20%D0%BD%D0%B5%20%D0%BF%D0%BE%20%D1%81%D0%B8%D0%BB%D0%B0%D1%85%20%D0%B7%20%D0%B1%D0%B0%D0%B3%D0%B0%D1%82%D1%8C%D0%BE%D1%85%20%D0%BF%D1%80%D0%B8%D1%87%D0%B8%D0%BD%20%D0%B0%D0%BB%D0%B5%20%D0%B4%D0%B5%D1%89%D0%BE%20%D0%B2%D1%96%D0%BD%20%D0%B2%D0%B0%D0%BC%20%D0%B2%D1%81%D0%B5%20%D1%82%D0%B0%D0%BA%D0%B8%20%D0%BF%D1%96%D0%B4%D0%BA%D0%B0%D0%B6%D0%B5.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("uk"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    for expected in ["PO_VS_OTHER", "comma_before_but", "WORDS_WITH_DASH"] {
        assert!(
            flagged.contains(&expected),
            "expected {expected} from the UkrainianTest oracle through the engine pipeline \
             (comma_before_but proves grammar-grammar.xml, WORDS_WITH_DASH proves the extras), \
             got {flagged:?}"
        );
    }
}

#[tokio::test]
async fn test_v2_check_chinese_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=zh&text=%E5%B0%86%E6%96%87%E6%9C%AC%E7%B2%98%E8%B4%B4%E5%9C%A8%E6%AD%A4%EF%BC%8C%E6%88%96%E8%80%85%E6%A3%80%E6%B5%8B%E4%BB%A5%E4%B8%8B%E6%96%87%E6%9C%AC%EF%BC%9A%E6%88%91%E5%92%8C%E5%A5%B9%E5%8E%BB%E7%9C%8B%E4%BA%86%E4%BA%8C%E9%83%A8%E7%94%B5%E5%BD%B1%E3%80%82")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("zh"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        flagged,
        vec!["wa5"],
        "ChineseTest.testLanguage expects exactly the wa5 match on '二部', got {flagged:?}"
    );
    let replacements: Vec<&str> = json["matches"][0]["replacements"]
        .as_array()
        .map(|rs| rs.iter().filter_map(|r| r["value"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        replacements.contains(&"两部"),
        "wa5 must suggest '两部', got {replacements:?}"
    );
}
