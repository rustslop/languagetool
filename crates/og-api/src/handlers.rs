use crate::request::MergedCheckParams;
use crate::response::{InfoResponse, LanguagesResponse, MaxTextLengthResponse, VersionResponse};
use crate::server::{AppState, MAX_TEXT_LENGTH};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use og_core::{CheckRequest, CheckResult, Language};
use std::sync::Arc;

pub async fn handle_check(
    State(state): State<Arc<AppState>>,
    MergedCheckParams(params): MergedCheckParams,
) -> Result<Json<CheckResult>, (StatusCode, Json<serde_json::Value>)> {
    let text = match params.get_text() {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing 'text' parameter"})),
            ));
        }
    };

    if text.len() > MAX_TEXT_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Text is too long. Maximum length is {} characters.", MAX_TEXT_LENGTH)
            })),
        ));
    }

    let auto_requested = params.language.eq_ignore_ascii_case("auto");
    let language = match params.get_language() {
        Some(l) => l,
        None if auto_requested => Language::English,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing or invalid 'language' parameter"})),
            ));
        }
    };

    let (language, detected) = if auto_requested {
        match detect_language(text) {
            Some((lang, confidence)) => {
                let detected = (lang.clone(), confidence);
                (lang, Some(detected))
            }
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Language detection failed for 'auto'; specify an explicit 'language' parameter"
                    })),
                ));
            }
        }
    } else {
        (language, None)
    };

    let engine = if let Some(e) = state.engine_for(language.code()) {
        e
    } else if let Some(e) = state.engine_for(language.short_code()) {
        e
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported language: {}", language.code())
            })),
        ));
    };

    let mut request = CheckRequest::new(text, language);
    request.enabled_rules = params.get_enabled_rules();
    request.disabled_rules = params.get_disabled_rules();
    request.enabled_categories = params.get_enabled_categories();
    request.disabled_categories = params.get_disabled_categories();
    request.level = params.level.clone();
    request.picky = params.picky;
    request.mother_tongue = params
        .mother_tongue
        .as_deref()
        .and_then(Language::from_code);

    let mut result = engine.check_with_request(&request);
    if let Some((lang, confidence)) = detected {
        result.language.detected.code = lang.code().to_string();
        result.language.detected.name = lang.name().to_string();
        result.language.detected.confidence = confidence;
    }
    Ok(Json(result))
}

fn detect_language(text: &str) -> Option<(Language, f64)> {
    let detector = og_langs::detection::LanguageDetector::shared()?;
    let supported = ["en", "de"];
    let (code, confidence) = detector.detect(text, &supported)?;
    Language::from_code(&code).map(|lang| (lang, confidence))
}

pub async fn handle_languages(State(state): State<Arc<AppState>>) -> Json<Vec<LanguagesResponse>> {
    let languages = state
        .language_codes()
        .into_iter()
        .filter_map(|code| {
            Language::from_code(code).map(|l| LanguagesResponse {
                name: l.name().to_string(),
                code: l.short_code().to_string(),
                long_code: l.code().to_string(),
            })
        })
        .collect();
    Json(languages)
}

pub async fn handle_version() -> Json<VersionResponse> {
    Json(VersionResponse {
        name: "OpenGrammar".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: 2,
    })
}

pub async fn handle_info() -> Json<InfoResponse> {
    Json(InfoResponse {
        name: "OpenGrammar".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        api_version: 2,
        max_text_length: MAX_TEXT_LENGTH,
        premium: false,
    })
}

