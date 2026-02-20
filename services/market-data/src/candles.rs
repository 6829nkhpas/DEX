//! OHLCV Candle Builder
//!
//! Builds rolling OHLCV (Open, High, Low, Close, Volume) candles from
//! trade events across multiple timeframes simultaneously.
//!
//! Implements spec §9.3.8: OHLCV calculation with 1-second update frequency.
//! Uses `Decimal` for all arithmetic (spec §12.4.1).
//!
//! Candle boundaries are aligned to epoch (e.g., 1m candles close on
//! minute boundaries). Missing candles are backfilled with the previous
//! close price and zero volume.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::MarketId;
use types::numeric::Price;

/// Supported candle timeframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Timeframe {
    /// 1 minute
    M1,
    /// 5 minutes
    M5,
    /// 15 minutes
    M15,
    /// 30 minutes
    M30,
    /// 1 hour
    H1,
    /// 4 hours
    H4,
    /// 1 day
    D1,
    /// 1 week
    W1,
}

impl Timeframe {
    /// Duration of this timeframe in nanoseconds.
    pub fn duration_nanos(&self) -> i64 {
        match self {
            Timeframe::M1 => 60 * 1_000_000_000,
            Timeframe::M5 => 5 * 60 * 1_000_000_000,
            Timeframe::M15 => 15 * 60 * 1_000_000_000,
            Timeframe::M30 => 30 * 60 * 1_000_000_000,
            Timeframe::H1 => 3600 * 1_000_000_000,
            Timeframe::H4 => 4 * 3600 * 1_000_000_000,
            Timeframe::D1 => 86400 * 1_000_000_000_i64,
            Timeframe::W1 => 7 * 86400 * 1_000_000_000_i64,
        }
    }

    /// All standard timeframes.
    pub fn all() -> &'static [Timeframe] {
        &[
            Timeframe::M1,
            Timeframe::M5,
            Timeframe::M15,
            Timeframe::M30,
            Timeframe::H1,
            Timeframe::H4,
            Timeframe::D1,
            Timeframe::W1,
        ]
    }

    /// Align a timestamp to this timeframe's boundary (floor).
    pub fn align_to_boundary(&self, timestamp_nanos: i64) -> i64 {
        let duration = self.duration_nanos();
        (timestamp_nanos / duration) * duration
    }
}

/// A single OHLCV candle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candle {
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub open_time: i64,
    pub close_time: i64,
    pub trade_count: u64,
    pub timeframe: Timeframe,
    pub symbol: MarketId,
}

impl Candle {
    /// Create a new candle from the first trade in this period.
    fn new(
        price: Decimal,
        volume: Decimal,
        open_time: i64,
        timeframe: Timeframe,
        symbol: MarketId,
    ) -> Self {
        let close_time = open_time + timeframe.duration_nanos() - 1;
        Self {
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
            open_time,
            close_time,
            trade_count: 1,
            timeframe,
            symbol,
        }
    }

    /// Update the candle with a new trade.
    fn update(&mut self, price: Decimal, volume: Decimal) {
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.volume += volume;
        self.trade_count += 1;
    }

    /// Create a flat (no-trade) candle for backfill.
    fn flat(
        prev_close: Decimal,
        open_time: i64,
        timeframe: Timeframe,
        symbol: MarketId,
    ) -> Self {
        let close_time = open_time + timeframe.duration_nanos() - 1;
        Self {
            open: prev_close,
            high: prev_close,
            low: prev_close,
            close: prev_close,
            volume: Decimal::ZERO,
            open_time,
            close_time,
            trade_count: 0,
            timeframe,
            symbol,
        }
    }

    /// Validate candle integrity (OHLCV invariants).
    pub fn is_valid(&self) -> bool {
        self.high >= self.open
            && self.high >= self.close
            && self.high >= self.low
            && self.low <= self.open
            && self.low <= self.close
            && self.volume >= Decimal::ZERO
            && self.close_time > self.open_time
    }
}

/// Builds candles for a single timeframe on a single symbol.
pub struct CandleBuilder {
    timeframe: Timeframe,
    symbol: MarketId,
    /// Currently building candle (not yet closed).
    current: Option<Candle>,
    /// Closed candles stored by open_time (BTreeMap for deterministic order).
    closed: BTreeMap<i64, Candle>,
    /// Max closed candles to retain.
    max_history: usize,
}

impl CandleBuilder {
    pub fn new(timeframe: Timeframe, symbol: MarketId, max_history: usize) -> Self {
        Self {
            timeframe,
            symbol,
            current: None,
            closed: BTreeMap::new(),
            max_history,
        }
    }

