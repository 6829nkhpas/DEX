use crate::auth::AuthenticatedUser;
use crate::error::AppError;
use crate::models::{CancelOrderRequest, CreateOrderRequest, OrderResponse};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use types::ids::OrderId;
use axum::http::StatusCode;

pub async fn create_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<OrderResponse>, AppError> {
    // 1. Check rate limits (API Level)
    // For VIP / Institutional this would vary based on auth user tier
    state
        .rate_limiter
        .check_rate_limit(&format!("{}:order_placement", user.account_id), 20, 20.0)?;

    // 2. Validate user identity matches order owner
    if user.account_id != payload.account_id {
        return Err(AppError::Unauthorized("Cannot place order for another account".into()));
    }

    // 3. Forward to internal Order Service
    // POST /internal/orders
    let res = state
        .http_client
        .post(&format!("{}/internal/orders", state.internal_services_url))
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Order service error: {}", e)))?;

    if !res.status().is_success() {
        return Err(AppError::BadRequest("Failed to create order".into()));
    }

    // Mock successful response
    Ok(Json(OrderResponse {
        order_id: OrderId::new(),
        status: "PENDING".to_string(),
    }))
}

pub async fn cancel_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(order_id): Path<String>,
    Json(payload): Json<CancelOrderRequest>,
) -> Result<StatusCode, AppError> {
    // 1. Rate limiting
    state
        .rate_limiter
        .check_rate_limit(&format!("{}:order_cancel", user.account_id), 50, 50.0)?;

    // 2. Identity validation
    if user.account_id != payload.account_id {
        return Err(AppError::Unauthorized("Cannot cancel order for another account".into()));
    }

    // 3. Forward
    let res = state
        .http_client
        .delete(&format!(
            "{}/internal/orders/{}",
            state.internal_services_url, order_id
        ))
        .json(&payload)
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Order service error: {}", e)))?;

    if !res.status().is_success() {
        return Err(AppError::BadRequest("Failed to cancel order".into()));
    }

    Ok(StatusCode::OK)
}
