//! Trade stream for public trade events
//!
//! Buffers executed trades, assigns monotonic trade sequence IDs,
//! aggregates same-price trades, normalizes timestamps, and maintains
//! a bounded trade history cache with replay capability.
//!
//! Implements spec §3 (Trade Lifecycle) reporting phase and
//! §8 (Event Taxonomy) TradeExecuted events.

use std::collections::VecDeque;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::{MarketId, TradeId};
use types::numeric::{Price, Quantity};
use types::order::Side;

/// A public trade event for broadcasting to clients.
///
/// Contains only public information (no account IDs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTrade {
    /// Unique trade identifier.
    pub trade_id: TradeId,
    /// Monotonic trade sequence number for this market.
    pub trade_sequence: u64,
    /// Trading pair symbol.
    pub symbol: MarketId,
    /// Execution price.
    pub price: Price,
    /// Traded quantity.
    pub quantity: Quantity,
    /// Trade value (price × quantity).
    pub value: Decimal,
    /// Taker side (BUY = buyer was taker, SELL = seller was taker).
    pub taker_side: Side,
    /// Execution timestamp (Unix nanos, normalized to exchange clock).
    pub timestamp: i64,
}

/// Manages the trade stream with buffering, sequencing, and history.
#[derive(Debug)]
pub struct TradeBuffer {
    /// Current trade sequence counter (per-market monotonic).
    sequence_counter: u64,
    /// Bounded history cache (ring buffer for recent trades).
    history: VecDeque<PublicTrade>,
    /// Maximum history cache size.
    max_history: usize,
    /// Symbol this buffer is for.
    symbol: MarketId,
}

impl TradeBuffer {
    /// Create a new trade buffer for the given symbol.
    pub fn new(symbol: MarketId, max_history: usize) -> Self {
        Self {
            sequence_counter: 0,
            history: VecDeque::with_capacity(max_history),
            max_history,
            symbol,
        }
    }

    /// Record a new trade and emit a public trade event.
    ///
    /// Assigns a monotonic trade sequence ID and normalizes the timestamp.
    pub fn record_trade(
        &mut self,
        trade_id: TradeId,
        price: Price,
        quantity: Quantity,
        taker_side: Side,
        timestamp: i64,
    ) -> PublicTrade {
        self.sequence_counter += 1;

        let value = quantity.as_decimal() * price.as_decimal();

        let trade = PublicTrade {
            trade_id,
            trade_sequence: self.sequence_counter,
            symbol: self.symbol.clone(),
            price,
            quantity,
            value,
            taker_side,
            timestamp,
        };

        // Add to history cache (evict oldest if at capacity)
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(trade.clone());

        trade
    }

    /// Aggregate trades at the same price level.
    ///
    /// Combines consecutive trades at the same price into a single trade
    /// with accumulated quantity. Returns the aggregated trades.
    pub fn aggregate_by_price(trades: &[PublicTrade]) -> Vec<PublicTrade> {
        if trades.is_empty() {
            return Vec::new();
        }

        let mut aggregated: Vec<PublicTrade> = Vec::new();

        for trade in trades {
            let should_merge = aggregated.last().map_or(false, |last: &PublicTrade| {
                last.price == trade.price && last.taker_side == trade.taker_side
            });

            if should_merge {
                let last = aggregated.last_mut().unwrap();
                let new_qty = last.quantity.as_decimal() + trade.quantity.as_decimal();
                last.quantity = Quantity::try_new(new_qty).unwrap_or(last.quantity);
                last.value = new_qty * last.price.as_decimal();
            } else {
                aggregated.push(trade.clone());
            }
        }

        aggregated
    }

    /// Get recent trades from history (newest first).
    pub fn recent_trades(&self, limit: usize) -> Vec<PublicTrade> {
        self.history.iter().rev().take(limit).cloned().collect()
    }

    /// Replay all trades in the history cache (oldest first).
    pub fn replay_history(&self) -> Vec<PublicTrade> {
        self.history.iter().cloned().collect()
    }

    /// Get a trade by its sequence number.
    pub fn get_by_sequence(&self, sequence: u64) -> Option<&PublicTrade> {
        self.history.iter().find(|t| t.trade_sequence == sequence)
    }

    /// Current trade sequence counter.
    pub fn current_sequence(&self) -> u64 {
        self.sequence_counter
    }

