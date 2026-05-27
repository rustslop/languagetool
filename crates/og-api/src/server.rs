use axum::routing::{get, post};
use axum::Router;
use og_core::Checker;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

pub const MAX_TEXT_LENGTH: usize = 100_000;

pub struct AppState {
    pub checker: Arc<Checker>,
}

pub fn build_checker() -> Checker {
    Checker::new()
        .with_sentence_tokenizer(Arc::new(crate::pipeline::SentenceSplitterAdapter))
        .with_word_tokenizer(Arc::new(crate::pipeline::WordTokenizerAdapter))
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
    let checker = build_checker();
    let state = Arc::new(AppState {
        checker: Arc::new(checker),
    });

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    info!("OpenGrammar server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
