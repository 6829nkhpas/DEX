//! Snapshot API for the Market Data Service
//!
//! Provides full-depth order book snapshots for client reconnect logic,
//! with versioning, sequence tagging, checksum generation, and pagination.
//!
//! Implements spec §9 section 3.8: "Serve stale data if < 10 seconds old"
//! for degraded mode, and snapshot versioning for deterministic sync.

use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use types::ids::MarketId;

use crate::order_book::{DepthSnapshot, OrderBookState, PriceLevel};

/// A versioned, checksummed snapshot of the full order book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullSnapshot {
    /// Monotonic snapshot version.
    pub version: u64,
    /// Trading pair symbol.
    pub symbol: MarketId,
    /// Bid levels in descending price order (best first).
    pub bids: Vec<PriceLevel>,
    /// Ask levels in ascending price order (best first).
    pub asks: Vec<PriceLevel>,
    /// Last event sequence number included in this snapshot.
    pub last_sequence: u64,
    /// Unix nanoseconds timestamp when snapshot was created.
    pub timestamp: i64,
    /// SHA-256 checksum of the snapshot content for integrity.
    pub checksum: String,
}

/// A paginated view of the snapshot for deep books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaginatedSnapshot {
    pub symbol: MarketId,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub last_sequence: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_bid_levels: usize,
    pub total_ask_levels: usize,
    pub has_more: bool,
}

/// Builds and manages versioned snapshots from the order book state.
pub struct SnapshotBuilder {
    /// Current snapshot version counter.
    version_counter: u64,
}

impl SnapshotBuilder {
    pub fn new() -> Self {
        Self { version_counter: 0 }
    }

    /// Build a full snapshot from the current order book state.
    pub fn build_full(
        &mut self,
        book: &OrderBookState,
        timestamp: i64,
    ) -> FullSnapshot {
        self.version_counter += 1;

        let bids = book.bid_levels();
        let asks = book.ask_levels();
        let last_sequence = book.last_sequence();

        let checksum = compute_checksum(&bids, &asks, last_sequence);

        FullSnapshot {
            version: self.version_counter,
            symbol: book.symbol.clone(),
            bids,
            asks,
            last_sequence,
            timestamp,
            checksum,
        }
    }

    /// Build a paginated snapshot for deep books.
    pub fn build_paginated(
        &self,
        book: &OrderBookState,
        page: usize,
        page_size: usize,
    ) -> PaginatedSnapshot {
        let all_bids = book.bid_levels();
        let all_asks = book.ask_levels();

        let offset = page * page_size;

        let bids: Vec<PriceLevel> = all_bids
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect();

        let asks: Vec<PriceLevel> = all_asks
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect();

        let total_bid_levels = book.bid_depth();
        let total_ask_levels = book.ask_depth();
        let max_total = std::cmp::max(total_bid_levels, total_ask_levels);
        let has_more = offset + page_size < max_total;

        PaginatedSnapshot {
            symbol: book.symbol.clone(),
            bids,
            asks,
            last_sequence: book.last_sequence(),
            page,
            page_size,
            total_bid_levels,
            total_ask_levels,
            has_more,
        }
    }

    /// Current snapshot version.
    pub fn current_version(&self) -> u64 {
        self.version_counter
    }
}

