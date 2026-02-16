//! Ask (sell-side) order book
//!
//! Maintains sell orders sorted by price ascending (best ask first).
//! Uses BTreeMap for deterministic iteration order per spec §12 (Determinism Rules).

use std::collections::BTreeMap;
use types::ids::OrderId;
use types::numeric::{Price, Quantity};
use types::order::Order;

use super::price_level::PriceLevel;

/// Ask (sell) side order book
///
/// Orders are sorted by price ascending, so the lowest ask is first.
/// At each price level, orders are maintained in FIFO order.
#[derive(Debug, Clone)]
pub struct AskBook {
    /// Price levels sorted ascending (lowest price first)
    /// Using BTreeMap ensures deterministic iteration
    levels: BTreeMap<Price, PriceLevel>,
}

impl AskBook {
    /// Create a new empty ask book
    pub fn new() -> Self {
        Self {
            levels: BTreeMap::new(),
        }
    }

    /// Insert an order into the ask book
    pub fn insert(&mut self, order: &Order) {
        let level = self.levels.entry(order.price).or_insert_with(PriceLevel::new);
        level.insert(order.order_id, order.account_id, order.remaining_quantity);
    }

    /// Remove an order from the ask book
    ///
    /// Returns true if the order was found and removed
    pub fn remove(&mut self, order_id: &OrderId, price: Price) -> bool {
        if let Some(level) = self.levels.get_mut(&price) {
            if level.remove(order_id).is_some() {
                // Remove empty price levels to keep book clean
                if level.is_empty() {
                    self.levels.remove(&price);
                }
                return true;
            }
        }
        false
    }

    /// Get the best ask (lowest price)
    pub fn best_ask(&self) -> Option<(Price, Quantity)> {
        // BTreeMap iter is ascending, so first() gives us lowest price
        self.levels.iter().next().map(|(price, level)| {
            (*price, level.total_quantity())
        })
    }

    /// Get the best ask price
    pub fn best_ask_price(&self) -> Option<Price> {
        self.levels.keys().next().copied()
    }

    /// Get mutable reference to the best ask level
    pub(crate) fn best_ask_level_mut(&mut self) -> Option<(Price, &mut PriceLevel)> {
        self.levels.iter_mut().next().map(|(price, level)| (*price, level))
    }

    /// Get depth snapshot (top N price levels)
    pub fn depth_snapshot(&self, depth: usize) -> Vec<(Price, Quantity)> {
        self.levels
            .iter()
            .take(depth)
            .map(|(price, level)| (*price, level.total_quantity()))
            .collect()
    }

    /// Check if the ask book is empty
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Get the total number of price levels
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }
}

impl Default for AskBook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{AccountId, MarketId};
    use types::order::{Side, TimeInForce};

    fn create_test_order(price_val: u64, qty_str: &str) -> Order {
        Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::SELL,
            Price::from_u64(price_val),
            Quantity::from_str(qty_str).unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        )
    }

    #[test]
    fn test_ask_book_insert() {
        let mut book = AskBook::new();
        let order = create_test_order(50000, "1.0");

        book.insert(&order);

        assert_eq!(book.level_count(), 1);
        assert!(!book.is_empty());
    }

    #[test]
    fn test_ask_book_best_ask() {
        let mut book = AskBook::new();
        
        let order1 = create_test_order(50000, "1.0");
        let order2 = create_test_order(51000, "2.0"); // Higher price
        let order3 = create_test_order(49000, "1.5"); // Lower price (best ask)

        book.insert(&order1);
        book.insert(&order2);
        book.insert(&order3);

        let (best_price, best_qty) = book.best_ask().unwrap();
        assert_eq!(best_price, Price::from_u64(49000)); // Lowest price
        assert_eq!(best_qty, Quantity::from_str("1.5").unwrap());
    }

    #[test]
    fn test_ask_book_remove() {
        let mut book = AskBook::new();
        let order = create_test_order(50000, "1.0");
        let order_id = order.order_id;
        let price = order.price;

        book.insert(&order);
        assert_eq!(book.level_count(), 1);

        let removed = book.remove(&order_id, price);
        assert!(removed);
        assert!(book.is_empty());
    }

    #[test]
    fn test_ask_book_depth_snapshot() {
        let mut book = AskBook::new();
        
        book.insert(&create_test_order(50000, "1.0"));
        book.insert(&create_test_order(51000, "2.0"));
        book.insert(&create_test_order(49000, "1.5"));
        book.insert(&create_test_order(52000, "0.5"));

        let depth = book.depth_snapshot(2);
        
        // Should return top 2 levels (lowest prices first)
        assert_eq!(depth.len(), 2);
        assert_eq!(depth[0].0, Price::from_u64(49000));
        assert_eq!(depth[1].0, Price::from_u64(50000));
    }

    #[test]
    fn test_ask_book_price_time_priority() {
        let mut book = AskBook::new();
        
        let order1 = create_test_order(50000, "1.0");
        let order2 = create_test_order(50000, "2.0"); // Same price
        
        book.insert(&order1);
        book.insert(&order2);

        // Both orders at same price level
        assert_eq!(book.level_count(), 1);
        
        let (price, total_qty) = book.best_ask().unwrap();
        assert_eq!(price, Price::from_u64(50000));
        assert_eq!(total_qty, Quantity::from_str("3.0").unwrap()); // 1.0 + 2.0
    }
}
