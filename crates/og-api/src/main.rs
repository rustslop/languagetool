use og_api::server::run_server;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    if let Err(e) = run_server(8081).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}
