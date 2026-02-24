use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// Central error type for the Gateway application
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error")]
    InternalError(#[from] anyhow::Error),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, code) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg, "UNAUTHORIZED"),
            AppError::RateLimitExceeded(msg) => {
                // Rate limits should preferably include Retry-After, but we handle that in the middleware mostly.
                // For explicit endpoint rate limits, we use this.
                (StatusCode::TOO_MANY_REQUESTS, msg, "RATE_LIMIT_EXCEEDED")
            }
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg, "BAD_REQUEST"),
            AppError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                msg,
                "SERVICE_UNAVAILABLE",
            ),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg, "NOT_FOUND"),
            AppError::InternalError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                "INTERNAL_ERROR",
            ),
        };

        let body = Json(json!({
            "error": code,
            "message": error_message
        }));

        (status, body).into_response()
    }
}
