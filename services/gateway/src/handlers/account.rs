use crate::auth::AuthenticatedUser;
use crate::error::AppError;
use crate::models::AccountResponse;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};

pub async fn get_account(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(account_id): Path<String>,
) -> Result<Json<AccountResponse>, AppError> {
    state
        .rate_limiter
        .check_rate_limit(&format!("{}:account_query", account_id), 60, 1.0)?;

    Ok(Json(state.get_mock_account_response(&account_id).await))
}