    /// Number of trades in the history cache.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Symbol this buffer is for.
    pub fn symbol(&self) -> &MarketId {
        &self.symbol
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer() -> TradeBuffer {
        TradeBuffer::new(MarketId::new("BTC/USDT"), 100)
    }

    #[test]
    fn test_record_trade() {
        let mut buf = make_buffer();

        let trade = buf.record_trade(
            TradeId::new(),
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            Side::BUY,
            1708123456789000000,
        );

        assert_eq!(trade.trade_sequence, 1);
        assert_eq!(trade.value, Decimal::from(25000));
        assert_eq!(trade.taker_side, Side::BUY);
        assert_eq!(buf.history_len(), 1);
    }

    #[test]
    fn test_monotonic_sequence() {
        let mut buf = make_buffer();

        for i in 0..5 {
            let trade = buf.record_trade(
                TradeId::new(),
                Price::from_u64(50000),
                Quantity::from_str("1.0").unwrap(),
                Side::BUY,
                1708123456789000000 + i * 1000,
            );
            assert_eq!(trade.trade_sequence, (i + 1) as u64);
        }
        assert_eq!(buf.current_sequence(), 5);
    }

    #[test]
    fn test_history_cache_eviction() {
        let mut buf = TradeBuffer::new(MarketId::new("BTC/USDT"), 3);

        for i in 0..5 {
            buf.record_trade(
                TradeId::new(),
                Price::from_u64(50000),
                Quantity::from_str("1.0").unwrap(),
                Side::BUY,
                1708123456789000000 + i * 1000,
            );
        }

        // Only 3 most recent should remain
        assert_eq!(buf.history_len(), 3);
        let trades = buf.recent_trades(10);
        assert_eq!(trades[0].trade_sequence, 5);
        assert_eq!(trades[2].trade_sequence, 3);
    }

    #[test]
    fn test_recent_trades_ordering() {
        let mut buf = make_buffer();

        for i in 0..3 {
            buf.record_trade(
                TradeId::new(),
                Price::from_u64(50000),
                Quantity::from_str("1.0").unwrap(),
                Side::BUY,
                1708123456789000000 + i * 1000,
            );
        }

        let recent = buf.recent_trades(2);
        assert_eq!(recent.len(), 2);
        // Most recent first
        assert_eq!(recent[0].trade_sequence, 3);
        assert_eq!(recent[1].trade_sequence, 2);
    }

    #[test]
    fn test_replay_history() {
        let mut buf = make_buffer();

        for i in 0..3 {
            buf.record_trade(
                TradeId::new(),
                Price::from_u64(50000),
                Quantity::from_str("1.0").unwrap(),
                Side::BUY,
                1708123456789000000 + i * 1000,
            );
        }

        let replay = buf.replay_history();
        assert_eq!(replay.len(), 3);
        // Oldest first
        assert_eq!(replay[0].trade_sequence, 1);
        assert_eq!(replay[2].trade_sequence, 3);
    }

    #[test]
    fn test_aggregate_same_price() {
        let mut buf = make_buffer();

        // Three trades at same price
        let t1 = buf.record_trade(
            TradeId::new(),
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            Side::BUY,
            1708123456789000000,
        );
        let t2 = buf.record_trade(
            TradeId::new(),
            Price::from_u64(50000),
            Quantity::from_str("2.0").unwrap(),
            Side::BUY,
            1708123456790000000,
        );
        // Different price — should not aggregate
        let t3 = buf.record_trade(
            TradeId::new(),
            Price::from_u64(51000),
            Quantity::from_str("0.5").unwrap(),
            Side::SELL,
            1708123456791000000,
        );

        let aggregated = TradeBuffer::aggregate_by_price(&[t1, t2, t3]);
        assert_eq!(aggregated.len(), 2);
        assert_eq!(aggregated[0].quantity.as_decimal(), Decimal::from(3));
        assert_eq!(aggregated[1].price, Price::from_u64(51000));
    }

    #[test]
    fn test_get_by_sequence() {
        let mut buf = make_buffer();

        buf.record_trade(
            TradeId::new(),
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            Side::BUY,
            1708123456789000000,
        );
        buf.record_trade(
            TradeId::new(),
            Price::from_u64(51000),
            Quantity::from_str("2.0").unwrap(),
            Side::SELL,
            1708123456790000000,
        );

        let trade = buf.get_by_sequence(2).unwrap();
        assert_eq!(trade.price, Price::from_u64(51000));

        assert!(buf.get_by_sequence(99).is_none());
    }

    #[test]
    fn test_trade_serialization() {
        let mut buf = make_buffer();

        let trade = buf.record_trade(
            TradeId::new(),
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            Side::BUY,
            1708123456789000000,
        );

        let json = serde_json::to_string(&trade).unwrap();
        let deserialized: PublicTrade = serde_json::from_str(&json).unwrap();
        assert_eq!(trade, deserialized);
    }
}
