#[cfg(test)]
mod tests {
    use axum::{
        routing::get,
        Router, extract::Path, response::IntoResponse,
    };
    use crate::auth::AuthenticatedUser;
    use crate::state::AppState;
    use crate::rate_limit::RateLimiter;
    use crate::handlers::{order, account};
    use reqwest::Client;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use types::ids::AccountId;



    async fn spawn_mock_internal_server(account_id: AccountId) -> String {
        let account_id_str = account_id.to_string();
        let account_id_str_orders = account_id_str.clone();
        let account_id_str_accounts = account_id_str.clone();
        let app = Router::new()
            .route("/internal/orders/{id}", get(move |Path(id): Path<String>| {
                let account_id_str = account_id_str_orders.clone();
                async move {
                    if id == "valid-order-id" {
                        let order_json = serde_json::json!({
                            "order_id": "11111111-1111-1111-1111-111111111111",
                            "account_id": account_id_str,
                            "symbol": "BTC/USDT",
                            "side": "BUY",
                            "price": "50000",
                            "quantity": "1000",
                            "filled_quantity": "0",
                            "remaining_quantity": "1000",
                            "status": {"state": "PENDING"},
                            "time_in_force": {"type": "GTC"},
                            "created_at": 1600000000,
                            "updated_at": 1600000000,
                            "version": 0
                        });
                        (axum::http::StatusCode::OK, axum::Json(order_json)).into_response()
                    } else if id == "unauth-order-id" {
                        let order_json = serde_json::json!({
                            "order_id": "11111111-1111-1111-1111-111111111111",
                            "account_id": "other-acc",
                            "symbol": "BTC/USDT",
                            "side": "BUY",
                            "price": "50000",
                            "quantity": "1000",
                            "filled_quantity": "0",
                            "remaining_quantity": "1000",
                            "status": {"state": "PENDING"},
                            "time_in_force": {"type": "GTC"},
                            "created_at": 1600000000,
                            "updated_at": 1600000000,
                            "version": 0
                        });
                        (axum::http::StatusCode::OK, axum::Json(order_json)).into_response()
                    } else {
                        (axum::http::StatusCode::NOT_FOUND, "Not found").into_response()
                    }
                }
            }))
            .route("/internal/accounts/{id}", get(move |Path(id): Path<String>| {
                let account_id_str = account_id_str_accounts.clone();
                async move {
                    if id == account_id_str {
                        let acc_json = serde_json::json!({
                            "account_id": id,
                            "account_type": "SPOT",
                            "status": "ACTIVE",
                            "balances": {},
                            "created_at": 1600000000,
                            "updated_at": 1600000000,
                            "version": 0
                        });
                        (axum::http::StatusCode::OK, axum::Json(acc_json)).into_response()
                    } else {
                        (axum::http::StatusCode::NOT_FOUND, "Not found").into_response()
                    }
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

    fn create_mock_user(account_id: AccountId) -> AuthenticatedUser {
        AuthenticatedUser { account_id }
    }

    #[tokio::test]
    async fn test_get_order_success() {
        let account_id = AccountId::new();
        let url = spawn_mock_internal_server(account_id).await;
        let state = create_mock_state(url);
        let user = create_mock_user(account_id);

        let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("valid-order-id".to_string())).await;
        assert!(res.is_ok());
        let order = res.unwrap().0;
        assert_eq!(order.account_id.to_string(), account_id.to_string());
        assert_eq!(order.symbol.to_string(), "BTC/USDT");
    }

    #[tokio::test]
    async fn test_get_order_not_found() {
        let account_id = AccountId::new();
        let url = spawn_mock_internal_server(account_id).await;
        let state = create_mock_state(url);
        let user = create_mock_user(account_id);

        let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("missing-order".to_string())).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, crate::error::AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_get_order_unauthorized() {
        let account_id = AccountId::new();
        let url = spawn_mock_internal_server(account_id).await;
        let state = create_mock_state(url);
        let user = create_mock_user(account_id);

        let res = order::get_order(axum::extract::State(state.clone()), user, axum::extract::Path("unauth-order-id".to_string())).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, crate::error::AppError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn test_get_account_success() {
        let account_id = AccountId::new();
        let url = spawn_mock_internal_server(account_id).await;
        let state = create_mock_state(url);
        let user = create_mock_user(account_id);

        let res = account::get_account(axum::extract::State(state.clone()), user, axum::extract::Path(account_id.to_string())).await;
        assert!(res.is_ok());
        let account = res.unwrap().0;
        assert_eq!(account.account_id.to_string(), account_id.to_string());
    }

    #[tokio::test]
    async fn test_get_account_unauthorized() {
        let account_id = AccountId::new();
        let url = spawn_mock_internal_server(account_id).await;
        let state = create_mock_state(url);
        let user = create_mock_user(account_id);

        let res = account::get_account(axum::extract::State(state.clone()), user, axum::extract::Path("other-acc".to_string())).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, crate::error::AppError::Unauthorized(_)));
    }
}
