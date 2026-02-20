use crate::handlers::{account, order, ws};
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/orders", post(order::create_order))
        .route("/orders/:id", delete(order::cancel_order))
        .route("/accounts/:id", get(account::get_account))
        .route("/ws", get(ws::ws_handler));

    Router::new()
        .nest("/v1", api_routes)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