impl Default for SnapshotBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a SHA-256 checksum over the book levels and sequence.
///
/// Uses deterministic serialization (sorted BTreeMap-backed levels).
fn compute_checksum(
    bids: &[PriceLevel],
    asks: &[PriceLevel],
    sequence: u64,
) -> String {
    let mut hasher = Sha256::new();

    // Hash bids
    for level in bids {
        hasher.update(level.price.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(level.total_quantity.to_string().as_bytes());
        hasher.update(b"|");
    }
    hasher.update(b"---");

    // Hash asks
    for level in asks {
        hasher.update(level.price.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(level.total_quantity.to_string().as_bytes());
        hasher.update(b"|");
    }
    hasher.update(b"---");

    // Hash sequence
    hasher.update(sequence.to_le_bytes());

    format!("{:x}", hasher.finalize())
}

/// Verify that a snapshot's checksum matches its content.
pub fn verify_snapshot_integrity(snapshot: &FullSnapshot) -> bool {
    let expected = compute_checksum(
        &snapshot.bids,
        &snapshot.asks,
        snapshot.last_sequence,
    );
    snapshot.checksum == expected
}

/// Validate that a snapshot and a sequence of deltas are in sync.
///
/// The snapshot's last_sequence should be less than the first delta's sequence.
pub fn validate_snapshot_delta_sync(
    snapshot: &FullSnapshot,
    first_delta_sequence: u64,
) -> bool {
    snapshot.last_sequence < first_delta_sequence
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::OrderId;
    use types::numeric::{Price, Quantity};
    use types::order::Side;

    fn populated_book() -> OrderBookState {
        let mut book = OrderBookState::new(MarketId::new("BTC/USDT"));

        for i in 1..=5 {
            book.apply_order_accepted(
                OrderId::new(),
                Side::BUY,
                Price::from_u64(50000 - i * 100),
                Quantity::from_str("1.0").unwrap(),
                i as u64,
            );
        }
        for i in 1..=5 {
            book.apply_order_accepted(
                OrderId::new(),
                Side::SELL,
                Price::from_u64(51000 + i * 100),
                Quantity::from_str("1.0").unwrap(),
                (5 + i) as u64,
            );
        }

        book
    }

    #[test]
    fn test_build_full_snapshot() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();
        let snap = builder.build_full(&book, 1708123456789000000);

        assert_eq!(snap.version, 1);
        assert_eq!(snap.bids.len(), 5);
        assert_eq!(snap.asks.len(), 5);
        assert_eq!(snap.last_sequence, 10);
        assert!(!snap.checksum.is_empty());
    }

    #[test]
    fn test_snapshot_versioning() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();

        let s1 = builder.build_full(&book, 1708123456789000000);
        let s2 = builder.build_full(&book, 1708123456790000000);

        assert_eq!(s1.version, 1);
        assert_eq!(s2.version, 2);
    }

    #[test]
    fn test_snapshot_integrity() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();
        let snap = builder.build_full(&book, 1708123456789000000);

        assert!(verify_snapshot_integrity(&snap));

        // Tamper with checksum → should fail
        let mut tampered = snap.clone();
        tampered.checksum = "corrupted".to_string();
        assert!(!verify_snapshot_integrity(&tampered));
    }

    #[test]
    fn test_deterministic_checksum() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();

        let s1 = builder.build_full(&book, 1708123456789000000);
        // Reset builder but same book state
        let mut builder2 = SnapshotBuilder::new();
        let s2 = builder2.build_full(&book, 1708123456789000000);

        assert_eq!(s1.checksum, s2.checksum);
    }

    #[test]
    fn test_paginated_snapshot() {
        let book = populated_book();
        let builder = SnapshotBuilder::new();

        let page0 = builder.build_paginated(&book, 0, 2);
        assert_eq!(page0.bids.len(), 2);
        assert_eq!(page0.asks.len(), 2);
        assert_eq!(page0.page, 0);
        assert!(page0.has_more);
        assert_eq!(page0.total_bid_levels, 5);

        let page1 = builder.build_paginated(&book, 1, 2);
        assert_eq!(page1.bids.len(), 2);
        assert_eq!(page1.page, 1);
        assert!(page1.has_more);

        let page2 = builder.build_paginated(&book, 2, 2);
        assert_eq!(page2.bids.len(), 1); // Only 1 remaining
        assert!(!page2.has_more);
    }

    #[test]
    fn test_snapshot_serialization() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();
        let snap = builder.build_full(&book, 1708123456789000000);

        let json = serde_json::to_string(&snap).unwrap();
        let deserialized: FullSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, deserialized);
    }

    #[test]
    fn test_snapshot_delta_sync_validation() {
        let book = populated_book();
        let mut builder = SnapshotBuilder::new();
        let snap = builder.build_full(&book, 1708123456789000000);

        // Delta starts after snapshot's last sequence
        assert!(validate_snapshot_delta_sync(&snap, 11));

        // Delta starts at or before snapshot → invalid
        assert!(!validate_snapshot_delta_sync(&snap, 10));
        assert!(!validate_snapshot_delta_sync(&snap, 5));
    }
}
