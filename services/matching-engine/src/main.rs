use axum::{routing::get, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(|| async { "matching-engine" }))
        .route("/health", get(|| async { "ok" }))
        .route("/readyz", get(|| async { "ready" }))
        .route("/metrics", get(|| async { "dex_matching_engine_up 1\n" }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8081));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("matching-engine listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
