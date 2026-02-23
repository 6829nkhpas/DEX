//! Deterministic matching engine for simulation
//!
//! Implements price-time priority order matching per spec §1 (Order Lifecycle)
//! and spec §3 (Trade Lifecycle). All arithmetic uses fixed-point Decimal.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use types::fee::FeeTier;
use types::ids::{AccountId, MarketId, OrderId, TradeId};
use types::numeric::{Price, Quantity};
use types::order::Side;

/// Fee rounding precision (8 dp, spec §7.2: round UP to 8 dp).
const FEE_DP: u32 = 8;

/// A resting order on the book, keyed by (price, timestamp) for price-time priority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookEntry {
    pub order_id: OrderId,
    pub account_id: AccountId,
    pub side: Side,
    pub price: Price,
    pub remaining: Decimal,
    pub timestamp: i64,
}

/// Events emitted during simulation matching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SimEvent {
    OrderPlaced {
        order_id: OrderId,
        account_id: AccountId,
        side: Side,
        price: Price,
        quantity: Decimal,
        timestamp: i64,
    },
    TradeExecuted {
        trade_id: TradeId,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker_account_id: AccountId,
        taker_account_id: AccountId,
        price: Price,
        quantity: Decimal,
        maker_fee: Decimal,
        taker_fee: Decimal,
        timestamp: i64,
    },
    OrderFilled {
        order_id: OrderId,
        filled_quantity: Decimal,
        timestamp: i64,
    },
    OrderPartiallyFilled {
        order_id: OrderId,
        filled_quantity: Decimal,
        remaining_quantity: Decimal,
        timestamp: i64,
    },
    OrderCanceled {
        order_id: OrderId,
        remaining_quantity: Decimal,
        timestamp: i64,
    },
}

/// A single price level aggregating multiple orders at the same price.
#[derive(Debug, Clone)]
struct PriceLevel {
    orders: Vec<BookEntry>,
}

impl PriceLevel {
    fn new() -> Self {
        Self { orders: Vec::new() }
    }

    fn total_quantity(&self) -> Decimal {
        self.orders.iter().map(|o| o.remaining).sum()
    }

    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

/// Deterministic simulation matching engine.
///
/// Maintains a bid/ask order book with price-time priority matching.
/// All state transitions emit `SimEvent`s for replay validation.
pub struct SimEngine {
    pub symbol: MarketId,
    bids: BTreeMap<OrderedPrice, PriceLevel>,
    asks: BTreeMap<OrderedPrice, PriceLevel>,
    fee_tier: FeeTier,
    pub events: Vec<SimEvent>,
    pub sequence: u64,
}

/// Wrapper for BTreeMap ordering. Bids: descending (negate). Asks: ascending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OrderedPrice {
    /// For bids we store negated price so BTreeMap natural order gives descending.
    /// For asks we store raw price for ascending order.
    key: i128,
}

impl OrderedPrice {
    fn bid(price: Price) -> Self {
        // Negate the mantissa for descending order in BTreeMap
        let raw = decimal_to_i128(price.as_decimal());
        Self { key: -raw }
    }

    fn ask(price: Price) -> Self {
        let raw = decimal_to_i128(price.as_decimal());
        Self { key: raw }
    }
}

/// Convert Decimal to i128 with 18-digit fixed-point scaling for ordering.
fn decimal_to_i128(d: Decimal) -> i128 {
    // Scale to 18 decimal places for comparison
    let scale_factor = Decimal::from_i128_with_scale(1_000_000_000_000_000_000, 0);
    let scaled = d * scale_factor;
    scaled.to_i128().unwrap_or(0)
}

