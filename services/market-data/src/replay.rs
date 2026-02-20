//! Replay and recovery engine for the Market Data Service
//!
//! Rebuilds order book state, candles, and trade cache from an event
//! journal on boot or after crash. Validates state via checksums and
//! detects journal corruption.
//!
//! Implements spec §11 (Replay Requirements):
//! - Full state reconstruction from event log
//! - Deterministic replay (same inputs → same outputs)
//! - State checksum validation post-replay

use std::time::Instant;


use sha2::{Digest, Sha256};
use tracing::{error, info};


use crate::events::{MarketEvent, MarketEventPayload};
use crate::order_book::OrderBookState;
use crate::trades::TradeBuffer;

/// Metrics collected during replay.
#[derive(Debug, Clone)]
pub struct ReplayMetrics {
    /// Total events replayed.
    pub events_replayed: u64,
    /// Duration of replay in milliseconds.
    pub duration_ms: u128,
    /// Events per second during replay.
    pub events_per_second: f64,
    /// State checksum after replay.
    pub state_checksum: String,
}

/// Result of a replay operation.
#[derive(Debug)]
pub struct ReplayResult {
    /// Rebuilt order books by symbol.
    pub books: std::collections::BTreeMap<String, OrderBookState>,
    /// Rebuilt trade buffers by symbol.
    pub trade_buffers: std::collections::BTreeMap<String, TradeBuffer>,
    /// Replay metrics.
    pub metrics: ReplayMetrics,
}

/// Errors during replay.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    #[error("journal corruption detected at sequence {sequence}: {reason}")]
    JournalCorruption { sequence: u64, reason: String },

    #[error("state checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("sequence gap detected: expected {expected}, got {actual}")]
    SequenceGap { expected: u64, actual: u64 },
}

/// Replays events from a journal to rebuild service state.
///
/// Deterministic: same events in same order → identical state (spec §12).
pub struct ReplayEngine {
    /// Expected state checksum for validation (if known).
    expected_checksum: Option<String>,
    /// Whether to enforce strict sequence ordering.
    strict_ordering: bool,
}

impl ReplayEngine {
    pub fn new() -> Self {
        Self {
            expected_checksum: None,
            strict_ordering: true,
        }
    }

    /// Set the expected state checksum for post-replay validation.
    pub fn with_expected_checksum(mut self, checksum: String) -> Self {
        self.expected_checksum = Some(checksum);
        self
    }

    /// Disable strict ordering (for recovery scenarios).
    pub fn with_relaxed_ordering(mut self) -> Self {
        self.strict_ordering = false;
        self
    }

    /// Replay a sequence of events and rebuild all state.
    ///
    /// Events must be sorted by sequence number.
    pub fn replay(&self, events: &[MarketEvent]) -> Result<ReplayResult, ReplayError> {
        let start = Instant::now();

        info!(event_count = events.len(), "Starting event replay");

        let mut books: std::collections::BTreeMap<String, OrderBookState> =
            std::collections::BTreeMap::new();
        let mut trade_buffers: std::collections::BTreeMap<String, TradeBuffer> =
            std::collections::BTreeMap::new();

        let mut last_sequence: Option<u64> = None;
        let mut events_replayed: u64 = 0;

        for event in events {
            // Validate ordering
            if self.strict_ordering {
                if let Some(last) = last_sequence {
                    if event.sequence <= last {
                        return Err(ReplayError::JournalCorruption {
                            sequence: event.sequence,
                            reason: format!(
                                "Non-monotonic sequence: {} after {}",
                                event.sequence, last
                            ),
                        });
                    }
                    if event.sequence > last + 1 {
                        return Err(ReplayError::SequenceGap {
                            expected: last + 1,
                            actual: event.sequence,
                        });
                    }
                }
            }

            self.apply_event(event, &mut books, &mut trade_buffers);

            last_sequence = Some(event.sequence);
            events_replayed += 1;
        }

        let duration_ms = start.elapsed().as_millis();
        let events_per_second = if duration_ms > 0 {
            (events_replayed as f64 / duration_ms as f64) * 1000.0
        } else {
            events_replayed as f64
        };

        let state_checksum = compute_state_checksum(&books);

        // Validate checksum if expected
        if let Some(ref expected) = self.expected_checksum {
            if &state_checksum != expected {
                error!(
                    expected = %expected,
                    actual = %state_checksum,
                    "State checksum mismatch after replay"
                );
                return Err(ReplayError::ChecksumMismatch {
                    expected: expected.clone(),
                    actual: state_checksum,
                });
            }
        }

        let metrics = ReplayMetrics {
            events_replayed,
            duration_ms,
            events_per_second,
            state_checksum,
        };

        info!(
            events_replayed = metrics.events_replayed,
            duration_ms = metrics.duration_ms,
            eps = %format!("{:.0}", metrics.events_per_second),
            "Replay completed successfully"
        );

        Ok(ReplayResult {
            books,
            trade_buffers,
            metrics,
        })
    }

