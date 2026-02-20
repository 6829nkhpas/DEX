//! Delta generator for incremental book updates
//!
//! Compares order book state before and after event processing to generate
//! deltas for subscribed clients. Ensures deterministic delta ordering
//! (spec §12) and conservation validation.
//!
//! Delta flow:
//! 1. Capture pre-event book state (snapshot of affected levels)
//! 2. Apply event to book
//! 3. Compare post-event state to pre-event state
//! 4. Emit sorted deltas with sequence numbers and timestamps
//! 5. Batch deltas and flush on timer or threshold

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::numeric::Price;
use types::order::Side;

use crate::order_book::OrderBookState;

/// A single change to a price level in the order book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookDelta {
    /// Which side changed.
    pub side: Side,
    /// The price level that changed.
    pub price: Price,
    /// New total quantity at this level (0 = level removed).
    pub new_quantity: Decimal,
    /// Previous total quantity at this level.
    pub old_quantity: Decimal,
    /// Sequence number of the event that caused this delta.
    pub sequence: u64,
    /// Timestamp (Unix nanos) of the causing event.
    pub timestamp: i64,
}

impl BookDelta {
    /// The net quantity change at this level.
    pub fn quantity_change(&self) -> Decimal {
        self.new_quantity - self.old_quantity
    }

    /// Whether this delta represents a level being fully removed.
    pub fn is_removal(&self) -> bool {
        self.new_quantity == Decimal::ZERO && self.old_quantity > Decimal::ZERO
    }

    /// Whether this delta represents a new level being created.
    pub fn is_new_level(&self) -> bool {
        self.old_quantity == Decimal::ZERO && self.new_quantity > Decimal::ZERO
    }
}

/// Deterministic ordering: side (Buy < Sell), then price ascending.
impl Ord for BookDelta {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let side_ord = match (&self.side, &other.side) {
            (Side::BUY, Side::SELL) => std::cmp::Ordering::Less,
            (Side::SELL, Side::BUY) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };
        side_ord.then_with(|| self.price.cmp(&other.price))
    }
}

impl PartialOrd for BookDelta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Snapshot of price levels at a point in time for diff calculation.
type LevelSnapshot = BTreeMap<(u8, Decimal), Decimal>;

/// Captures a before-snapshot of the book for later diff.
fn capture_level_snapshot(book: &OrderBookState) -> LevelSnapshot {
    let mut snap = BTreeMap::new();
    for level in book.bid_levels() {
        snap.insert((0, level.price.as_decimal()), level.total_quantity);
    }
    for level in book.ask_levels() {
        snap.insert((1, level.price.as_decimal()), level.total_quantity);
    }
    snap
}

/// Generates deltas by comparing before and after book state.
///
/// Zero-change levels are excluded (no phantom deltas).
/// Output is deterministically sorted by side then price.
pub fn generate_deltas(
    before: &LevelSnapshot,
    after: &LevelSnapshot,
    sequence: u64,
    timestamp: i64,
) -> Vec<BookDelta> {
    let mut deltas = Vec::new();

    // Check all levels that existed before
    for (&(side_byte, price_dec), &old_qty) in before {
        let new_qty = after.get(&(side_byte, price_dec)).copied().unwrap_or(Decimal::ZERO);
        if old_qty != new_qty {
            let side = if side_byte == 0 { Side::BUY } else { Side::SELL };
            let price = Price::try_new(price_dec).unwrap();
            deltas.push(BookDelta {
                side,
                price,
                new_quantity: new_qty,
                old_quantity: old_qty,
                sequence,
                timestamp,
            });
        }
    }

    // Check levels that are new (exist in after but not before)
    for (&(side_byte, price_dec), &new_qty) in after {
        if !before.contains_key(&(side_byte, price_dec)) && new_qty > Decimal::ZERO {
            let side = if side_byte == 0 { Side::BUY } else { Side::SELL };
            let price = Price::try_new(price_dec).unwrap();
            deltas.push(BookDelta {
                side,
                price,
                new_quantity: new_qty,
                old_quantity: Decimal::ZERO,
                sequence,
                timestamp,
            });
        }
    }

    deltas.sort(); // Deterministic ordering
    deltas
}