impl SimEngine {
    /// Create a new engine for a market with a fee tier.
    pub fn new(symbol: MarketId, fee_tier: FeeTier) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            fee_tier,
            events: Vec::new(),
            sequence: 0,
        }
    }

    /// Insert an order: match against opposing side, then rest remainder.
    pub fn submit_order(
        &mut self,
        account_id: AccountId,
        side: Side,
        price: Price,
        quantity: Decimal,
        timestamp: i64,
    ) -> OrderId {
        let order_id = OrderId::new();
        self.sequence += 1;

        self.events.push(SimEvent::OrderPlaced {
            order_id,
            account_id,
            side,
            price,
            quantity,
            timestamp,
        });

        let mut remaining = quantity;
        remaining = self.match_against_book(
            order_id, account_id, side, price, remaining, timestamp,
        );

        if remaining > Decimal::ZERO {
            self.insert_resting(BookEntry {
                order_id,
                account_id,
                side,
                price,
                remaining,
                timestamp,
            });
        }

        let filled = quantity - remaining;
        if filled > Decimal::ZERO && remaining == Decimal::ZERO {
            self.events.push(SimEvent::OrderFilled {
                order_id,
                filled_quantity: filled,
                timestamp,
            });
        } else if filled > Decimal::ZERO {
            self.events.push(SimEvent::OrderPartiallyFilled {
                order_id,
                filled_quantity: filled,
                remaining_quantity: remaining,
                timestamp,
            });
        }

        order_id
    }

    /// Match incoming order against the opposing side of the book.
    fn match_against_book(
        &mut self,
        taker_id: OrderId,
        taker_account: AccountId,
        side: Side,
        limit_price: Price,
        mut remaining: Decimal,
        timestamp: i64,
    ) -> Decimal {
        let prices_to_remove: Vec<OrderedPrice>;

        match side {
            Side::BUY => {
                prices_to_remove = self.match_buy(
                    taker_id, taker_account, limit_price, &mut remaining, timestamp,
                );
                for p in prices_to_remove {
                    self.asks.remove(&p);
                }
            }
            Side::SELL => {
                prices_to_remove = self.match_sell(
                    taker_id, taker_account, limit_price, &mut remaining, timestamp,
                );
                for p in prices_to_remove {
                    self.bids.remove(&p);
                }
            }
        }

        remaining
    }

    /// Match a BUY order against asks (ascending price order).
    fn match_buy(
        &mut self,
        taker_id: OrderId,
        taker_account: AccountId,
        limit_price: Price,
        remaining: &mut Decimal,
        timestamp: i64,
    ) -> Vec<OrderedPrice> {
        let mut to_remove = Vec::new();

        let keys: Vec<OrderedPrice> = self.asks.keys().cloned().collect();
        for key in keys {
            if *remaining <= Decimal::ZERO {
                break;
            }

            let level = self.asks.get_mut(&key).unwrap();
            let maker_price = level.orders[0].price;

            // BUY: only match if ask price <= limit price
            if maker_price.as_decimal() > limit_price.as_decimal() {
                break;
            }

            self.match_level(
                level, taker_id, taker_account, maker_price, remaining, timestamp,
            );

            if level.is_empty() {
                to_remove.push(key);
            }
        }

        to_remove
    }

    /// Match a SELL order against bids (descending price order).
    fn match_sell(
        &mut self,
        taker_id: OrderId,
        taker_account: AccountId,
        limit_price: Price,
        remaining: &mut Decimal,
        timestamp: i64,
    ) -> Vec<OrderedPrice> {
        let mut to_remove = Vec::new();

        let keys: Vec<OrderedPrice> = self.bids.keys().cloned().collect();
        for key in keys {
            if *remaining <= Decimal::ZERO {
                break;
            }

            let level = self.bids.get_mut(&key).unwrap();
            let maker_price = level.orders[0].price;

            // SELL: only match if bid price >= limit price
            if maker_price.as_decimal() < limit_price.as_decimal() {
                break;
            }

            self.match_level(
                level, taker_id, taker_account, maker_price, remaining, timestamp,
            );

            if level.is_empty() {
                to_remove.push(key);
            }
        }

        to_remove
    }

    /// Match against orders at a single price level (time priority within level).
    fn match_level(
        &mut self,
        level: &mut PriceLevel,
        taker_id: OrderId,
        taker_account: AccountId,
        price: Price,
        remaining: &mut Decimal,
        timestamp: i64,
    ) {
        let mut filled_indices = Vec::new();

        for (i, maker) in level.orders.iter_mut().enumerate() {
            if *remaining <= Decimal::ZERO {
                break;
            }

            let fill_qty = (*remaining).min(maker.remaining);
            let fill_value = fill_qty * price.as_decimal();

            let maker_fee = round_up_fee(fill_value * self.fee_tier.maker_rate);
            let taker_fee = round_up_fee(fill_value * self.fee_tier.taker_rate);

            self.sequence += 1;
            self.events.push(SimEvent::TradeExecuted {
                trade_id: TradeId::new(),
                maker_order_id: maker.order_id,
                taker_order_id: taker_id,
                maker_account_id: maker.account_id,
                taker_account_id: taker_account,
                price,
                quantity: fill_qty,
                maker_fee,
                taker_fee,
                timestamp,
            });

            maker.remaining -= fill_qty;
            *remaining -= fill_qty;

            if maker.remaining == Decimal::ZERO {
                filled_indices.push(i);
                self.events.push(SimEvent::OrderFilled {
                    order_id: maker.order_id,
                    filled_quantity: fill_qty,
                    timestamp,
                });
            } else {
                self.events.push(SimEvent::OrderPartiallyFilled {
                    order_id: maker.order_id,
                    filled_quantity: fill_qty,
                    remaining_quantity: maker.remaining,
                    timestamp,
                });
            }
        }

        // Remove fully filled orders (in reverse to preserve indices)
        for i in filled_indices.into_iter().rev() {
            level.orders.remove(i);
        }
    }

    /// Insert a resting order into the book.
    fn insert_resting(&mut self, entry: BookEntry) {
        match entry.side {
            Side::BUY => {
                let key = OrderedPrice::bid(entry.price);
                self.bids.entry(key).or_insert_with(PriceLevel::new).orders.push(entry);
            }
            Side::SELL => {
                let key = OrderedPrice::ask(entry.price);
                self.asks.entry(key).or_insert_with(PriceLevel::new).orders.push(entry);
            }
        }
    }

    /// Cancel an order by ID. Returns true if found and canceled.
    pub fn cancel_order(&mut self, order_id: OrderId, timestamp: i64) -> bool {
        if let Some(remaining) = self.remove_from_book(&self.bids.clone(), order_id, Side::BUY) {
            self.bids = self.rebuild_without(self.bids.clone(), order_id);
            self.events.push(SimEvent::OrderCanceled {
                order_id,
                remaining_quantity: remaining,
                timestamp,
            });
            return true;
        }

        if let Some(remaining) = self.remove_from_book(&self.asks.clone(), order_id, Side::SELL) {
            self.asks = self.rebuild_without(self.asks.clone(), order_id);
            self.events.push(SimEvent::OrderCanceled {
                order_id,
                remaining_quantity: remaining,
                timestamp,
            });
            return true;
        }

        false
    }

    /// Find an order in a side of the book and return its remaining qty.
    fn remove_from_book(
        &self,
        book: &BTreeMap<OrderedPrice, PriceLevel>,
        order_id: OrderId,
        _side: Side,
    ) -> Option<Decimal> {
        for level in book.values() {
            for entry in &level.orders {
                if entry.order_id == order_id {
                    return Some(entry.remaining);
                }
            }
        }
        None
    }

    /// Rebuild a book side without the given order_id.
    fn rebuild_without(
        &self,
        mut book: BTreeMap<OrderedPrice, PriceLevel>,
        order_id: OrderId,
    ) -> BTreeMap<OrderedPrice, PriceLevel> {
        for level in book.values_mut() {
            level.orders.retain(|e| e.order_id != order_id);
        }
        book.retain(|_, level| !level.is_empty());
        book
    }

    /// Get the best bid price.
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.values().next().and_then(|l| l.orders.first().map(|o| o.price))
    }

    /// Get the best ask price.
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.values().next().and_then(|l| l.orders.first().map(|o| o.price))
    }

    /// Get mid price (average of best bid and ask).
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => {
                Some((bid.as_decimal() + ask.as_decimal()) / Decimal::from(2))
            }
            _ => None,
        }
    }

    /// Get total bid depth.
    pub fn bid_depth(&self) -> Decimal {
        self.bids.values().map(|l| l.total_quantity()).sum()
    }

    /// Get total ask depth.
    pub fn ask_depth(&self) -> Decimal {
        self.asks.values().map(|l| l.total_quantity()).sum()
    }

    /// Get bid levels as (price, quantity) tuples, sorted descending.
    pub fn bid_levels(&self) -> Vec<(Price, Decimal)> {
        self.bids.values()
            .map(|l| (l.orders[0].price, l.total_quantity()))
            .collect()
    }

    /// Get ask levels as (price, quantity) tuples, sorted ascending.
    pub fn ask_levels(&self) -> Vec<(Price, Decimal)> {
        self.asks.values()
            .map(|l| (l.orders[0].price, l.total_quantity()))
            .collect()
    }

    /// Total number of resting orders.
    pub fn order_count(&self) -> usize {
        let bid_count: usize = self.bids.values().map(|l| l.orders.len()).sum();
        let ask_count: usize = self.asks.values().map(|l| l.orders.len()).sum();
        bid_count + ask_count
    }

    /// Clear all events (for replay checkpointing).
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Count trades in event log.
    pub fn trade_count(&self) -> usize {
        self.events.iter().filter(|e| matches!(e, SimEvent::TradeExecuted { .. })).count()
    }
}

