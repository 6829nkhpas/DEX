//! Replay attack simulation.
//! Tests that replaying the exact same signed payload or order is rejected.

use types::ids::{AccountId, MarketId, OrderId};
use types::order::{Side, TimeInForce, Order};
use types::numeric::{Price, Quantity};
use std::str::FromStr;
use std::collections::HashSet;

/// Simulates an exchange endpoint that drops replayed messages referencing the same Order ID
/// or sequence number.
pub struct ReplayDetector {
    seen_orders: HashSet<OrderId>,
}

impl ReplayDetector {
    pub fn new() -> Self {
        Self {
            seen_orders: HashSet::new(),
        }
    }

    /// Process an order. Returns true if accepted, false if it's a replay.
    pub fn process_order(&mut self, order: &Order) -> bool {
        if self.seen_orders.contains(&order.order_id) {
            return false;
        }
        self.seen_orders.insert(order.order_id);
        true
    }
}

impl Default for ReplayDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_attack_mitigation() {
        let mut detector = ReplayDetector::new();
        
        let account_id = AccountId::new();
        let symbol = MarketId::new("BTC/USDT");
        
        // Attacker observes a legitimate order
        let legit_order = Order::new(
            account_id,
            symbol,
            Side::BUY,
            Price::from_str("50000").unwrap(),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000, 
        );

        // System accepts it once
        assert!(detector.process_order(&legit_order), "Legitimate order rejected");

        // Attacker captures and replays exact same order payload
        let replayed_order = legit_order.clone();
        
        // System must reject the replay
        assert!(!detector.process_order(&replayed_order), "Replay attack succeeded");
    }
}
