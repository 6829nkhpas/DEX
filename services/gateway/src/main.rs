mod auth;
mod error;
mod handlers;
mod models;
mod rate_limit;
mod router;
mod state;

use router::create_router;
use state::AppState;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Gateway API service");

    // Initialize application state
    // Pointing to a dummy internal service URL for compiling/testing
    let state = AppState::new("http://localhost:8081".to_string());

    // Create router
    let app = create_router(state);

    // Bind and serve
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await?;
    
    tracing::info!("Listening on {}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