    /// Process a trade: update or create candle, close candle at boundary.
    ///
    /// Returns a closed candle if the trade crosses a boundary.
    pub fn process_trade(
        &mut self,
        price: Price,
        quantity: Decimal,
        timestamp: i64,
    ) -> Option<Candle> {
        let price_dec = price.as_decimal();
        let boundary = self.timeframe.align_to_boundary(timestamp);

        // Check if we need to close the current candle
        let mut closed_candle = None;

        if let Some(ref current) = self.current {
            let current_boundary =
                self.timeframe.align_to_boundary(current.open_time);
            if boundary > current_boundary {
                // Close the current candle
                closed_candle = self.close_current();
            }
        }

        // Update or create candle for this boundary
        match &mut self.current {
            Some(candle) => candle.update(price_dec, quantity),
            None => {
                self.current = Some(Candle::new(
                    price_dec,
                    quantity,
                    boundary,
                    self.timeframe,
                    self.symbol.clone(),
                ));
            }
        }

        closed_candle
    }

    /// Force-close the current candle (e.g., on timer).
    pub fn close_current(&mut self) -> Option<Candle> {
        if let Some(candle) = self.current.take() {
            let open_time = candle.open_time;
            self.closed.insert(open_time, candle.clone());
            self.trim_history();
            Some(candle)
        } else {
            None
        }
    }

    /// Backfill missing candles between `from` and `to` timestamps
    /// using the given previous close price.
    pub fn backfill(
        &mut self,
        prev_close: Decimal,
        from_nanos: i64,
        to_nanos: i64,
    ) -> Vec<Candle> {
        let duration = self.timeframe.duration_nanos();
        let mut backfilled = Vec::new();
        let mut t = self.timeframe.align_to_boundary(from_nanos) + duration;

        while t < to_nanos {
            if !self.closed.contains_key(&t) {
                let candle = Candle::flat(
                    prev_close,
                    t,
                    self.timeframe,
                    self.symbol.clone(),
                );
                self.closed.insert(t, candle.clone());
                backfilled.push(candle);
            }
            t += duration;
        }

        self.trim_history();
        backfilled
    }

    /// Get closed candles in chronological order.
    pub fn get_candles(&self, limit: usize) -> Vec<Candle> {
        self.closed.values().rev().take(limit).cloned().collect()
    }

    /// Get the current (unclosed) candle.
    pub fn current_candle(&self) -> Option<&Candle> {
        self.current.as_ref()
    }

    /// Trim history to max_history.
    fn trim_history(&mut self) {
        while self.closed.len() > self.max_history {
            self.closed.pop_first();
        }
    }
}

/// Manages candle builders across all timeframes for a single symbol.
pub struct MultiTimeframeCandleManager {
    builders: BTreeMap<Timeframe, CandleBuilder>,
    symbol: MarketId,
}

impl MultiTimeframeCandleManager {
    /// Create managers for all standard timeframes.
    pub fn new(symbol: MarketId, max_history_per_tf: usize) -> Self {
        let mut builders = BTreeMap::new();
        for &tf in Timeframe::all() {
            builders.insert(
                tf,
                CandleBuilder::new(tf, symbol.clone(), max_history_per_tf),
            );
        }
        Self { builders, symbol }
    }

    /// Process a trade across all timeframes.
    ///
    /// Returns closed candles (if any) from each timeframe.
    pub fn process_trade(
        &mut self,
        price: Price,
        quantity: Decimal,
        timestamp: i64,
    ) -> Vec<Candle> {
        let mut closed = Vec::new();
        for builder in self.builders.values_mut() {
            if let Some(candle) = builder.process_trade(price, quantity, timestamp)
            {
                closed.push(candle);
            }
        }
        closed
    }

    /// Get candles for a specific timeframe.
    pub fn get_candles(
        &self,
        timeframe: Timeframe,
        limit: usize,
    ) -> Vec<Candle> {
        self.builders
            .get(&timeframe)
            .map(|b| b.get_candles(limit))
            .unwrap_or_default()
    }

