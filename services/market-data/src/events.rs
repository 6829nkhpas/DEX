//! Internal event definitions for the Market Data Service
//!
//! Defines the `MarketEvent` enum representing all events consumed by this
//! service from the matching engine. Each variant maps to spec §8 (Event
//! Taxonomy) event types.
//!
//! Uses `Ord` on sequence for deterministic ordering per §12 and §14.

use serde::{Deserialize, Serialize};
use types::ids::{AccountId, MarketId, OrderId, TradeId};
use types::numeric::{Price, Quantity};
use types::order::Side;
use uuid::Uuid;

/// Source of a cancel action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CancelSource {
    /// Canceled by the user
    User,
    /// Canceled by the system (e.g., liquidation, risk breach)
    System,
}

/// Internal event enum for all events consumed by the Market Data Service.
///
/// Each event carries a monotonic sequence number (spec §14) and a timestamp
/// in Unix nanoseconds (spec §13). The service uses these for ordering,
/// deduplication, and gap detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketEvent {
    /// Unique event identifier (UUID v7)
    pub event_id: Uuid,
    /// Global monotonic sequence number (spec §14.2.1)
    pub sequence: u64,
    /// Unix nanoseconds timestamp from exchange clock (spec §13)
    pub timestamp: i64,
    /// Source service that emitted this event
    pub source: String,
    /// Event-specific payload
    pub payload: MarketEventPayload,
    /// Schema version for backward compatibility (spec §9)
    pub schema_version: String,
    /// Correlation ID for request tracing
    pub correlation_id: Uuid,
}

/// Event-specific payloads per spec §8 (Event Taxonomy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum MarketEventPayload {
    /// An order was accepted and placed on the book (spec §8.3.1)
    OrderAccepted {
        order_id: OrderId,
        account_id: AccountId,
        symbol: MarketId,
        side: Side,
        price: Price,
        quantity: Quantity,
    },

    /// A trade was executed between maker and taker (spec §8.3.2)
    TradeExecuted {
        trade_id: TradeId,
        symbol: MarketId,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker_account_id: AccountId,
        taker_account_id: AccountId,
        price: Price,
        quantity: Quantity,
        /// Side from taker perspective
        side: Side,
        executed_at: i64,
    },

    /// An order was partially filled (spec §8.3.1)
    OrderPartiallyFilled {
        order_id: OrderId,
        filled_quantity: Quantity,
        remaining_quantity: Quantity,
        average_price: Price,
    },

    /// An order was completely filled (spec §8.3.1)
    OrderFilled {
        order_id: OrderId,
        filled_quantity: Quantity,
        average_price: Price,
    },

    /// An order was canceled (spec §8.3.1)
    OrderCanceled {
        order_id: OrderId,
        symbol: MarketId,
        side: Side,
        price: Price,
        remaining_quantity: Quantity,
        canceled_by: CancelSource,
        reason: String,
    },
}

impl MarketEvent {
    /// Extract the symbol from the event if present.
    pub fn symbol(&self) -> Option<&MarketId> {
        match &self.payload {
            MarketEventPayload::OrderAccepted { symbol, .. } => Some(symbol),
            MarketEventPayload::TradeExecuted { symbol, .. } => Some(symbol),
            MarketEventPayload::OrderCanceled { symbol, .. } => Some(symbol),
            MarketEventPayload::OrderPartiallyFilled { .. } => None,
            MarketEventPayload::OrderFilled { .. } => None,
        }
    }

    /// Get the event type as a string label for logging.
    pub fn event_type_label(&self) -> &'static str {
        match &self.payload {
            MarketEventPayload::OrderAccepted { .. } => "OrderAccepted",
            MarketEventPayload::TradeExecuted { .. } => "TradeExecuted",
            MarketEventPayload::OrderPartiallyFilled { .. } => "OrderPartiallyFilled",
            MarketEventPayload::OrderFilled { .. } => "OrderFilled",
            MarketEventPayload::OrderCanceled { .. } => "OrderCanceled",
        }
    }
}

/// Implement ordering by sequence number for deterministic processing (spec §14)
impl Ord for MarketEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence.cmp(&other.sequence)
    }
}

impl PartialOrd for MarketEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Request to recover missing events from the event store.
///
/// Emitted when the ingestion layer detects a gap in sequence numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRequest {
    /// First missing sequence number (inclusive)
    pub from_sequence: u64,
    /// Last missing sequence number (inclusive)
    pub to_sequence: u64,
    /// Timestamp when gap was detected
    pub detected_at: i64,
}

impl RecoveryRequest {
    /// Number of events missing in this gap.
    pub fn gap_size(&self) -> u64 {
        self.to_sequence - self.from_sequence + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn sample_order_accepted(seq: u64) -> MarketEvent {
        make_event(
            seq,
            MarketEventPayload::OrderAccepted {
                order_id: OrderId::new(),
                account_id: AccountId::new(),
                symbol: MarketId::new("BTC/USDT"),
                side: Side::BUY,
                price: Price::from_u64(50000),
                quantity: Quantity::from_str("1.0").unwrap(),
            },
        )
    }

    #[test]
    fn test_event_ordering_by_sequence() {
        let e1 = sample_order_accepted(1);
        let e2 = sample_order_accepted(2);
        let e3 = sample_order_accepted(3);

        let mut events = vec![e3.clone(), e1.clone(), e2.clone()];
        events.sort();

        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[2].sequence, 3);
    }

    #[test]
    fn test_event_type_label() {
        let e = sample_order_accepted(1);
        assert_eq!(e.event_type_label(), "OrderAccepted");
    }

    #[test]
    fn test_event_symbol_extraction() {
        let e = sample_order_accepted(1);
        assert_eq!(e.symbol().unwrap().as_str(), "BTC/USDT");
    }

    #[test]
    fn test_event_serialization_roundtrip() {
        let e = sample_order_accepted(42);
        let json = serde_json::to_string(&e).unwrap();
        let deserialized: MarketEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, deserialized);
    }

    #[test]
    fn test_recovery_request_gap_size() {
        let req = RecoveryRequest {
            from_sequence: 10,
            to_sequence: 15,
            detected_at: 1708123456789000000,
        };
        assert_eq!(req.gap_size(), 6);
    }
}
