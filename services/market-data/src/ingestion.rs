//! Event ingestion layer for the Market Data Service
//!
//! Validates incoming events, enforces monotonic sequencing, detects
//! duplicates and gaps, and buffers events for downstream processing.
//!
//! Implements spec §14 (Sequence Numbering) invariants:
//! - No gaps in sequence numbers
//! - No duplicate sequences
//! - Strictly increasing sequence order

use std::collections::VecDeque;

use tracing::{debug, error, info, warn};

use crate::events::{MarketEvent, RecoveryRequest};

/// Errors that can occur during event ingestion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IngestionError {
    #[error("duplicate event: sequence {0} already processed")]
    DuplicateEvent(u64),

    #[error("sequence gap detected: expected {expected}, got {actual}")]
    SequenceGap { expected: u64, actual: u64 },

    #[error("non-monotonic sequence: last={last}, received={received}")]
    NonMonotonic { last: u64, received: u64 },

    #[error("buffer overflow: capacity {capacity}, current size {size}")]
    BufferOverflow { capacity: usize, size: usize },
}

/// Result of ingesting a single event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestionResult {
    /// Event accepted and buffered for processing.
    Accepted,
    /// Duplicate event was dropped.
    Dropped,
    /// Gap detected; event buffered but recovery needed.
    GapDetected(RecoveryRequest),
}

/// Configuration for the event ingester.
#[derive(Debug, Clone)]
pub struct IngesterConfig {
    /// Maximum number of events in the input buffer.
    pub buffer_capacity: usize,
    /// Maximum number of recent sequence IDs to track for dedup.
    pub dedup_window: usize,
}

impl Default for IngesterConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: 100_000,
            dedup_window: 10_000,
        }
    }
}

/// Event ingestion layer that validates, deduplicates, and buffers events.
///
/// Maintains monotonic sequence tracking and emits recovery requests
/// when gaps are detected (spec §14.6.1).
pub struct EventIngester {
    /// Last successfully processed sequence number.
    last_sequence: Option<u64>,
    /// Circular buffer of recently seen sequence IDs for dedup.
    seen_sequences: VecDeque<u64>,
    /// Input buffer queue for downstream consumers.
    buffer: VecDeque<MarketEvent>,
    /// Configuration.
    config: IngesterConfig,
    /// Total events ingested (accepted).
    events_accepted: u64,
    /// Total duplicate events dropped.
    events_dropped: u64,
    /// Total gaps detected.
    gaps_detected: u64,
}

impl EventIngester {
    /// Create a new ingester with the given configuration.
    pub fn new(config: IngesterConfig) -> Self {
        info!(
            buffer_capacity = config.buffer_capacity,
            dedup_window = config.dedup_window,
            "EventIngester initialized"
        );

        Self {
            last_sequence: None,
            seen_sequences: VecDeque::with_capacity(config.dedup_window),
            buffer: VecDeque::with_capacity(config.buffer_capacity),
            config,
            events_accepted: 0,
            events_dropped: 0,
            gaps_detected: 0,
        }
    }

