use crate::auth::AuthenticatedUser;
use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::Response,
};
use futures::stream::StreamExt;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Response, AppError> {
    // 1. Rate limiting
    state
        .rate_limiter
        .check_rate_limit(&format!("{}:ws_connections", user.account_id), 10, 10.0)?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, user)))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, user: AuthenticatedUser) {
    // Mock WebSocket loop
    if socket.send(Message::Text(axum::extract::ws::Utf8Bytes::from("Connected"))).await.is_err() {
        return;
    }

    while let Some(msg) = socket.next().await {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(text) => {
                    // E.g., subscription requests
                    if text == "subscribe:market_data" {
                        // Rate Limit API
                        let _ = state.rate_limiter.check_rate_limit(&format!("{}:ws_subscriptions", user.account_id), 50, 50.0);
                        let _ = socket.send(Message::Text(axum::extract::ws::Utf8Bytes::from("Subscribed"))).await;
                    }
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        } else {
            break;
        }
    }
}
