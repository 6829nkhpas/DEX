//! Event structures for matching engine
//!
//! Defines events emitted during matching per spec §8 (Event Taxonomy)

use serde::{Deserialize, Serialize};
use types::ids::{AccountId, OrderId, TradeId};
use types::numeric::{Price, Quantity};
use types::order::Side;

/// Trade executed event per spec §8.3.2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecutedEvent {
    pub trade_id: TradeId,
    pub sequence: u64,
    pub symbol: String,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub maker_account_id: AccountId,
    pub taker_account_id: AccountId,
    pub price: Price,
    pub quantity: Quantity,
    pub side: Side,
    pub executed_at: i64,
}

/// Order partially filled event per spec §8.3.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderPartiallyFilledEvent {
    pub order_id: OrderId,
    pub filled_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub average_price: Price,
}

/// Order filled event per spec §8.3.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFilledEvent {
    pub order_id: OrderId,
    pub filled_quantity: Quantity,
    pub average_price: Price,
    pub total_value: String,
    pub total_fee: String,
}

/// Order canceled event per spec §8.3.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCanceledEvent {
    pub order_id: OrderId,
    pub canceled_by: CancelSource,
    pub reason: String,
    pub filled_quantity: Quantity,
    pub unfilled_quantity: Quantity,
}

/// Who canceled the order
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CancelSource {
    User,
    System,
}
