use axum::{routing::get, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(|| async { "risk-engine" }))
        .route("/health", get(|| async { "ok" }))
        .route("/readyz", get(|| async { "ready" }))
        .route("/metrics", get(|| async { "dex_risk_engine_up 1\n" }));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8083));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("risk-engine listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
