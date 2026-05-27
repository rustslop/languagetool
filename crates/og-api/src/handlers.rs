use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use og_core::{CheckResult, Language};
use crate::request::CheckParams;
use crate::response::{InfoResponse, LanguagesResponse, MaxTextLengthResponse, VersionResponse};
use crate::server::{AppState, MAX_TEXT_LENGTH};
use std::sync::Arc;

pub async fn handle_check(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CheckParams>,
) -> Result<Json<CheckResult>, (StatusCode, Json<serde_json::Value>)> {
    let text = match params.get_text() {
        Some(t) if !t.is_empty() => t.to_string(),
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

    let language = match params.get_language() {
        Some(l) => l,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing or invalid 'language' parameter"})),
            ));
        }
    };

    // Find the appropriate language engine
    let engine = {
        let engines = &state.engines;
        let lang_code = language.code();
        // Try exact match first, then short code
        if let Some(e) = engines.get(lang_code) {
            e
        } else if let Some(e) = engines.get(language.short_code()) {
            e
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unsupported language: {}", lang_code)
                })),
            ));
        }
    };

    let result = engine.check(&text);
    Ok(Json(result))
}

pub async fn handle_languages() -> Json<Vec<LanguagesResponse>> {
    let languages = Language::all_languages()
        .into_iter()
        .map(|l| LanguagesResponse {
            name: l.name().to_string(),
            code: l.short_code().to_string(),
            long_code: l.code().to_string(),
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
