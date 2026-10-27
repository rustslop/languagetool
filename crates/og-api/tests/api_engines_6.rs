mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_slovenian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=sl&text=Kupil%20je%20npr.%20jajca%2C%20moko%20in%20mleko.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("sl"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        flagged,
        vec!["MORFOLOGIK_RULE_SL_SI"],
        "JLanguageToolTest.testSlovenian expects exactly 1 match: the sl_SI speller flags 'npr' \
         (the dict stores 'npr.') while the Slovenian SRX ruleset keeps the sentence unsplit, got \
         {json:?}"
    );
}

#[tokio::test]
async fn test_v2_check_serbian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v2/check?language=sr&text=%D0%A2%D0%BE%20%D1%98%D0%B5%20%D0%B1%D0%B8%D0%BB%D0%BE%2031.11.%20%D0%A1%D0%B8%D0%BD%20%D1%98%D0%B5%20%D0%B2%D0%B8%D1%88%D1%99%D0%B8%20%D0%BE%D0%B4%20%D0%BE%D1%86%D0%B0.",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("sr"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"INVALID_DATE"),
        "INVALID_DATE from grammar-logical.xml must fire on '31.11.' through the engine pipeline, \
         got {flagged:?}"
    );
    let replace: Vec<&serde_json::Value> = json["matches"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .filter(|m| m["rule"]["id"] == "SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        replace.len(),
        1,
        "the ekavian grammar-replace native must flag 'вишљи', got {json:?}"
    );
    assert!(
        replace[0]["replacements"]
            .as_array()
            .is_some_and(|rs| rs.iter().any(|r| r["value"] == "виши")),
        "SimpleGrammarEkavianReplaceRuleTest pins вишљи -> виши, got {json:?}"
    );
}

#[tokio::test]
async fn test_v2_check_tamil_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ta&text=%E0%AE%87%E0%AE%A8%E0%AF%8D%E0%AE%A4%20%E0%AE%AA%E0%AF%86%E0%AE%9F%E0%AF%8D%E0%AE%9F%E0%AE%BF%E0%AE%AF%E0%AE%BF%E0%AE%B2%E0%AF%8D%20%E0%AE%89%E0%AE%99%E0%AF%8D%E0%AE%95%E0%AE%B3%E0%AF%8D%20%E0%AE%89%E0%AE%B0%E0%AF%88%E0%AE%AF%E0%AF%88%20%E0%AE%92%E0%AE%9F%E0%AF%8D%E0%AE%9F%E0%AE%BF%20%E0%AE%9A%E0%AE%B0%E0%AE%BF%E0%AE%B5%E0%AE%B0%20%E0%AE%9A%E0%AF%8B%E0%AE%A4%E0%AE%BF%E0%AE%95%E0%AF%8D%E0%AE%95%E0%AE%BF%E0%AE%B1%E0%AE%A4%E0%AE%BE%20%E0%AE%8E%E0%AE%A9%20%E0%AE%AA%E0%AE%BE%E0%AE%B0%E0%AF%81%E0%AE%99%E0%AF%8D%E0%AE%95%E0%AE%B3%E0%AF%8D.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ta"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"Ends_in_A_1"),
        "Ends_in_A_1 must fire on the TamilTest demo text through the engine pipeline, got \
         {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_tagalog_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=tl&text=Ang%20LanguageTool%20ay%20maganda%20gamit%20sa%20araw-araw.%20Ang%20talatang%20ito%20ay%20nagpapakita%20ng%20ng%20kakayahan%20ng%20LanguageTool%20at%20hinahalimbawa%20kung%20paano%20ito%20gamitin.%20Litaw%20rin%20sa%20talatang%20ito%20na%20may%20mga%20bagaybagay%20na%20hindii%20pa%20kayang%20itama%20nng%20LanguageTool.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("tl"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        flagged,
        vec![
            "ADJECTIVE-V_COMMON_NOUN",
            "NG_NG",
            "MORFOLOGIK_RULE_TL",
            "R_WORDS",
            "MORFOLOGIK_RULE_TL",
            "MORFOLOGIK_RULE_TL",
            "MORFOLOGIK_RULE_TL",
        ],
        "TagalogTest.java:33-37 testDemoText pins exactly these seven matches in this order, got \
         {flagged:?}"
    );
}
