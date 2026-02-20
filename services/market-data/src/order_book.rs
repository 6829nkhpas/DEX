//! In-memory order book state builder
//!
//! Maintains a mirrored book state from matching-engine events.
//! Uses `BTreeMap` for deterministic sorted iteration (spec §12.3.5).
//! All arithmetic uses `Decimal` (spec §12.4.1).
//!
//! The book processes:
//! - `OrderAccepted` → add quantity to price level
//! - `TradeExecuted` → remove filled quantity from maker's level
//! - `OrderCanceled` → remove remaining quantity from price level
//!
//! Fully filled orders are removed. Empty price levels are compressed.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::{MarketId, OrderId};
use types::numeric::{Price, Quantity};
use types::order::Side;

/// A single price level in the order book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceLevel {
    /// The price of this level.
    pub price: Price,
    /// Total quantity at this level across all orders.
    pub total_quantity: Decimal,
    /// Number of orders resting at this level.
    pub order_count: u32,
}

impl PriceLevel {
    /// Create a new price level with initial quantity.
    fn new(price: Price, quantity: Decimal) -> Self {
        Self {
            price,
            total_quantity: quantity,
            order_count: 1,
        }
    }

    /// Check if this level has no remaining quantity.
    fn is_empty(&self) -> bool {
        self.total_quantity <= Decimal::ZERO
    }
}

/// Tracks an individual order resting on the book for accurate level updates.
#[derive(Debug, Clone)]
struct RestingOrder {
    pub order_id: OrderId,
    pub side: Side,
    pub price: Price,
    pub remaining_quantity: Decimal,
}

/// In-memory order book mirror for a single symbol.
///
/// Bids stored in descending price order (best bid first).
/// Asks stored in ascending price order (best ask first).
/// Uses `BTreeMap` for deterministic iteration (spec §12.3.5).
#[derive(Debug, Clone)]
pub struct OrderBookState {
    /// Trading pair symbol.
    pub symbol: MarketId,
    /// Bid levels: price → level (BTreeMap sorts ascending, we reverse for best-bid-first).
    bids: BTreeMap<Decimal, PriceLevel>,
    /// Ask levels: price → level (ascending = best ask first).
    asks: BTreeMap<Decimal, PriceLevel>,
    /// Individual resting orders for tracking fills/cancels.
    orders: BTreeMap<String, RestingOrder>,
    /// Current best bid price.
    best_bid: Option<Price>,
    /// Current best ask price.
    best_ask: Option<Price>,
    /// Last sequence number processed.
    last_sequence: u64,
}

