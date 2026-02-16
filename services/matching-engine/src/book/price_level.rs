//! Price level implementation with FIFO queue
//!
//! A price level contains all orders at a specific price point.
//! Orders are maintained in FIFO (First-In-First-Out) order to enforce
//! time priority per spec §3.11 (Matching Algorithm - Price-Time Priority).

use std::collections::VecDeque;
use types::ids::{AccountId, OrderId};
use types::numeric::Quantity;

/// A price level containing orders at a specific price
///
/// Maintains strict FIFO ordering for time-priority matching.
/// Orders are stored as OrderId references with their quantities.
#[derive(Debug, Clone)]
pub struct PriceLevel {
    /// Queue of orders at this price level (FIFO order)
    orders: VecDeque<OrderEntry>,
    /// Total quantity available at this level
    total_quantity: Quantity,
}

/// Entry in the price level queue
#[derive(Debug, Clone)]
struct OrderEntry {
    order_id: OrderId,
    account_id: AccountId,
    remaining_quantity: Quantity,
}

impl PriceLevel {
    /// Create a new empty price level
    pub fn new() -> Self {
        Self {
            orders: VecDeque::new(),
            total_quantity: Quantity::zero(),
        }
    }

    /// Insert an order at the back of the queue (time priority)
    pub fn insert(&mut self, order_id: OrderId, account_id: AccountId, quantity: Quantity) {
        self.orders.push_back(OrderEntry {
            order_id,
            account_id,
            remaining_quantity: quantity,
        });
        self.total_quantity = self.total_quantity + quantity;
    }

    /// Remove an order from the queue by OrderId
    ///
    /// Returns the remaining quantity of the removed order, or None if not found
    pub fn remove(&mut self, order_id: &OrderId) -> Option<Quantity> {
        // Find and remove the order
        let position = self.orders.iter().position(|entry| &entry.order_id == order_id)?;
        let entry = self.orders.remove(position)?;
        
        // Update total quantity
        self.total_quantity = Quantity::try_new(
            self.total_quantity.as_decimal() - entry.remaining_quantity.as_decimal()
        ).unwrap_or(Quantity::zero());
        
        Some(entry.remaining_quantity)
    }

    /// Peek at the front order without removing it
    ///
    /// Returns (order_id, account_id, quantity)
    pub fn peek_front(&self) -> Option<(OrderId, AccountId, Quantity)> {
        self.orders.front().map(|entry| (entry.order_id, entry.account_id, entry.remaining_quantity))
    }

    /// Pop the front order from the queue
    pub fn pop_front(&mut self) -> Option<(OrderId, Quantity)> {
        let entry = self.orders.pop_front()?;
        
        // Update total quantity
        self.total_quantity = Quantity::try_new(
            self.total_quantity.as_decimal() - entry.remaining_quantity.as_decimal()
        ).unwrap_or(Quantity::zero());
        
        Some((entry.order_id, entry.remaining_quantity))
    }

    /// Update the remaining quantity for the front order
    ///
    /// Used when an order is partially filled. If quantity becomes zero,
    /// the order is automatically removed.
    pub fn update_front_quantity(&mut self, new_quantity: Quantity) -> bool {
        if let Some(entry) = self.orders.front_mut() {
            let old_quantity = entry.remaining_quantity;
            
            if new_quantity.is_zero() {
                // Remove the order if fully filled
                self.orders.pop_front();
            } else {
                entry.remaining_quantity = new_quantity;
            }
            
            // Update total quantity
            self.total_quantity = Quantity::try_new(
                self.total_quantity.as_decimal() - old_quantity.as_decimal() + new_quantity.as_decimal()
            ).unwrap_or(Quantity::zero());
            
            true
        } else {
            false
        }
    }

    /// Check if the price level is empty
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Get the total quantity at this price level
    pub fn total_quantity(&self) -> Quantity {
        self.total_quantity
    }

    /// Get the number of orders at this level
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }
}

impl Default for PriceLevel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_level_insert() {
        let mut level = PriceLevel::new();
        let order_id = OrderId::new();
        let account_id = AccountId::new();
        let qty = Quantity::from_str("1.5").unwrap();

        level.insert(order_id, account_id, qty);

        assert_eq!(level.order_count(), 1);
        assert_eq!(level.total_quantity(), qty);
        assert!(!level.is_empty());
    }

    #[test]
    fn test_price_level_fifo_order() {
        let mut level = PriceLevel::new();
        let account_id = AccountId::new();
        let order1 = OrderId::new();
        let order2 = OrderId::new();
        let order3 = OrderId::new();

        level.insert(order1, account_id, Quantity::from_str("1.0").unwrap());
        level.insert(order2, account_id, Quantity::from_str("2.0").unwrap());
        level.insert(order3, account_id, Quantity::from_str("3.0").unwrap());

        // First order should be at front
        let (front_id, _, front_qty) = level.peek_front().unwrap();
        assert_eq!(front_id, order1);
        assert_eq!(front_qty, Quantity::from_str("1.0").unwrap());
    }

    #[test]
    fn test_price_level_remove() {
        let mut level = PriceLevel::new();
        let account_id = AccountId::new();
        let order1 = OrderId::new();
        let order2 = OrderId::new();

        level.insert(order1, account_id, Quantity::from_str("1.0").unwrap());
        level.insert(order2, account_id, Quantity::from_str("2.0").unwrap());

        // Remove middle order
        let removed_qty = level.remove(&order1);
        assert_eq!(removed_qty, Some(Quantity::from_str("1.0").unwrap()));
        assert_eq!(level.order_count(), 1);
        assert_eq!(level.total_quantity(), Quantity::from_str("2.0").unwrap());
    }

    #[test]
    fn test_price_level_pop_front() {
        let mut level = PriceLevel::new();
        let account_id = AccountId::new();
        let order1 = OrderId::new();
        let order2 = OrderId::new();

        level.insert(order1, account_id, Quantity::from_str("1.0").unwrap());
        level.insert(order2, account_id, Quantity::from_str("2.0").unwrap());

        let (popped_id, _) = level.pop_front().unwrap();
        assert_eq!(popped_id, order1);
        assert_eq!(level.order_count(), 1);
    }

    #[test]
    fn test_price_level_update_front_quantity() {
        let mut level = PriceLevel::new();
        let account_id = AccountId::new();
        let order_id = OrderId::new();

        level.insert(order_id, account_id, Quantity::from_str("5.0").unwrap());

        // Partial fill
        level.update_front_quantity(Quantity::from_str("3.0").unwrap());
        assert_eq!(level.total_quantity(), Quantity::from_str("3.0").unwrap());
        assert_eq!(level.order_count(), 1);

        // Complete fill (zero quantity)
        level.update_front_quantity(Quantity::zero());
        assert!(level.is_empty());
        assert_eq!(level.total_quantity(), Quantity::zero());
    }

    #[test]
    fn test_price_level_total_quantity_invariant() {
        let mut level = PriceLevel::new();
        let account_id = AccountId::new();
        
        level.insert(OrderId::new(), account_id, Quantity::from_str("1.5").unwrap());
        level.insert(OrderId::new(), account_id, Quantity::from_str("2.5").unwrap());
        level.insert(OrderId::new(), account_id, Quantity::from_str("3.0").unwrap());

        // Total should be sum of all quantities
        assert_eq!(level.total_quantity(), Quantity::from_str("7.0").unwrap());
    }
}
