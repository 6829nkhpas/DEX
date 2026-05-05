use crate::error::AppError;
use crate::models::{AccountResponse, CreateOrderRequest, OrderResponse};
use crate::rate_limit::RateLimiter;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::RwLock;
use types::order::{CancelReason, Order};

#[derive(Clone)]
pub struct AppState {
    pub rate_limiter: Arc<RateLimiter>,
    pub http_client: Client,
    pub internal_services_url: String,
    pub mock_exchange: Arc<RwLock<MockExchangeState>>,
    pub ws_connections_served: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct AccountStreamSnapshot {
    pub account_id: String,
    pub balances: HashMap<String, String>,
    pub orders: Vec<Order>,
}

#[derive(Debug, Default)]
pub struct MockExchangeState {
    orders: HashMap<String, Order>,
    balances: HashMap<String, HashMap<String, String>>,
}

impl AppState {
    pub fn new(service_url: String) -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new()),
            http_client: Client::new(),
            internal_services_url: service_url,
            mock_exchange: Arc::new(RwLock::new(MockExchangeState::default())),
            ws_connections_served: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn create_mock_order(
        &self,
        payload: CreateOrderRequest,
    ) -> Result<OrderResponse, AppError> {
        let timestamp = unix_timestamp_nanos()?;
        let order = Order::new(
            payload.account_id,
            payload.symbol,
            payload.side,
            payload.price,
            payload.quantity,
            payload.time_in_force,
            timestamp,
        );

        let mut exchange = self.mock_exchange.write().await;
        let account_key = payload.account_id.to_string();
        exchange.ensure_account(&account_key);
        exchange
            .orders
            .insert(order.order_id.to_string(), order.clone());

        Ok(OrderResponse {
            order_id: order.order_id,
            status: "PENDING".to_string(),
        })
    }

    pub async fn cancel_mock_order(
        &self,
        order_id: &str,
        account_id: &str,
    ) -> Result<(), AppError> {
        let timestamp = unix_timestamp_nanos()?;
        let mut exchange = self.mock_exchange.write().await;
        let Some(order) = exchange.orders.get_mut(order_id) else {
            return Err(AppError::NotFound(format!("Order {} not found", order_id)));
        };

        if order.account_id.to_string() != account_id {
            return Err(AppError::Unauthorized(
                "Cannot cancel order for another account".into(),
            ));
        }

        if order.status.is_terminal() {
            return Err(AppError::BadRequest("Order is already terminal".into()));
        }

        order.cancel(CancelReason::UserRequested, timestamp);
        Ok(())
    }

    pub async fn get_mock_order(&self, order_id: &str) -> Option<Order> {
        let exchange = self.mock_exchange.read().await;
        exchange.orders.get(order_id).cloned()
    }

    pub async fn get_mock_account_response(&self, account_id: &str) -> AccountResponse {
        let exchange = self.mock_exchange.read().await;
        AccountResponse {
            account_id: account_id.to_string(),
            balances: exchange
                .balances
                .get(account_id)
                .cloned()
                .unwrap_or_else(default_balances),
        }
    }

    pub async fn get_mock_account_stream_snapshot(
        &self,
        account_id: &str,
    ) -> AccountStreamSnapshot {
        let exchange = self.mock_exchange.read().await;
        AccountStreamSnapshot {
            account_id: account_id.to_string(),
            balances: exchange
                .balances
                .get(account_id)
                .cloned()
                .unwrap_or_else(default_balances),
            orders: exchange
                .orders
                .values()
                .filter(|order| order.account_id.to_string() == account_id)
                .cloned()
                .collect(),
        }
    }

    pub fn record_ws_connection(&self) -> u64 {
        self.ws_connections_served.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub async fn metrics_text(&self) -> String {
        let exchange = self.mock_exchange.read().await;
        let account_count = exchange.balances.len();
        let order_count = exchange.orders.len();
        let ws_connections = self.ws_connections_served.load(Ordering::Relaxed);

        format!(
            "# HELP dex_gateway_mock_orders Number of mock orders stored.\n\
# TYPE dex_gateway_mock_orders gauge\n\
dex_gateway_mock_orders {order_count}\n\
# HELP dex_gateway_mock_accounts Number of mock accounts stored.\n\
# TYPE dex_gateway_mock_accounts gauge\n\
dex_gateway_mock_accounts {account_count}\n\
# HELP dex_gateway_ws_connections_total Number of websocket sessions served.\n\
# TYPE dex_gateway_ws_connections_total counter\n\
dex_gateway_ws_connections_total {ws_connections}\n"
        )
    }
}

impl MockExchangeState {
    fn ensure_account(&mut self, account_id: &str) {
        self.balances
            .entry(account_id.to_string())
            .or_insert_with(default_balances);
    }
}

fn default_balances() -> HashMap<String, String> {
    HashMap::from([
        ("USDT".to_string(), "100000.00".to_string()),
        ("BTC".to_string(), "2.5000".to_string()),
        ("ETH".to_string(), "25.0000".to_string()),
    ])
}

fn unix_timestamp_nanos() -> Result<i64, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| AppError::InternalError(anyhow::anyhow!(err)))?;
    let nanos = now.as_nanos();
    i64::try_from(nanos).map_err(|_| AppError::InternalError(anyhow::anyhow!("timestamp overflow")))
}