impl OrderBookState {
    /// Create an empty order book for the given symbol.
    pub fn new(symbol: MarketId) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: BTreeMap::new(),
            best_bid: None,
            best_ask: None,
            last_sequence: 0,
        }
    }

    /// Apply an OrderAccepted event: add the order to the appropriate side.
    pub fn apply_order_accepted(
        &mut self,
        order_id: OrderId,
        side: Side,
        price: Price,
        quantity: Quantity,
        sequence: u64,
    ) {
        let qty_dec = quantity.as_decimal();
        let price_dec = price.as_decimal();

        // Add to price level
        let levels = match side {
            Side::BUY => &mut self.bids,
            Side::SELL => &mut self.asks,
        };

        levels
            .entry(price_dec)
            .and_modify(|level| {
                level.total_quantity += qty_dec;
                level.order_count += 1;
            })
            .or_insert_with(|| PriceLevel::new(price, qty_dec));

        // Track the resting order
        self.orders.insert(
            order_id.to_string(),
            RestingOrder {
                order_id,
                side,
                price,
                remaining_quantity: qty_dec,
            },
        );

        self.last_sequence = sequence;
        self.update_best_prices();
    }

    /// Apply a TradeExecuted event: reduce maker's resting quantity.
    ///
    /// The maker's order loses `quantity` from its resting level.
    /// If the order is fully filled, it is removed.
    pub fn apply_trade_executed(
        &mut self,
        maker_order_id: OrderId,
        quantity: Quantity,
        sequence: u64,
    ) {
        let qty_dec = quantity.as_decimal();
        let order_key = maker_order_id.to_string();

        if let Some(order) = self.orders.get_mut(&order_key) {
            let price_dec = order.price.as_decimal();
            let levels = match order.side {
                Side::BUY => &mut self.bids,
                Side::SELL => &mut self.asks,
            };

            if let Some(level) = levels.get_mut(&price_dec) {
                level.total_quantity -= qty_dec;

                if level.total_quantity < Decimal::ZERO {
                    level.total_quantity = Decimal::ZERO;
                }
            }

            order.remaining_quantity -= qty_dec;
            if order.remaining_quantity < Decimal::ZERO {
                order.remaining_quantity = Decimal::ZERO;
            }

            // Remove fully filled order
            if order.remaining_quantity <= Decimal::ZERO {
                let side = order.side;
                let price_dec_copy = price_dec;
                self.orders.remove(&order_key);

                // Decrement order count and compress if empty
                let levels = match side {
                    Side::BUY => &mut self.bids,
                    Side::SELL => &mut self.asks,
                };
                if let Some(level) = levels.get_mut(&price_dec_copy) {
                    level.order_count = level.order_count.saturating_sub(1);
                }
            }
        }

        self.compress_empty_levels();
        self.last_sequence = sequence;
        self.update_best_prices();
    }

    /// Apply an OrderCanceled event: remove remaining quantity from the level.
    pub fn apply_cancel(
        &mut self,
        order_id: OrderId,
        remaining_quantity: Quantity,
        sequence: u64,
    ) {
        let qty_dec = remaining_quantity.as_decimal();
        let order_key = order_id.to_string();

        if let Some(order) = self.orders.remove(&order_key) {
            let price_dec = order.price.as_decimal();
            let levels = match order.side {
                Side::BUY => &mut self.bids,
                Side::SELL => &mut self.asks,
            };

            if let Some(level) = levels.get_mut(&price_dec) {
                level.total_quantity -= qty_dec;
                if level.total_quantity < Decimal::ZERO {
                    level.total_quantity = Decimal::ZERO;
                }
                level.order_count = level.order_count.saturating_sub(1);
            }
        }

        self.compress_empty_levels();
        self.last_sequence = sequence;
        self.update_best_prices();
    }

    /// Remove all price levels with zero or negative quantity.
    pub fn compress_empty_levels(&mut self) {
        self.bids.retain(|_, level| !level.is_empty());
        self.asks.retain(|_, level| !level.is_empty());
    }

    /// Get the current best bid price.
    pub fn best_bid(&self) -> Option<Price> {
        self.best_bid
    }

    /// Get the current best ask price.
    pub fn best_ask(&self) -> Option<Price> {
        self.best_ask
    }

    /// Get the mid-market price (average of best bid and best ask).
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => {
                Some((bid.as_decimal() + ask.as_decimal()) / Decimal::from(2))
            }
            _ => None,
        }
    }

    /// Get the spread between best ask and best bid.
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid, self.best_ask) {
            (Some(bid), Some(ask)) => Some(ask.as_decimal() - bid.as_decimal()),
            _ => None,
        }
    }

    /// Build a depth snapshot with the specified max levels per side.
    ///
    /// Bids are returned in descending price order (best first).
    /// Asks are returned in ascending price order (best first).
    pub fn depth_snapshot(&self, max_levels: usize) -> DepthSnapshot {
        let bids: Vec<PriceLevel> = self
            .bids
            .values()
            .rev() // Descending for bids (best bid = highest price)
            .take(max_levels)
            .cloned()
            .collect();

        let asks: Vec<PriceLevel> = self
            .asks
            .values()
            .take(max_levels) // Ascending for asks (best ask = lowest price)
            .cloned()
            .collect();

        DepthSnapshot {
            symbol: self.symbol.clone(),
            bids,
            asks,
            last_sequence: self.last_sequence,
        }
    }

    /// Number of bid price levels.
    pub fn bid_depth(&self) -> usize {
        self.bids.len()
    }

    /// Number of ask price levels.
    pub fn ask_depth(&self) -> usize {
        self.asks.len()
    }

    /// Total number of resting orders tracked.
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    /// Last processed sequence number.
    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    /// Get all bid levels (descending price order).
    pub fn bid_levels(&self) -> Vec<PriceLevel> {
        self.bids.values().rev().cloned().collect()
    }

    /// Get all ask levels (ascending price order).
    pub fn ask_levels(&self) -> Vec<PriceLevel> {
        self.asks.values().cloned().collect()
    }

    /// Recalculate best bid and best ask from the book state.
    fn update_best_prices(&mut self) {
        // Best bid = highest price in bids (last in BTreeMap)
        self.best_bid = self
            .bids
            .keys()
            .next_back()
            .and_then(|d| Price::try_new(*d));

        // Best ask = lowest price in asks (first in BTreeMap)
        self.best_ask = self.asks.keys().next().and_then(|d| Price::try_new(*d));
    }
}

