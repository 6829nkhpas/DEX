use crate::auth::AuthenticatedUser;
use crate::error::AppError;
use crate::models::{CancelOrderRequest, CreateOrderRequest, OrderResponse};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use types::order::Order;

pub async fn create_order(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<OrderResponse>, AppError> {
    state
        .rate_limiter
        .check_rate_limit(
            &format!("{}:order_placement", payload.account_id),
            20,
            20.0,
        )?;

    Ok(Json(state.create_mock_order(payload).await?))
}

pub async fn cancel_order(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(order_id): Path<String>,
    Json(payload): Json<CancelOrderRequest>,
) -> Result<StatusCode, AppError> {
    state
        .rate_limiter
        .check_rate_limit(
            &format!("{}:order_cancel", payload.account_id),
            50,
            50.0,
        )?;

    state
        .cancel_mock_order(&order_id, &payload.account_id.to_string())
        .await?;

    Ok(StatusCode::OK)
}

pub async fn get_order(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(order_id): Path<String>,
) -> Result<Json<Order>, AppError> {
    state
        .rate_limiter
        .check_rate_limit("orders:query", 60, 1.0)?;

    let Some(order) = state.get_mock_order(&order_id).await else {
        return Err(AppError::NotFound(format!("Order {} not found", order_id)));
    };

    Ok(Json(order))
}
