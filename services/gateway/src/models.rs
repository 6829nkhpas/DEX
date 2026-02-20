use serde::{Deserialize, Serialize};
use types::numeric::{Price, Quantity};
use types::order::{Side, TimeInForce};
use types::ids::{AccountId, MarketId, OrderId};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrderRequest {
    pub account_id: AccountId,
    pub symbol: MarketId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub time_in_force: TimeInForce,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderResponse {
    pub order_id: OrderId,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CancelOrderRequest {
    pub account_id: AccountId,
}