/// A snapshot of the order book depth at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepthSnapshot {
    pub symbol: MarketId,
    /// Bids in descending price order (best first).
    pub bids: Vec<PriceLevel>,
    /// Asks in ascending price order (best first).
    pub asks: Vec<PriceLevel>,
    /// Sequence number at the time of the snapshot.
    pub last_sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_book() -> OrderBookState {
        OrderBookState::new(MarketId::new("BTC/USDT"))
    }

    #[test]
    fn test_empty_book() {
        let book = make_book();
        assert_eq!(book.bid_depth(), 0);
        assert_eq!(book.ask_depth(), 0);
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
        assert!(book.mid_price().is_none());
        assert!(book.spread().is_none());
    }

    #[test]
    fn test_apply_order_accepted_bid() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        assert_eq!(book.bid_depth(), 1);
        assert_eq!(book.best_bid(), Some(Price::from_u64(50000)));
        assert_eq!(book.order_count(), 1);
    }

    #[test]
    fn test_apply_order_accepted_ask() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::SELL,
            Price::from_u64(51000),
            Quantity::from_str("2.5").unwrap(),
            1,
        );

        assert_eq!(book.ask_depth(), 1);
        assert_eq!(book.best_ask(), Some(Price::from_u64(51000)));
    }

    #[test]
    fn test_multiple_orders_same_level() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );
        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("2.0").unwrap(),
            2,
        );

        assert_eq!(book.bid_depth(), 1); // Same price level
        let levels = book.bid_levels();
        assert_eq!(levels[0].total_quantity, Decimal::from(3));
        assert_eq!(levels[0].order_count, 2);
    }

    #[test]
    fn test_best_bid_ask_updates() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(49000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );
        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            2,
        );
        book.apply_order_accepted(
            OrderId::new(),
            Side::SELL,
            Price::from_u64(51000),
            Quantity::from_str("1.0").unwrap(),
            3,
        );
        book.apply_order_accepted(
            OrderId::new(),
            Side::SELL,
            Price::from_u64(52000),
            Quantity::from_str("1.0").unwrap(),
            4,
        );

        assert_eq!(book.best_bid(), Some(Price::from_u64(50000)));
        assert_eq!(book.best_ask(), Some(Price::from_u64(51000)));

        let spread = book.spread().unwrap();
        assert_eq!(spread, Decimal::from(1000));

        let mid = book.mid_price().unwrap();
        assert_eq!(mid, Decimal::from(50500));
    }

    #[test]
    fn test_apply_trade_executed() {
        let mut book = make_book();
        let maker_id = OrderId::new();

        book.apply_order_accepted(
            maker_id,
            Side::SELL,
            Price::from_u64(51000),
            Quantity::from_str("2.0").unwrap(),
            1,
        );

        // Partial fill: 0.5 of 2.0
        book.apply_trade_executed(maker_id, Quantity::from_str("0.5").unwrap(), 2);

        assert_eq!(book.ask_depth(), 1);
        let levels = book.ask_levels();
        assert_eq!(
            levels[0].total_quantity,
            Decimal::from_str_exact("1.5").unwrap()
        );
    }

    #[test]
    fn test_trade_fully_fills_order() {
        let mut book = make_book();
        let maker_id = OrderId::new();

        book.apply_order_accepted(
            maker_id,
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        book.apply_trade_executed(maker_id, Quantity::from_str("1.0").unwrap(), 2);

        // Level should be compressed away
        assert_eq!(book.bid_depth(), 0);
        assert_eq!(book.order_count(), 0);
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_apply_cancel() {
        let mut book = make_book();
        let order_id = OrderId::new();

        book.apply_order_accepted(
            order_id,
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("3.0").unwrap(),
            1,
        );

        book.apply_cancel(order_id, Quantity::from_str("3.0").unwrap(), 2);

        assert_eq!(book.bid_depth(), 0);
        assert_eq!(book.order_count(), 0);
    }

    #[test]
    fn test_depth_snapshot() {
        let mut book = make_book();

        // Add 5 bid levels
        for i in 1..=5 {
            book.apply_order_accepted(
                OrderId::new(),
                Side::BUY,
                Price::from_u64(50000 - i * 100),
                Quantity::from_str("1.0").unwrap(),
                i as u64,
            );
        }

        // Add 5 ask levels
        for i in 1..=5 {
            book.apply_order_accepted(
                OrderId::new(),
                Side::SELL,
                Price::from_u64(51000 + i * 100),
                Quantity::from_str("1.0").unwrap(),
                (5 + i) as u64,
            );
        }

        // Snapshot with 3 levels max
        let snapshot = book.depth_snapshot(3);
        assert_eq!(snapshot.bids.len(), 3);
        assert_eq!(snapshot.asks.len(), 3);

        // Bids should be descending (best first)
        assert!(snapshot.bids[0].price > snapshot.bids[1].price);

        // Asks should be ascending (best first)
        assert!(snapshot.asks[0].price < snapshot.asks[1].price);

        assert_eq!(snapshot.last_sequence, 10);
    }

    #[test]
    fn test_compress_empty_levels() {
        let mut book = make_book();
        let id1 = OrderId::new();
        let id2 = OrderId::new();

        book.apply_order_accepted(
            id1,
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );
        book.apply_order_accepted(
            id2,
            Side::BUY,
            Price::from_u64(49000),
            Quantity::from_str("1.0").unwrap(),
            2,
        );

        assert_eq!(book.bid_depth(), 2);

        // Cancel first order — level should be compressed
        book.apply_cancel(id1, Quantity::from_str("1.0").unwrap(), 3);
        assert_eq!(book.bid_depth(), 1);
        assert_eq!(book.best_bid(), Some(Price::from_u64(49000)));
    }

    #[test]
    fn test_depth_snapshot_serialization() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        let snapshot = book.depth_snapshot(10);
        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: DepthSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, deserialized);
    }

    #[test]
    fn test_sequence_tracking() {
        let mut book = make_book();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            42,
        );

        assert_eq!(book.last_sequence(), 42);
    }
}
