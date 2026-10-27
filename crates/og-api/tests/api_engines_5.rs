mod common;

use axum::body::Body;
use common::app;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_v2_check_khmer_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=km&text=%E1%9E%9F%E1%9F%86%E1%9E%9A%E1%9E%B6%E1%9E%94%E1%9F%8B%E1%9F%94")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("km"));
    assert!(json["matches"].is_array());
    let replace: Vec<&serde_json::Value> = json["matches"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .filter(|m| m["rule"]["id"] == "KM_SIMPLE_REPLACE")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        replace.len(),
        1,
        "KhmerSimpleReplaceRule must flag the coherency.txt key 'សំរាប់', got {json:?}"
    );
    assert!(
        replace[0]["replacements"]
            .as_array()
            .is_some_and(|rs| rs.iter().any(|r| r["value"] == "សម្រាប់")),
        "coherency.txt:1260 maps សំរាប់ to សម្រាប់, got {json:?}"
    );
}

#[tokio::test]
async fn test_v2_check_lithuanian_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=lt&text=Jaroslavas%20pajuto%20kad%20jo%20geriausias%20draugas%20yra%20Mantas.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("lt"));
    assert!(json["matches"].is_array());
    let flagged: Vec<&str> = json["matches"]
        .as_array()
        .map(|ms| ms.iter().filter_map(|m| m["rule"]["id"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        flagged.contains(&"BRAK_PRZECINKA_ZE"),
        "the lt grammar rule must fire on the missing comma before 'kad' through the engine \
         pipeline, got {flagged:?}"
    );
    assert!(
        !flagged.contains(&"MORFOLOGIK_RULE_LT_LT"),
        "no lt dictionary ships in the checkout, so no speller may fire, got {flagged:?}"
    );
}

#[tokio::test]
async fn test_v2_check_malayalam_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v2/check?language=ml&text=Aagohw.")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("ml"));
    assert!(json["matches"].is_array());
    let speller: Vec<&serde_json::Value> = json["matches"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .filter(|m| m["rule"]["id"] == "MORFOLOGIK_RULE_ML_IN")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        speller.len(),
        1,
        "MorfologikMalayalamSpellerRule over hunspell/ml_IN.dict must flag the non-word 'Aagohw', \
         got {json:?}"
    );
}

#[tokio::test]
async fn test_v2_check_slovak_engine_registered() {
    let app = app();
    let response = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v2/check?language=sk&text=Toto%20je%20%C4%8Desko%20slovensk%C3%BD%20projekt.",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["language"]["code"].as_str(), Some("sk"));
    assert!(json["matches"].is_array());
    let compound: Vec<&serde_json::Value> = json["matches"]
        .as_array()
        .map(|ms| {
            ms.iter()
                .filter(|m| m["rule"]["id"] == "SK_COMPOUNDS")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        compound.len(),
        1,
        "SK_COMPOUNDS must flag the spaced 'česko slovenský', got {json:?}"
    );
    assert!(
        compound[0]["replacements"]
            .as_array()
            .is_some_and(|rs| rs.iter().any(|r| r["value"] == "česko-slovenský")),
        "the spaced form must suggest the hyphenated form, got {json:?}"
    );
}