/// Round fee UP (never undercharge, spec §7.2).
fn round_up_fee(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(FEE_DP, RoundingStrategy::AwayFromZero)
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::fee::FeeTier;

    fn test_fee_tier() -> FeeTier {
        FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        }
    }

    fn test_engine() -> SimEngine {
        SimEngine::new(MarketId::new("BTC/USDT"), test_fee_tier())
    }

    #[test]
    fn test_insert_and_best_bid_ask() {
        let mut engine = test_engine();
        let acc = AccountId::new();

        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(1), 1000);
        engine.submit_order(acc, Side::BUY, Price::from_u64(49800), Decimal::from(2), 1001);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::from(1), 1002);

        assert_eq!(engine.best_bid(), Some(Price::from_u64(49900)));
        assert_eq!(engine.best_ask(), Some(Price::from_u64(50100)));
        assert_eq!(engine.order_count(), 3);
    }

    #[test]
    fn test_full_match() {
        let mut engine = test_engine();
        let maker = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(maker, Side::SELL, Price::from_u64(50000), Decimal::from(1), 1000);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50000), Decimal::from(1), 1001);

        assert_eq!(engine.order_count(), 0);
        assert_eq!(engine.trade_count(), 1);
    }

    #[test]
    fn test_partial_match() {
        let mut engine = test_engine();
        let maker = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(maker, Side::SELL, Price::from_u64(50000), Decimal::from(3), 1000);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50000), Decimal::from(1), 1001);

        // 2 remaining on the ask
        assert_eq!(engine.ask_depth(), Decimal::from(2));
        assert_eq!(engine.trade_count(), 1);
    }

    #[test]
    fn test_multi_level_match() {
        let mut engine = test_engine();
        let m1 = AccountId::new();
        let m2 = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(m1, Side::SELL, Price::from_u64(50100), Decimal::from(1), 1000);
        engine.submit_order(m2, Side::SELL, Price::from_u64(50200), Decimal::from(1), 1001);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50200), Decimal::from(2), 1002);

        assert_eq!(engine.order_count(), 0);
        assert_eq!(engine.trade_count(), 2);
    }

    #[test]
    fn test_no_cross_above_limit() {
        let mut engine = test_engine();
        let maker = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(maker, Side::SELL, Price::from_u64(50200), Decimal::from(1), 1000);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50100), Decimal::from(1), 1001);

        // No match, both rest
        assert_eq!(engine.order_count(), 2);
        assert_eq!(engine.trade_count(), 0);
    }

    #[test]
    fn test_cancel_order() {
        let mut engine = test_engine();
        let acc = AccountId::new();

        let oid = engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(1), 1000);
        assert_eq!(engine.order_count(), 1);

        let canceled = engine.cancel_order(oid, 1001);
        assert!(canceled);
        assert_eq!(engine.order_count(), 0);
    }

    #[test]
    fn test_cancel_nonexistent() {
        let mut engine = test_engine();
        let fake_id = OrderId::new();
        assert!(!engine.cancel_order(fake_id, 1000));
    }

    #[test]
    fn test_fee_calculation() {
        let mut engine = test_engine();
        let maker = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(maker, Side::SELL, Price::from_u64(50000), Decimal::from(1), 1000);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50000), Decimal::from(1), 1001);

        // Find the trade event
        let trade = engine.events.iter().find(|e| matches!(e, SimEvent::TradeExecuted { .. }));
        match trade {
            Some(SimEvent::TradeExecuted { maker_fee, taker_fee, .. }) => {
                // value = 50000, maker_rate = 0.0002, taker_rate = 0.0005
                assert_eq!(*maker_fee, Decimal::from(10)); // 50000 * 0.0002
                assert_eq!(*taker_fee, Decimal::from(25)); // 50000 * 0.0005
            }
            _ => panic!("Expected TradeExecuted event"),
        }
    }

    #[test]
    fn test_deterministic_replay() {
        let fee = test_fee_tier();
        let acc1 = AccountId::new();
        let acc2 = AccountId::new();

        let run = |engine: &mut SimEngine| {
            engine.submit_order(acc1, Side::SELL, Price::from_u64(50000), Decimal::from(2), 100);
            engine.submit_order(acc1, Side::SELL, Price::from_u64(50100), Decimal::from(3), 101);
            engine.submit_order(acc2, Side::BUY, Price::from_u64(50100), Decimal::from(4), 102);
        };

        let mut e1 = SimEngine::new(MarketId::new("BTC/USDT"), fee.clone());
        let mut e2 = SimEngine::new(MarketId::new("BTC/USDT"), fee);
        run(&mut e1);
        run(&mut e2);

        assert_eq!(e1.bid_depth(), e2.bid_depth());
        assert_eq!(e1.ask_depth(), e2.ask_depth());
        assert_eq!(e1.trade_count(), e2.trade_count());
        assert_eq!(e1.order_count(), e2.order_count());
    }

    #[test]
    fn test_mid_price() {
        let mut engine = test_engine();
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49800), Decimal::from(1), 100);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50200), Decimal::from(1), 101);

        assert_eq!(engine.mid_price(), Some(Decimal::from(50000)));
    }

    #[test]
    fn test_price_time_priority() {
        let mut engine = test_engine();
        let early = AccountId::new();
        let late = AccountId::new();
        let taker = AccountId::new();

        // Same price, early submits first
        engine.submit_order(early, Side::SELL, Price::from_u64(50000), Decimal::from(1), 100);
        engine.submit_order(late, Side::SELL, Price::from_u64(50000), Decimal::from(1), 200);

        // Taker buys 1 — should match early's order first
        engine.submit_order(taker, Side::BUY, Price::from_u64(50000), Decimal::from(1), 300);

        // Only late's order remains
        assert_eq!(engine.ask_depth(), Decimal::from(1));
        let trade = engine.events.iter().find(|e| matches!(e, SimEvent::TradeExecuted { .. }));
        match trade {
            Some(SimEvent::TradeExecuted { maker_account_id, .. }) => {
                assert_eq!(*maker_account_id, early);
            }
            _ => panic!("Expected trade"),
        }
    }
}