    /// Symbol managed by this instance.
    pub fn symbol(&self) -> &MarketId {
        &self.symbol
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::numeric::Quantity;

    fn nanos(minutes: i64) -> i64 {
        minutes * 60 * 1_000_000_000
    }

    #[test]
    fn test_timeframe_duration() {
        assert_eq!(Timeframe::M1.duration_nanos(), 60_000_000_000);
        assert_eq!(Timeframe::H1.duration_nanos(), 3_600_000_000_000);
        assert_eq!(
            Timeframe::D1.duration_nanos(),
            86_400_000_000_000
        );
    }

    #[test]
    fn test_timeframe_alignment() {
        let ts = nanos(5) + 30_000_000_000; // 5m30s
        assert_eq!(Timeframe::M1.align_to_boundary(ts), nanos(5));
        assert_eq!(Timeframe::M5.align_to_boundary(ts), nanos(5));
        assert_eq!(Timeframe::M15.align_to_boundary(ts), nanos(0));
    }

    #[test]
    fn test_candle_creation() {
        let candle = Candle::new(
            Decimal::from(50000),
            Decimal::from(1),
            nanos(0),
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
        );

        assert_eq!(candle.open, Decimal::from(50000));
        assert_eq!(candle.high, Decimal::from(50000));
        assert_eq!(candle.low, Decimal::from(50000));
        assert_eq!(candle.close, Decimal::from(50000));
        assert_eq!(candle.trade_count, 1);
        assert!(candle.is_valid());
    }

    #[test]
    fn test_candle_update() {
        let mut candle = Candle::new(
            Decimal::from(50000),
            Decimal::from(1),
            nanos(0),
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
        );

        candle.update(Decimal::from(51000), Decimal::from(2)); // New high
        candle.update(Decimal::from(49000), Decimal::from(3)); // New low
        candle.update(Decimal::from(50500), Decimal::from(1)); // Close

        assert_eq!(candle.open, Decimal::from(50000));
        assert_eq!(candle.high, Decimal::from(51000));
        assert_eq!(candle.low, Decimal::from(49000));
        assert_eq!(candle.close, Decimal::from(50500));
        assert_eq!(candle.volume, Decimal::from(7));
        assert_eq!(candle.trade_count, 4);
        assert!(candle.is_valid());
    }

    #[test]
    fn test_candle_builder_basic() {
        let mut builder = CandleBuilder::new(
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
            100,
        );

        // Trade within the first minute
        let result = builder.process_trade(
            Price::from_u64(50000),
            Decimal::from(1),
            nanos(0) + 10_000_000_000, // 10 seconds
        );
        assert!(result.is_none()); // No candle closed yet

        let current = builder.current_candle().unwrap();
        assert_eq!(current.open, Decimal::from(50000));
    }

    #[test]
    fn test_candle_close_at_boundary() {
        let mut builder = CandleBuilder::new(
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
            100,
        );

        // Trade in minute 0
        builder.process_trade(
            Price::from_u64(50000),
            Decimal::from(1),
            nanos(0) + 10_000_000_000,
        );

        // Trade in minute 1 → should close minute 0 candle
        let closed = builder.process_trade(
            Price::from_u64(51000),
            Decimal::from(2),
            nanos(1) + 5_000_000_000,
        );

        assert!(closed.is_some());
        let closed_candle = closed.unwrap();
        assert_eq!(closed_candle.open, Decimal::from(50000));
        assert_eq!(closed_candle.close, Decimal::from(50000));
        assert_eq!(closed_candle.trade_count, 1);
    }

    #[test]
    fn test_candle_backfill() {
        let mut builder = CandleBuilder::new(
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
            100,
        );

        let backfilled = builder.backfill(
            Decimal::from(50000),
            nanos(0),
            nanos(3),
        );

        // Should create flat candles for minutes 1 and 2
        assert_eq!(backfilled.len(), 2);
        for candle in &backfilled {
            assert_eq!(candle.open, Decimal::from(50000));
            assert_eq!(candle.close, Decimal::from(50000));
            assert_eq!(candle.volume, Decimal::ZERO);
            assert_eq!(candle.trade_count, 0);
            assert!(candle.is_valid());
        }
    }

    #[test]
    fn test_candle_integrity_validation() {
        let valid = Candle::new(
            Decimal::from(50000),
            Decimal::from(1),
            nanos(0),
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
        );
        assert!(valid.is_valid());

        // Manually create invalid candle
        let invalid = Candle {
            open: Decimal::from(50000),
            high: Decimal::from(49000), // High < Open → invalid
            low: Decimal::from(48000),
            close: Decimal::from(49500),
            volume: Decimal::from(1),
            open_time: nanos(0),
            close_time: nanos(0) + Timeframe::M1.duration_nanos() - 1,
            trade_count: 1,
            timeframe: Timeframe::M1,
            symbol: MarketId::new("BTC/USDT"),
        };
        assert!(!invalid.is_valid());
    }

    #[test]
    fn test_multi_timeframe_manager() {
        let mut manager = MultiTimeframeCandleManager::new(
            MarketId::new("BTC/USDT"),
            100,
        );

        // Trade at t=10s
        let closed = manager.process_trade(
            Price::from_u64(50000),
            Decimal::from(1),
            10_000_000_000,
        );
        assert!(closed.is_empty()); // No candles closed yet

        // Trade at t=61s (crosses M1 boundary)
        let closed = manager.process_trade(
            Price::from_u64(51000),
            Decimal::from(2),
            nanos(1) + 1_000_000_000,
        );

        // M1 should have a closed candle
        assert!(closed.iter().any(|c| c.timeframe == Timeframe::M1));
    }

    #[test]
    fn test_candle_serialization() {
        let candle = Candle::new(
            Decimal::from(50000),
            Decimal::from(1),
            nanos(0),
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
        );

        let json = serde_json::to_string(&candle).unwrap();
        let deserialized: Candle = serde_json::from_str(&json).unwrap();
        assert_eq!(candle, deserialized);
    }

    #[test]
    fn test_candle_history_limit() {
        let mut builder = CandleBuilder::new(
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
            3, // Keep only 3 candles
        );

        // Create 5 candles by trading across 5 minutes
        for minute in 0..5 {
            builder.process_trade(
                Price::from_u64(50000),
                Decimal::from(1),
                nanos(minute) + 5_000_000_000,
            );
        }
        // Close the last one
        builder.close_current();

        let candles = builder.get_candles(10);
        assert!(candles.len() <= 3);
    }
}
