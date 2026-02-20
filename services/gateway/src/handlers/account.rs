use crate::auth::AuthenticatedUser;
use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use types::account::Account;

pub async fn get_account(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(account_id): Path<String>,
) -> Result<Json<Account>, AppError> {
    // Rate limits
    state
        .rate_limiter
        .check_rate_limit(&format!("{}:account_query", user.account_id), 60, 1.0)?;

    // Identity validation
    if user.account_id.to_string() != account_id {
        return Err(AppError::Unauthorized("Cannot view another account".into()));
    }

    // Forward to internal Account Service
    let res = state
        .http_client
        .get(&format!(
            "{}/internal/accounts/{}",
            state.internal_services_url, account_id
        ))
        .send()
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Account service error: {}", e)))?;

    if !res.status().is_success() {
        return Err(AppError::BadRequest("Failed to retrieve account".into()));
    }
    
    // Deser Account
    let account = res
        .json::<Account>()
        .await
        .map_err(|_| AppError::InternalError(anyhow::anyhow!("Invalid account parsing")))?;

    Ok(Json(account))
}