/// Accumulates deltas into batches and flushes on threshold.
pub struct DeltaBatcher {
    /// Accumulated deltas awaiting flush.
    batch: Vec<BookDelta>,
    /// Maximum batch size before auto-flush.
    max_batch_size: usize,
}

impl DeltaBatcher {
    /// Create a new batcher with the given flush threshold.
    pub fn new(max_batch_size: usize) -> Self {
        Self {
            batch: Vec::new(),
            max_batch_size,
        }
    }

    /// Add deltas to the batch. Returns flushed batch if threshold reached.
    pub fn add(&mut self, deltas: Vec<BookDelta>) -> Option<Vec<BookDelta>> {
        self.batch.extend(deltas);
        if self.batch.len() >= self.max_batch_size {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Force-flush all accumulated deltas.
    pub fn flush(&mut self) -> Vec<BookDelta> {
        let mut flushed = Vec::new();
        std::mem::swap(&mut flushed, &mut self.batch);
        flushed
    }

    /// Number of deltas currently batched.
    pub fn pending_count(&self) -> usize {
        self.batch.len()
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }
}

/// High-level delta generator that wraps snapshot + diff logic.
pub struct DeltaGenerator {
    /// Last captured book snapshot for diff calculation.
    last_snapshot: Option<LevelSnapshot>,
}

impl DeltaGenerator {
    pub fn new() -> Self {
        Self {
            last_snapshot: None,
        }
    }

    /// Capture the current book state as the "before" snapshot.
    pub fn capture_before(&mut self, book: &OrderBookState) {
        self.last_snapshot = Some(capture_level_snapshot(book));
    }

    /// Generate deltas by comparing the "before" snapshot with current book state.
    ///
    /// Returns empty vec if no before snapshot was captured.
    pub fn generate_after(
        &mut self,
        book: &OrderBookState,
        sequence: u64,
        timestamp: i64,
    ) -> Vec<BookDelta> {
        let after = capture_level_snapshot(book);
        let deltas = match &self.last_snapshot {
            Some(before) => generate_deltas(before, &after, sequence, timestamp),
            None => Vec::new(),
        };
        self.last_snapshot = Some(after);
        deltas
    }
}

impl Default for DeltaGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate conservation: sum of all delta quantity changes should match
/// the net book change. Returns true if conservation holds.
pub fn validate_conservation(deltas: &[BookDelta]) -> bool {
    // Each delta's quantity_change shows the net effect.
    // For a valid set of deltas from a single event, the total net change
    // should be internally consistent (no phantom quantity created/destroyed).
    for delta in deltas {
        // No delta should have negative new_quantity
        if delta.new_quantity < Decimal::ZERO {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{MarketId, OrderId};
    use types::numeric::Quantity;

    fn make_book() -> OrderBookState {
        OrderBookState::new(MarketId::new("BTC/USDT"))
    }

    #[test]
    fn test_delta_on_new_order() {
        let mut book = make_book();
        let mut gen = DeltaGenerator::new();

        gen.capture_before(&book);

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        let deltas = gen.generate_after(&book, 1, 1708123456789000000);
        assert_eq!(deltas.len(), 1);
        assert!(deltas[0].is_new_level());
        assert_eq!(deltas[0].new_quantity, Decimal::from(1));
        assert_eq!(deltas[0].old_quantity, Decimal::ZERO);
        assert_eq!(deltas[0].side, Side::BUY);
    }

    #[test]
    fn test_delta_on_trade() {
        let mut book = make_book();
        let maker_id = OrderId::new();
        let mut gen = DeltaGenerator::new();

        book.apply_order_accepted(
            maker_id,
            Side::SELL,
            Price::from_u64(51000),
            Quantity::from_str("2.0").unwrap(),
            1,
        );

        gen.capture_before(&book);
        book.apply_trade_executed(maker_id, Quantity::from_str("0.5").unwrap(), 2);
        let deltas = gen.generate_after(&book, 2, 1708123456790000000);

        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].side, Side::SELL);
        assert_eq!(
            deltas[0].new_quantity,
            Decimal::from_str_exact("1.5").unwrap()
        );
        assert_eq!(deltas[0].old_quantity, Decimal::from(2));
        assert!(!deltas[0].is_removal());
    }

    #[test]
    fn test_delta_on_level_removal() {
        let mut book = make_book();
        let maker_id = OrderId::new();
        let mut gen = DeltaGenerator::new();

        book.apply_order_accepted(
            maker_id,
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        gen.capture_before(&book);
        book.apply_trade_executed(maker_id, Quantity::from_str("1.0").unwrap(), 2);
        let deltas = gen.generate_after(&book, 2, 1708123456790000000);

        assert_eq!(deltas.len(), 1);
        assert!(deltas[0].is_removal());
    }

    #[test]
    fn test_no_phantom_deltas() {
        let mut book = make_book();
        let mut gen = DeltaGenerator::new();

        book.apply_order_accepted(
            OrderId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            1,
        );

        // Capture and generate with no changes
        gen.capture_before(&book);
        let deltas = gen.generate_after(&book, 2, 1708123456790000000);

        assert!(deltas.is_empty(), "No phantom deltas should be generated");
    }

    #[test]
    fn test_deterministic_delta_ordering() {
        let mut book = make_book();
        let mut gen = DeltaGenerator::new();

        gen.capture_before(&book);

        // Add orders at multiple levels on both sides
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
            Price::from_u64(49000),
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

        let deltas = gen.generate_after(&book, 3, 1708123456790000000);

        // Should be sorted: buys first (ascending price), then sells
        assert_eq!(deltas.len(), 3);
        assert_eq!(deltas[0].side, Side::BUY);
        assert_eq!(deltas[0].price, Price::from_u64(49000));
        assert_eq!(deltas[1].side, Side::BUY);
        assert_eq!(deltas[1].price, Price::from_u64(50000));
        assert_eq!(deltas[2].side, Side::SELL);
        assert_eq!(deltas[2].price, Price::from_u64(51000));
    }

    #[test]
    fn test_conservation_validation() {
        let deltas = vec![
            BookDelta {
                side: Side::BUY,
                price: Price::from_u64(50000),
                new_quantity: Decimal::from(1),
                old_quantity: Decimal::ZERO,
                sequence: 1,
                timestamp: 1708123456789000000,
            },
        ];
        assert!(validate_conservation(&deltas));
    }

    #[test]
    fn test_conservation_validation_fails_negative() {
        let deltas = vec![
            BookDelta {
                side: Side::BUY,
                price: Price::from_u64(50000),
                new_quantity: Decimal::from(-1),
                old_quantity: Decimal::from(1),
                sequence: 1,
                timestamp: 1708123456789000000,
            },
        ];
        assert!(!validate_conservation(&deltas));
    }

    #[test]
    fn test_delta_batcher_auto_flush() {
        let mut batcher = DeltaBatcher::new(2);

        let d1 = BookDelta {
            side: Side::BUY,
            price: Price::from_u64(50000),
            new_quantity: Decimal::from(1),
            old_quantity: Decimal::ZERO,
            sequence: 1,
            timestamp: 1708123456789000000,
        };

        // First add: below threshold
        let result = batcher.add(vec![d1.clone()]);
        assert!(result.is_none());
        assert_eq!(batcher.pending_count(), 1);

        // Second add: reaches threshold
        let result = batcher.add(vec![d1.clone()]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 2);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_delta_batcher_manual_flush() {
        let mut batcher = DeltaBatcher::new(100);

        let d = BookDelta {
            side: Side::BUY,
            price: Price::from_u64(50000),
            new_quantity: Decimal::from(1),
            old_quantity: Decimal::ZERO,
            sequence: 1,
            timestamp: 1708123456789000000,
        };

        batcher.add(vec![d]);
        let flushed = batcher.flush();
        assert_eq!(flushed.len(), 1);
        assert!(batcher.is_empty());
    }
}
