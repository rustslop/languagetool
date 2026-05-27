use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use og_core::CheckRequest;
use og_core::CheckResult;
use og_core::Language;
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

    let mother_tongue = params.mother_tongue.as_deref().and_then(Language::from_code);

    let check_request = CheckRequest {
        text,
        language,
        mother_tongue,
        enabled_rules: params.get_enabled_rules(),
        disabled_rules: params.get_disabled_rules(),
        enabled_categories: params.get_enabled_categories(),
        disabled_categories: params.get_disabled_categories(),
        level: params.level.clone(),
        picky: params.picky,
    };

    let result = state.checker.check(&check_request);
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
