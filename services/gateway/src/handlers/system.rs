use crate::state::AppState;
use axum::{
    extract::State,
    http::header::CONTENT_TYPE,
    response::IntoResponse,
    Json,
};
use serde_json::json;

pub async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "gateway",
    }))
}

pub async fn ready(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ready",
        "service": "gateway",
        "internal_services_url": state.internal_services_url,
    }))
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        state.metrics_text().await,
    )
}
