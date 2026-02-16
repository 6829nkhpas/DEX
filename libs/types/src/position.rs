//! Position tracking types
//!
//! Implements spec §4.4 (Position Model)

use crate::ids::{AccountId, MarketId};
use crate::numeric::{Price, Quantity};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Position side enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PositionSide {
    /// Long position - profit when price increases
    LONG,
    /// Short position - profit when price decreases
    SHORT,
}

/// Position structure per spec §4.4.1
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub position_id: Uuid,
    pub account_id: AccountId,
    pub symbol: MarketId,
    pub side: PositionSide,
    pub size: Quantity,
    pub entry_price: Price,
    pub mark_price: Price,
    pub liquidation_price: Price,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub initial_margin: Decimal,
    pub maintenance_margin: Decimal,
    pub leverage: u8,  // 1-125
    pub opened_at: i64,
    pub updated_at: i64,
    pub version: u64,
}

impl Position {
    /// Create a new position
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: AccountId,
        symbol: MarketId,
        side: PositionSide,
        size: Quantity,
        entry_price: Price,
        mark_price: Price,
        liquidation_price: Price,
        initial_margin: Decimal,
        maintenance_margin: Decimal,
        leverage: u8,
        timestamp: i64,
    ) -> Self {
        let unrealized_pnl = Self::calculate_pnl(side, entry_price, mark_price, size);
        
        Self {
            position_id: Uuid::now_v7(),
            account_id,
            symbol,
            side,
            size,
            entry_price,
            mark_price,
            liquidation_price,
            realized_pnl: Decimal::ZERO,
            unrealized_pnl,
            initial_margin,
            maintenance_margin,
            leverage,
            opened_at: timestamp,
            updated_at: timestamp,
            version: 0,
        }
    }

    /// Calculate unrealized PnL per spec §4.4.3
    fn calculate_pnl(side: PositionSide, entry_price: Price, mark_price: Price, size: Quantity) -> Decimal {
        let size_decimal = size.as_decimal();
        match side {
            // LONG: (mark_price - entry_price) × size
            PositionSide::LONG => {
                (mark_price.as_decimal() - entry_price.as_decimal()) * size_decimal
            }
            // SHORT: (entry_price - mark_price) × size
            PositionSide::SHORT => {
                (entry_price.as_decimal() - mark_price.as_decimal()) * size_decimal
            }
        }
    }

    /// Update mark price and recalculate unrealized PnL
    pub fn update_mark_price(&mut self, new_mark_price: Price, timestamp: i64) {
        self.mark_price = new_mark_price;
        self.unrealized_pnl = Self::calculate_pnl(self.side, self.entry_price, new_mark_price, self.size);
        self.updated_at = timestamp;
        self.version += 1;
    }

    /// Calculate margin ratio per spec §4.8.1
    pub fn margin_ratio(&self) -> Decimal {
        let equity = self.initial_margin + self.unrealized_pnl;
        if self.maintenance_margin == Decimal::ZERO {
            Decimal::MAX
        } else {
            equity / self.maintenance_margin
        }
    }

    /// Check if position should be liquidated (margin_ratio < 1.1)
    pub fn should_liquidate(&self) -> bool {
        self.margin_ratio() < Decimal::from_str_exact("1.1").unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_creation() {
        let position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(51000),
            Price::from_u64(49500),
            Decimal::from(5000),
            Decimal::from(500),
            10,
            1708123456789000000,
        );

        assert_eq!(position.side, PositionSide::LONG);
        assert_eq!(position.unrealized_pnl, Decimal::from(1000));  // (51000 - 50000) * 1
    }

    #[test]
    fn test_long_position_pnl() {
        let position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(51000),
            Price::from_u64(49500),
            Decimal::from(5000),
            Decimal::from(500),
            10,
            1708123456789000000,
        );

        assert_eq!(position.unrealized_pnl, Decimal::from(1000));
    }

    #[test]
    fn test_short_position_pnl() {
        let position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::SHORT,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(49000),
            Price::from_u64(50500),
            Decimal::from(5000),
            Decimal::from(500),
            10,
            1708123456789000000,
        );

        assert_eq!(position.unrealized_pnl, Decimal::from(1000));  // (50000 - 49000) * 1
    }

    #[test]
    fn test_mark_price_update() {
        let mut position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(50000),
            Price::from_u64(49500),
            Decimal::from(5000),
            Decimal::from(500),
            10,
            1708123456789000000,
        );

        position.update_mark_price(Price::from_u64(52000), 1708123456790000000);
        assert_eq!(position.unrealized_pnl, Decimal::from(2000));
    }

    #[test]
    fn test_margin_ratio() {
        let position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(51000),
            Price::from_u64(49500),
            Decimal::from(5000),
            Decimal::from(500),
            10,
            1708123456789000000,
        );

        // Equity = 5000 + 1000 = 6000
        // Margin ratio = 6000 / 500 = 12.0
        assert_eq!(position.margin_ratio(), Decimal::from(12));
        assert!(!position.should_liquidate());
    }

    #[test]
    fn test_liquidation_trigger() {
        let position = Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50000),
            Price::from_u64(45500),  // Large loss
            Price::from_u64(49500),
            Decimal::from(5000),
            Decimal::from(5000),  // High maintenance margin
            10,
            1708123456789000000,
        );

        // Equity = 5000 + (-4500) = 500
        // Margin ratio = 500 / 5000 = 0.1
        assert!(position.should_liquidate());
    }
}

