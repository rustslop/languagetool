use axum::routing::{get, post};
use axum::Router;
use og_langs::engine::LanguageEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

pub const MAX_TEXT_LENGTH: usize = 100_000;

pub struct AppState {
    pub engines: Arc<HashMap<String, LanguageEngine>>,
}

pub fn build_engines() -> HashMap<String, LanguageEngine> {
    let mut engines = HashMap::new();

    info!("Loading English language engine...");
    let en_engine = LanguageEngine::english();
    info!("English engine loaded with {} rules", en_engine.rule_count());
    engines.insert("en".to_string(), en_engine);
    engines.insert("en-US".to_string(), LanguageEngine::english());
    engines.insert("en-GB".to_string(), LanguageEngine::english());

    engines
}

pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v2/check", get(crate::handlers::handle_check))
        .route("/v2/check", post(crate::handlers::handle_check))
        .route("/v2/languages", get(crate::handlers::handle_languages))
        .route("/v2/version", get(crate::handlers::handle_version))
        .route("/v2/info", get(crate::handlers::handle_info))
        .route("/v2/maxtextlength", get(crate::handlers::handle_max_text_length))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn run_server(port: u16) -> anyhow::Result<()> {
    let engines = build_engines();
    let state = Arc::new(AppState {
        engines: Arc::new(engines),
    });

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    info!("OpenGrammar server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