    /// Apply a single event to the appropriate state.
    fn apply_event(
        &self,
        event: &MarketEvent,
        books: &mut std::collections::BTreeMap<String, OrderBookState>,
        trade_buffers: &mut std::collections::BTreeMap<String, TradeBuffer>,
    ) {
        match &event.payload {
            MarketEventPayload::OrderAccepted {
                order_id,
                symbol,
                side,
                price,
                quantity,
                ..
            } => {
                let book = books
                    .entry(symbol.as_str().to_string())
                    .or_insert_with(|| OrderBookState::new(symbol.clone()));
                book.apply_order_accepted(
                    *order_id,
                    *side,
                    *price,
                    *quantity,
                    event.sequence,
                );
            }
            MarketEventPayload::TradeExecuted {
                trade_id,
                symbol,
                maker_order_id,
                price,
                quantity,
                side,
                executed_at,
                ..
            } => {
                // Update book
                if let Some(book) = books.get_mut(symbol.as_str()) {
                    book.apply_trade_executed(*maker_order_id, *quantity, event.sequence);
                }

                // Record trade
                let buffer = trade_buffers
                    .entry(symbol.as_str().to_string())
                    .or_insert_with(|| TradeBuffer::new(symbol.clone(), 10000));
                buffer.record_trade(*trade_id, *price, *quantity, *side, *executed_at);
            }
            MarketEventPayload::OrderCanceled {
                order_id,
                remaining_quantity,
                ..
            } => {
                // Apply cancel to all books (we don't know which symbol without lookup)
                for book in books.values_mut() {
                    book.apply_cancel(*order_id, *remaining_quantity, event.sequence);
                }
            }
            MarketEventPayload::OrderPartiallyFilled { .. }
            | MarketEventPayload::OrderFilled { .. } => {
                // These are informational; actual book update happens via TradeExecuted
            }
        }
    }
}