pub async fn handle_max_text_length() -> Json<MaxTextLengthResponse> {
    Json(MaxTextLengthResponse {
        max_text_length: MAX_TEXT_LENGTH,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::build_app;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    fn app() -> axum::Router {
        build_app(Arc::new(AppState::new()))
    }

    async fn send(request: axum::http::Request<Body>) -> (axum::http::StatusCode, Value) {
        let response = app().oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn get(uri: &str) -> (axum::http::StatusCode, Value) {
        send(
            axum::http::Request::builder()
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    #[tokio::test]
    async fn check_without_text_is_a_json_bad_request() {
        for uri in [
            "/v2/check?language=en-US",
            "/v2/check?language=en-US&text=",
            "/v2/check",
        ] {
            let (status, json) = get(uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(json["error"], "Missing 'text' parameter", "{uri}: {json}");
        }
    }

    #[tokio::test]
    async fn check_with_unknown_or_absent_language_is_rejected() {
        for uri in ["/v2/check?text=Hello&language=xx", "/v2/check?text=Hello"] {
            let (status, json) = get(uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(
                json["error"], "Missing or invalid 'language' parameter",
                "{uri}: {json}"
            );
        }
    }

    #[tokio::test]
    async fn check_reads_data_as_text_before_validating_language() {
        let (status, json) = get("/v2/check?language=xx&data=Hello").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            json["error"], "Missing or invalid 'language' parameter",
            "data must satisfy the text requirement so only the language error remains: {json}"
        );
    }

    #[tokio::test]
    async fn check_rejects_text_over_the_length_cap() {
        let body = format!("language=en-US&text={}", "x".repeat(MAX_TEXT_LENGTH + 1));
        let (status, json) = send(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v2/check")
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            json["error"],
            format!("Text is too long. Maximum length is {MAX_TEXT_LENGTH} characters.")
        );
    }

    #[tokio::test]
    async fn check_auto_fails_when_detection_leaves_the_supported_set() {
        let text = "Bonjour%2C+ceci+est+une+phrase+fran%C3%A7aise+assez+longue+pour+%C3%AAtre+d%C3%A9tect%C3%A9e.";
        let (status, json) = get(&format!("/v2/check?language=auto&text={text}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            json["error"],
            "Language detection failed for 'auto'; specify an explicit 'language' parameter"
        );
    }

    #[tokio::test]
    async fn version_endpoint_reports_the_api_contract() {
        let (status, json) = get("/v2/version").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["name"], "OpenGrammar");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["apiVersion"], 2);
    }

    #[tokio::test]
    async fn info_endpoint_reports_name_version_and_limits() {
        let (status, json) = get("/v2/info").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["name"], "OpenGrammar");
        assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["apiVersion"], 2);
        assert_eq!(json["maxTextLength"], MAX_TEXT_LENGTH);
        assert_eq!(json["premium"], false);
    }

    #[tokio::test]
    async fn max_text_length_endpoint_exposes_the_cap() {
        let (status, json) = get("/v2/maxtextlength").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["maxTextLength"], MAX_TEXT_LENGTH);
    }

    #[tokio::test]
    async fn languages_endpoint_lists_sorted_advertised_engines() {
        let (status, json) = get("/v2/languages").await;
        assert_eq!(status, StatusCode::OK);
        let languages = json.as_array().expect("array body");
        assert_eq!(languages.len(), crate::server::ENGINE_CODES.len());
        let mut expected: Vec<&str> = crate::server::ENGINE_CODES.to_vec();
        expected.sort_unstable();
        let expected: Vec<String> = expected
            .iter()
            .map(|code| {
                Language::from_code(code)
                    .map(|l| l.code().to_string())
                    .unwrap_or_default()
            })
            .collect();
        let long_codes: Vec<String> = languages
            .iter()
            .map(|l| l["longCode"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(long_codes, expected);
        for language in languages {
            assert!(language["name"].as_str().is_some_and(|n| !n.is_empty()));
            assert!(language["code"].as_str().is_some_and(|c| !c.is_empty()));
        }
    }

    #[tokio::test]
    async fn cors_preflight_is_answered_by_the_permissive_layer() {
        let response = app()
            .oneshot(
                axum::http::Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/v2/languages")
                    .header("origin", "https://example.org")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
}
