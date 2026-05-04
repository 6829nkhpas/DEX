use axum::{
    routing::get,
    Router, Json, extract::Path,
};
use gateway::auth::AuthenticatedUser;
use gateway::state::AppState;
use gateway::rate_limit::RateLimiter;
use gateway::handlers::{order, account};
use reqwest::Client;
use std::sync::Arc;
use tokio::net::TcpListener;
use types::account::Account;
use types::order::{Order, OrderSide, OrderType, OrderStatus};
use types::ids::OrderId;
use rust_decimal::Decimal;
use std::str::FromStr;

async fn spawn_mock_internal_server() -> String {
    let app = Router::new()
        .route("/internal/orders/:id", get(|Path(id): Path<String>| async move {
            if id == "valid-order-id" {
                let mut order = Order::new(
                    "valid-acc",
                    "BTC/USDT",
                    OrderSide::Buy,
                    OrderType::Limit,
                    Decimal::from_str("1000").unwrap(),
                    Some(Decimal::from_str("50000").unwrap()),
                );
                // Hardcode order_id to pretend it is the queried one
                // Wait, order.order_id is private or public?
                // Let's just serialize it directly as JSON to bypass struct field accessibility if needed
                let order_json = serde_json::json!({
                    "order_id": id,
                    "account_id": "valid-acc",
                    "symbol": "BTC/USDT",
                    "side": "Buy",
                    "order_type": "Limit",
                    "quantity": "1000",
                    "price": "50000",
                    "status": "New",
                    "filled_quantity": "0",
                    "created_at": 1600000000,
                    "updated_at": 1600000000
                });
                (axum::http::StatusCode::OK, axum::Json(order_json)).into_response()
            } else if id == "unauth-order-id" {
                let order_json = serde_json::json!({
                    "order_id": id,
                    "account_id": "other-acc",
                    "symbol": "BTC/USDT",
                    "side": "Buy",
                    "order_type": "Limit",
                    "quantity": "1000",
                    "price": "50000",
                    "status": "New",
                    "filled_quantity": "0",
                    "created_at": 1600000000,
                    "updated_at": 1600000000
                });
                (axum::http::StatusCode::OK, axum::Json(order_json)).into_response()
            } else {
                (axum::http::StatusCode::NOT_FOUND, "Not found").into_response()
            }
        }))
        .route("/internal/accounts/:id", get(|Path(id): Path<String>| async move {
            if id == "valid-acc" {
                let acc_json = serde_json::json!({
                    "account_id": id,
                    "balances": {},
                    "margin_fraction": "1.0",
                    "is_liquidatable": false,
                    "nonce": 0
                });
                (axum::http::StatusCode::OK, axum::Json(acc_json)).into_response()
            } else {
                (axum::http::StatusCode::NOT_FOUND, "Not found").into_response()
            }
        }));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{}", addr)
}

fn create_mock_state(internal_url: String) -> AppState {
    AppState {
        rate_limiter: Arc::new(RateLimiter::new()),
        internal_services_url: internal_url,
        http_client: Client::new(),
    }
}

fn create_mock_user(account_id: &str) -> AuthenticatedUser {
    AuthenticatedUser {
        account_id: account_id.to_string(),
        role: "user".to_string(),
    }
}

#[tokio::test]
async fn test_get_order_success() {
    let url = spawn_mock_internal_server().await;
    let state = create_mock_state(url);
    let user = create_mock_user("valid-acc");

    let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("valid-order-id".to_string())).await;
    assert!(res.is_ok());
    let order = res.unwrap().0;
    assert_eq!(order.account_id, "valid-acc");
    assert_eq!(order.symbol, "BTC/USDT");
}

#[tokio::test]
async fn test_get_order_not_found() {
    let url = spawn_mock_internal_server().await;
    let state = create_mock_state(url);
    let user = create_mock_user("valid-acc");

    let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("missing-order".to_string())).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(matches!(err, gateway::error::AppError::NotFound(_)));
}

#[tokio::test]
async fn test_get_order_unauthorized() {
    let url = spawn_mock_internal_server().await;
    let state = create_mock_state(url);
    let user = create_mock_user("valid-acc");

    let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("unauth-order-id".to_string())).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(matches!(err, gateway::error::AppError::Unauthorized(_)));
}

#[tokio::test]
async fn test_get_account_success() {
    let url = spawn_mock_internal_server().await;
    let state = create_mock_state(url);
    let user = create_mock_user("valid-acc");

    let res = account::get_account(axum::extract::State(state.clone()), user, axum::extract::Path("valid-acc".to_string())).await;
    assert!(res.is_ok());
    let account = res.unwrap().0;
    assert_eq!(account.account_id, "valid-acc");
}

#[tokio::test]
async fn test_get_account_unauthorized() {
    let url = spawn_mock_internal_server().await;
    let state = create_mock_state(url);
    let user = create_mock_user("valid-acc");

    let res = account::get_account(axum::extract::State(state.clone()), user, axum::extract::Path("other-acc".to_string())).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(matches!(err, gateway::error::AppError::Unauthorized(_)));
}