impl Default for ReplayEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a checksum over the full state of all books.
fn compute_state_checksum(
    books: &std::collections::BTreeMap<String, OrderBookState>,
) -> String {
    let mut hasher = Sha256::new();

    for (symbol, book) in books {
        hasher.update(symbol.as_bytes());
        hasher.update(b"|");

        for level in book.bid_levels() {
            hasher.update(level.price.to_string().as_bytes());
            hasher.update(b":");
            hasher.update(level.total_quantity.to_string().as_bytes());
            hasher.update(b",");
        }
        hasher.update(b"---");

        for level in book.ask_levels() {
            hasher.update(level.price.to_string().as_bytes());
            hasher.update(b":");
            hasher.update(level.total_quantity.to_string().as_bytes());
            hasher.update(b",");
        }
        hasher.update(b"===");
    }

    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MarketEventPayload;
    use rust_decimal::Decimal;
    use types::ids::{AccountId, MarketId, OrderId, TradeId};
    use types::numeric::{Price, Quantity};
    use types::order::Side;
    use uuid::Uuid;

    fn make_event(seq: u64, payload: MarketEventPayload) -> MarketEvent {
        MarketEvent {
            event_id: Uuid::now_v7(),
            sequence: seq,
            timestamp: 1708123456789000000 + (seq as i64 * 1000),
            source: "matching-engine".to_string(),
            payload,
            schema_version: "1.0.0".to_string(),
            correlation_id: Uuid::now_v7(),
        }
    }

    fn order_accepted(seq: u64, order_id: OrderId, side: Side, price: u64) -> MarketEvent {
        make_event(
            seq,
            MarketEventPayload::OrderAccepted {
                order_id,
                account_id: AccountId::new(),
                symbol: MarketId::new("BTC/USDT"),
                side,
                price: Price::from_u64(price),
                quantity: Quantity::from_str("1.0").unwrap(),
            },
        )
    }

    fn trade_executed(seq: u64, maker_id: OrderId) -> MarketEvent {
        make_event(
            seq,
            MarketEventPayload::TradeExecuted {
                trade_id: TradeId::new(),
                symbol: MarketId::new("BTC/USDT"),
                maker_order_id: maker_id,
                taker_order_id: OrderId::new(),
                maker_account_id: AccountId::new(),
                taker_account_id: AccountId::new(),
                price: Price::from_u64(50000),
                quantity: Quantity::from_str("0.5").unwrap(),
                side: Side::BUY,
                executed_at: 1708123456789000000,
            },
        )
    }

    #[test]
    fn test_basic_replay() {
        let order_id = OrderId::new();
        let events = vec![
            order_accepted(1, order_id, Side::BUY, 50000),
            order_accepted(2, OrderId::new(), Side::SELL, 51000),
        ];

        let engine = ReplayEngine::new();
        let result = engine.replay(&events).unwrap();

        assert_eq!(result.metrics.events_replayed, 2);
        assert!(result.books.contains_key("BTC/USDT"));

        let book = &result.books["BTC/USDT"];
        assert_eq!(book.bid_depth(), 1);
        assert_eq!(book.ask_depth(), 1);
    }

    #[test]
    fn test_replay_with_trades() {
        let maker_id = OrderId::new();
        let events = vec![
            order_accepted(1, maker_id, Side::SELL, 50000),
            trade_executed(2, maker_id),
        ];

        let engine = ReplayEngine::new();
        let result = engine.replay(&events).unwrap();

        let book = &result.books["BTC/USDT"];
        // Partially filled: 1.0 - 0.5 = 0.5 remaining
        let levels = book.ask_levels();
        assert_eq!(levels[0].total_quantity, Decimal::from_str_exact("0.5").unwrap());

        let trades = &result.trade_buffers["BTC/USDT"];
        assert_eq!(trades.history_len(), 1);
    }

    #[test]
    fn test_replay_detects_sequence_gap() {
        let events = vec![
            order_accepted(1, OrderId::new(), Side::BUY, 50000),
            order_accepted(5, OrderId::new(), Side::BUY, 49000), //  Gap: 2,3,4 missing
        ];

        let engine = ReplayEngine::new();
        let result = engine.replay(&events);
        assert!(result.is_err());

        match result.unwrap_err() {
            ReplayError::SequenceGap { expected, actual } => {
                assert_eq!(expected, 2);
                assert_eq!(actual, 5);
            }
            err => panic!("Expected SequenceGap, got {:?}", err),
        }
    }

    #[test]
    fn test_replay_detects_corruption() {
        let events = vec![
            order_accepted(5, OrderId::new(), Side::BUY, 50000),
            order_accepted(3, OrderId::new(), Side::BUY, 49000), // Out of order
        ];

        let engine = ReplayEngine::new();
        let result = engine.replay(&events);
        assert!(result.is_err());

        match result.unwrap_err() {
            ReplayError::JournalCorruption { sequence, .. } => {
                assert_eq!(sequence, 3);
            }
            err => panic!("Expected JournalCorruption, got {:?}", err),
        }
    }

    #[test]
    fn test_deterministic_replay() {
        let order_id = OrderId::new();
        let events = vec![
            order_accepted(1, order_id, Side::BUY, 50000),
            order_accepted(2, OrderId::new(), Side::SELL, 51000),
            trade_executed(3, order_id),
        ];

        let engine = ReplayEngine::new();
        let result1 = engine.replay(&events).unwrap();
        let result2 = engine.replay(&events).unwrap();

        assert_eq!(result1.metrics.state_checksum, result2.metrics.state_checksum);
    }

    #[test]
    fn test_checksum_validation_pass() {
        let events = vec![order_accepted(1, OrderId::new(), Side::BUY, 50000)];

        let engine = ReplayEngine::new();
        let result = engine.replay(&events).unwrap();
        let checksum = result.metrics.state_checksum.clone();

        // Replay with expected checksum
        let engine2 = ReplayEngine::new().with_expected_checksum(checksum);
        let result2 = engine2.replay(&events);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_checksum_validation_fail() {
        let events = vec![order_accepted(1, OrderId::new(), Side::BUY, 50000)];

        let engine = ReplayEngine::new().with_expected_checksum("bad_checksum".to_string());
        let result = engine.replay(&events);
        assert!(result.is_err());

        match result.unwrap_err() {
            ReplayError::ChecksumMismatch { .. } => {}
            err => panic!("Expected ChecksumMismatch, got {:?}", err),
        }
    }

    #[test]
    fn test_relaxed_ordering() {
        let events = vec![
            order_accepted(1, OrderId::new(), Side::BUY, 50000),
            order_accepted(5, OrderId::new(), Side::BUY, 49000), // Gap OK in relaxed
        ];

        let engine = ReplayEngine::new().with_relaxed_ordering();
        let result = engine.replay(&events);
        assert!(result.is_ok());
    }
}
