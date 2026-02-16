//! Trade execution and settlement types
//!
//! Implements spec §3 (Trade Lifecycle)

use crate::ids::{AccountId, MarketId, OrderId, TradeId};
use crate::numeric::{Price, Quantity};
use crate::order::Side;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Trade state enum per spec §3.4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeState {
    /// Trade created, pending settlement
    MATCHED,
    /// Fully settled to accounts (terminal)
    SETTLED,
    /// Settlement failed - catastrophic (terminal)
    FAILED,
}

/// Complete trade structure per spec §3.2.1
///
/// Represents an atomic exchange between maker and taker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: TradeId,
    pub sequence: u64,  // Global monotonic sequence
    pub symbol: MarketId,
    
    // Order references
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    
    // Account references
    pub maker_account_id: AccountId,
    pub taker_account_id: AccountId,
    
    // Trade details (from taker perspective)
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    
    // Fees
    pub maker_fee: Decimal,
    pub taker_fee: Decimal,
    
    // Timestamps
    pub executed_at: i64,   // Unix nanos
    pub settled_at: Option<i64>,
    
    pub state: TradeState,
}

impl Trade {
    /// Create a new matched trade
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sequence: u64,
        symbol: MarketId,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker_account_id: AccountId,
        taker_account_id: AccountId,
        side: Side,
        price: Price,
        quantity: Quantity,
        maker_fee: Decimal,
        taker_fee: Decimal,
        executed_at: i64,
    ) -> Self {
        Self {
            trade_id: TradeId::new(),
            sequence,
            symbol,
            maker_order_id,
            taker_order_id,
            maker_account_id,
            taker_account_id,
            side,
            price,
            quantity,
            maker_fee,
            taker_fee,
            executed_at,
            settled_at: None,
            state: TradeState::MATCHED,
        }
    }

    /// Mark trade as settled
    pub fn settle(&mut self, timestamp: i64) {
        self.state = TradeState::SETTLED;
        self.settled_at = Some(timestamp);
    }

    /// Calculate trade value (price × quantity)
    pub fn trade_value(&self) -> Decimal {
        self.quantity.as_decimal() * self.price.as_decimal()
    }

    /// Check if trade is settled
    pub fn is_settled(&self) -> bool {
        matches!(self.state, TradeState::SETTLED)
    }

    /// Validate no self-trade (spec §3 invariant)
    pub fn validate_no_self_trade(&self) -> bool {
        self.maker_account_id != self.taker_account_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_creation() {
        let trade = Trade::new(
            123456,
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            Decimal::from(-5),  // Maker rebate
            Decimal::from(25),   // Taker fee
            1708123456789000000,
        );

        assert_eq!(trade.state, TradeState::MATCHED);
        assert!(!trade.is_settled());
        assert!(trade.validate_no_self_trade());
    }

    #[test]
    fn test_trade_settlement() {
        let mut trade = Trade::new(
            123456,
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            Decimal::ZERO,
            Decimal::from(25),
            1708123456789000000,
        );

        trade.settle(1708123456790000000);
        assert_eq!(trade.state, TradeState::SETTLED);
        assert!(trade.is_settled());
        assert!(trade.settled_at.is_some());
    }

    #[test]
    fn test_trade_value() {
        let trade = Trade::new(
            123456,
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            Decimal::ZERO,
            Decimal::from(25),
            1708123456789000000,
        );

        assert_eq!(trade.trade_value(), Decimal::from(25000));
    }
}