    /// Create a new ingester with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(IngesterConfig::default())
    }

    /// Ingest a single event.
    ///
    /// Validates monotonic sequencing, drops duplicates, detects gaps,
    /// and queues the event in the input buffer.
    pub fn ingest(&mut self, event: MarketEvent) -> Result<IngestionResult, IngestionError> {
        let seq = event.sequence;

        // Check for duplicate
        if self.is_duplicate(seq) {
            self.events_dropped += 1;
            debug!(
                sequence = seq,
                event_type = event.event_type_label(),
                "Dropping duplicate event"
            );
            return Ok(IngestionResult::Dropped);
        }

        // Check buffer overflow guard
        if self.buffer.len() >= self.config.buffer_capacity {
            error!(
                capacity = self.config.buffer_capacity,
                size = self.buffer.len(),
                sequence = seq,
                "Buffer overflow — rejecting event"
            );
            return Err(IngestionError::BufferOverflow {
                capacity: self.config.buffer_capacity,
                size: self.buffer.len(),
            });
        }

        // Check for non-monotonic sequence (received < last)
        if let Some(last) = self.last_sequence {
            if seq <= last {
                warn!(
                    last_sequence = last,
                    received_sequence = seq,
                    "Non-monotonic sequence detected"
                );
                return Err(IngestionError::NonMonotonic {
                    last,
                    received: seq,
                });
            }
        }

        // Detect gap
        let gap_request = self.detect_gap(seq);

        // Accept the event
        self.record_sequence(seq);
        self.last_sequence = Some(seq);
        self.events_accepted += 1;

        debug!(
            sequence = seq,
            event_type = event.event_type_label(),
            buffer_size = self.buffer.len() + 1,
            "Event accepted"
        );

        self.buffer.push_back(event);

        match gap_request {
            Some(req) => {
                self.gaps_detected += 1;
                warn!(
                    from = req.from_sequence,
                    to = req.to_sequence,
                    gap_size = req.gap_size(),
                    "Sequence gap detected — recovery needed"
                );
                Ok(IngestionResult::GapDetected(req))
            }
            None => Ok(IngestionResult::Accepted),
        }
    }

    /// Drain all buffered events in sequence order.
    ///
    /// Returns events sorted by sequence number for deterministic
    /// downstream processing (spec §12).
    pub fn drain_buffer(&mut self) -> Vec<MarketEvent> {
        let mut events: Vec<MarketEvent> = self.buffer.drain(..).collect();
        events.sort(); // Ord is by sequence
        events
    }

    /// Peek at the next event without removing it.
    pub fn peek(&self) -> Option<&MarketEvent> {
        self.buffer.front()
    }

    /// Number of events currently in the buffer.
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the buffer is empty.
    pub fn is_buffer_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Last processed sequence number, if any.
    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }

    /// Total events accepted since creation.
    pub fn events_accepted(&self) -> u64 {
        self.events_accepted
    }

    /// Total events dropped as duplicates since creation.
    pub fn events_dropped(&self) -> u64 {
        self.events_dropped
    }

    /// Total gaps detected since creation.
    pub fn gaps_detected(&self) -> u64 {
        self.gaps_detected
    }

    /// Check if a sequence number has been recently seen.
    fn is_duplicate(&self, seq: u64) -> bool {
        self.seen_sequences.contains(&seq)
    }

    /// Record a sequence number in the dedup window.
    fn record_sequence(&mut self, seq: u64) {
        if self.seen_sequences.len() >= self.config.dedup_window {
            self.seen_sequences.pop_front();
        }
        self.seen_sequences.push_back(seq);
    }

    /// Detect if there is a gap between last_sequence and the incoming seq.
    ///
    /// Returns a RecoveryRequest if a gap is found.
    fn detect_gap(&self, incoming_seq: u64) -> Option<RecoveryRequest> {
        if let Some(last) = self.last_sequence {
            let expected = last + 1;
            if incoming_seq > expected {
                return Some(RecoveryRequest {
                    from_sequence: expected,
                    to_sequence: incoming_seq - 1,
                    detected_at: 0, // Caller should set from exchange clock
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{MarketEventPayload, RecoveryRequest};
    use types::ids::{AccountId, MarketId, OrderId};
    use types::numeric::{Price, Quantity};
    use types::order::Side;
    use uuid::Uuid;

    fn make_event(seq: u64) -> MarketEvent {
        MarketEvent {
            event_id: Uuid::now_v7(),
            sequence: seq,
            timestamp: 1708123456789000000 + (seq as i64 * 1000),
            source: "matching-engine".to_string(),
            payload: MarketEventPayload::OrderAccepted {
                order_id: OrderId::new(),
                account_id: AccountId::new(),
                symbol: MarketId::new("BTC/USDT"),
                side: Side::BUY,
                price: Price::from_u64(50000),
                quantity: Quantity::from_str("1.0").unwrap(),
            },
            schema_version: "1.0.0".to_string(),
            correlation_id: Uuid::now_v7(),
        }
    }

    #[test]
    fn test_sequential_ingestion() {
        let mut ingester = EventIngester::with_defaults();

        for seq in 1..=10 {
            let result = ingester.ingest(make_event(seq)).unwrap();
            assert_eq!(result, IngestionResult::Accepted);
        }

        assert_eq!(ingester.last_sequence(), Some(10));
        assert_eq!(ingester.events_accepted(), 10);
        assert_eq!(ingester.buffer_len(), 10);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut ingester = EventIngester::with_defaults();

        let result = ingester.ingest(make_event(1)).unwrap();
        assert_eq!(result, IngestionResult::Accepted);

        // Same sequence again
        let e2 = make_event(1);
        let result = ingester.ingest(e2).unwrap();
        assert_eq!(result, IngestionResult::Dropped);
        assert_eq!(ingester.events_dropped(), 1);
        assert_eq!(ingester.events_accepted(), 1);
    }

    #[test]
    fn test_gap_detection() {
        let mut ingester = EventIngester::with_defaults();

        ingester.ingest(make_event(1)).unwrap();

        // Skip sequences 2, 3, 4
        let result = ingester.ingest(make_event(5)).unwrap();
        assert_eq!(
            result,
            IngestionResult::GapDetected(RecoveryRequest {
                from_sequence: 2,
                to_sequence: 4,
                detected_at: 0,
            })
        );
        assert_eq!(ingester.gaps_detected(), 1);
    }

    #[test]
    fn test_non_monotonic_rejected() {
        let mut ingester = EventIngester::with_defaults();

        ingester.ingest(make_event(5)).unwrap();

        let result = ingester.ingest(make_event(3));
        assert!(result.is_err());
        match result.unwrap_err() {
            IngestionError::NonMonotonic { last, received } => {
                assert_eq!(last, 5);
                assert_eq!(received, 3);
            }
            err => panic!("Expected NonMonotonic, got {:?}", err),
        }
    }

    #[test]
    fn test_buffer_overflow() {
        let config = IngesterConfig {
            buffer_capacity: 3,
            dedup_window: 100,
        };
        let mut ingester = EventIngester::new(config);

        ingester.ingest(make_event(1)).unwrap();
        ingester.ingest(make_event(2)).unwrap();
        ingester.ingest(make_event(3)).unwrap();

        let result = ingester.ingest(make_event(4));
        assert!(result.is_err());
        match result.unwrap_err() {
            IngestionError::BufferOverflow { capacity, size } => {
                assert_eq!(capacity, 3);
                assert_eq!(size, 3);
            }
            err => panic!("Expected BufferOverflow, got {:?}", err),
        }
    }

    #[test]
    fn test_drain_returns_sorted() {
        let mut ingester = EventIngester::with_defaults();

        // Insert with gaps (gap detection fires but events still buffered)
        ingester.ingest(make_event(1)).unwrap();
        ingester.ingest(make_event(5)).unwrap();
        ingester.ingest(make_event(10)).unwrap();

        let events = ingester.drain_buffer();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 5);
        assert_eq!(events[2].sequence, 10);
        assert!(ingester.is_buffer_empty());
    }

    #[test]
    fn test_dedup_window_eviction() {
        let config = IngesterConfig {
            buffer_capacity: 100_000,
            dedup_window: 3,
        };
        let mut ingester = EventIngester::new(config);

        ingester.ingest(make_event(1)).unwrap();
        ingester.ingest(make_event(2)).unwrap();
        ingester.ingest(make_event(3)).unwrap();
        // Dedup window full; seq 1 should be evicted on next insert
        ingester.ingest(make_event(4)).unwrap();

        // Seq 1 is no longer in dedup window, but it would be
        // rejected as non-monotonic anyway (1 < 4)
        let result = ingester.ingest(make_event(1));
        assert!(result.is_err()); // NonMonotonic
    }

    #[test]
    fn test_first_event_no_gap() {
        let mut ingester = EventIngester::with_defaults();

        // First event can be any sequence (no predecessor to compare)
        let result = ingester.ingest(make_event(100)).unwrap();
        assert_eq!(result, IngestionResult::Accepted);
        assert_eq!(ingester.last_sequence(), Some(100));
    }

    #[test]
    fn test_peek() {
        let mut ingester = EventIngester::with_defaults();
        assert!(ingester.peek().is_none());

        ingester.ingest(make_event(1)).unwrap();
        assert_eq!(ingester.peek().unwrap().sequence, 1);
    }

    #[test]
    fn test_ingester_stats() {
        let mut ingester = EventIngester::with_defaults();

        ingester.ingest(make_event(1)).unwrap();
        ingester.ingest(make_event(2)).unwrap();
        // duplicate
        let _ = ingester.ingest(make_event(2));
        // gap (skip 3,4)
        ingester.ingest(make_event(5)).unwrap();

        assert_eq!(ingester.events_accepted(), 3);
        assert_eq!(ingester.events_dropped(), 1);
        assert_eq!(ingester.gaps_detected(), 1);
    }
}
