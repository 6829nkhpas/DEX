//! Order lifecycle types
//!
//! Implements spec §1 (Order Lifecycle) and §2 (Order States)

use crate::ids::{AccountId, MarketId, OrderId};
use crate::numeric::{Price, Quantity};
use serde::{Deserialize, Serialize};

/// Order side (buyer or seller)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    /// Buy order (bid)
    BUY,
    /// Sell order (ask)
    SELL,
}

impl Side {
    /// Get the opposite side
    pub fn opposite(&self) -> Self {
        match self {
            Side::BUY => Side::SELL,
            Side::SELL => Side::BUY,
        }
    }
}

/// Time-in-force policy for orders
///
/// Defines how long an order remains active per spec §1.5
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum TimeInForce {
    /// Good-Till-Cancel: remains until filled or explicitly canceled
    GTC,
    /// Immediate-Or-Cancel: match immediately, cancel remainder
    IOC,
    /// Fill-Or-Kill: full match or reject entirely
    FOK,
    /// Good-Till-Date: expire at specified Unix nanos timestamp
    GTD(i64),
}

/// Order status enum matching spec §2.2 exactly
///
/// State IDs match specification for wire protocol compatibility
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason")]
pub enum OrderStatus {
    /// State 0: Accepted and awaiting matching
    #[serde(rename = "PENDING")]
    Pending,
    
    /// State 1: Partially matched
    #[serde(rename = "PARTIAL")]
    Partial,
    
    /// State 2: Completely matched (terminal)
    #[serde(rename = "FILLED")]
    Filled,
    
    /// State 3: Canceled by user or system (terminal)
    #[serde(rename = "CANCELED")]
    Canceled(CancelReason),
    
    /// State 4: Failed validation (terminal)
    #[serde(rename = "REJECTED")]
    Rejected(RejectReason),
    
    /// State 5: Time-in-force deadline reached (terminal)
    #[serde(rename = "EXPIRED")]
    Expired,
}

impl OrderStatus {
    /// Check if status is terminal (no further transitions possible)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled | OrderStatus::Canceled(_) | OrderStatus::Rejected(_) | OrderStatus::Expired
        )
    }

    /// Get the state ID for wire protocol
    pub fn state_id(&self) -> u8 {
        match self {
            OrderStatus::Pending => 0,
            OrderStatus::Partial => 1,
            OrderStatus::Filled => 2,
            OrderStatus::Canceled(_) => 3,
            OrderStatus::Rejected(_) => 4,
            OrderStatus::Expired => 5,
        }
    }
}

/// Cancel reasons per spec §2.2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancelReason {
    UserRequested,
    SelfTrade,
    PostOnlyReject,
    InsufficientMargin,
    RiskLimitBreach,
    AdminCancel,
}

/// Reject reasons per spec §2.2
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RejectReason {
    InvalidSchema,
    InvalidPrice,
    InvalidQuantity,
    InsufficientBalance,
    SymbolNotFound,
    AccountSuspended,
    RateLimited,
}

/// Complete order structure per spec §1
///
/// Includes all fields required for order lifecycle tracking
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub order_id: OrderId,
    pub account_id: AccountId,
    pub symbol: MarketId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub filled_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub status: OrderStatus,
    pub time_in_force: TimeInForce,
    pub created_at: i64,  // Unix nanos
    pub updated_at: i64,  // Unix nanos
    pub version: u64,     // Optimistic locking
}

impl Order {
    /// Create a new pending order
    pub fn new(
        account_id: AccountId,
        symbol: MarketId,
        side: Side,
        price: Price,
        quantity: Quantity,
        time_in_force: TimeInForce,
        timestamp: i64,
    ) -> Self {
        Self {
            order_id: OrderId::new(),
            account_id,
            symbol,
            side,
            price,
            quantity,
            filled_quantity: Quantity::zero(),
            remaining_quantity: quantity,
            status: OrderStatus::Pending,
            time_in_force,
            created_at: timestamp,
            updated_at: timestamp,
            version: 0,
        }
    }

    /// Check quantity invariant: filled + remaining = total
    pub fn check_invariant(&self) -> bool {
        self.filled_quantity.as_decimal() + self.remaining_quantity.as_decimal()
            == self.quantity.as_decimal()
    }

    /// Check if order is completely filled
    pub fn is_filled(&self) -> bool {
        self.filled_quantity == self.quantity
    }

    /// Check if order has any fills
    pub fn has_fills(&self) -> bool {
        !self.filled_quantity.is_zero()
    }

    /// Update filled quantity and adjust status
    ///
    /// # Panics
    /// Panics if the fill would exceed total quantity or violate invariants
    pub fn add_fill(&mut self, fill_quantity: Quantity, timestamp: i64) {
        let new_filled = self.filled_quantity + fill_quantity;
        
        assert!(
            new_filled.as_decimal() <= self.quantity.as_decimal(),
            "Fill would exceed order quantity"
        );

        self.filled_quantity = new_filled;
        self.remaining_quantity = Quantity::try_new(
            self.quantity.as_decimal() - new_filled.as_decimal()
        ).unwrap_or(Quantity::zero());
        
        // Update status based on fill
        if self.is_filled() {
            self.status = OrderStatus::Filled;
        } else if self.has_fills() {
            self.status = OrderStatus::Partial;
        }
        
        self.updated_at = timestamp;
        self.version += 1;

        assert!(self.check_invariant(), "Invariant violated after fill");
    }

    /// Cancel the order
    ///
    /// # Panics
    /// Panics if order is already in terminal state
    pub fn cancel(&mut self, reason: CancelReason, timestamp: i64) {
        assert!(!self.status.is_terminal(), "Cannot cancel terminal order");
        
        self.status = OrderStatus::Canceled(reason);
        self.updated_at = timestamp;
        self.version += 1;
    }

    /// Reject the order
    pub fn reject(reason: RejectReason, timestamp: i64) -> OrderStatus {
        OrderStatus::Rejected(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_side_opposite() {
        assert_eq!(Side::BUY.opposite(), Side::SELL);
        assert_eq!(Side::SELL.opposite(), Side::BUY);
    }

    #[test]
    fn test_order_creation() {
        let order = Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        );

        assert_eq!(order.status, OrderStatus::Pending);
        assert!(order.check_invariant());
        assert!(!order.has_fills());
    }

    #[test]
    fn test_order_fill() {
        let mut order = Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        );

        // Partial fill
        order.add_fill(Quantity::from_str("0.3").unwrap(), 1708123456790000000);
        assert_eq!(order.status, OrderStatus::Partial);
        assert!(order.has_fills());
        assert!(!order.is_filled());
        assert!(order.check_invariant());

        // Complete fill
        order.add_fill(Quantity::from_str("0.7").unwrap(), 1708123456791000000);
        assert_eq!(order.status, OrderStatus::Filled);
        assert!(order.is_filled());
        assert!(order.check_invariant());
    }

    #[test]
    #[should_panic(expected = "Fill would exceed order quantity")]
    fn test_order_overfill_panics() {
        let mut order = Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        );

        order.add_fill(Quantity::from_str("1.5").unwrap(), 1708123456790000000);
    }

    #[test]
    fn test_order_cancel() {
        let mut order = Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        );

        order.cancel(CancelReason::UserRequested, 1708123456790000000);
        assert_eq!(order.status, OrderStatus::Canceled(CancelReason::UserRequested));
        assert!(order.status.is_terminal());
    }

    #[test]
    #[should_panic(expected = "Cannot cancel terminal order")]
    fn test_cancel_terminal_panics() {
        let mut order = Order::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("1.0").unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        );

        order.add_fill(Quantity::from_str("1.0").unwrap(), 1708123456790000000);
        order.cancel(CancelReason::UserRequested, 1708123456791000000);
    }

    #[test]
    fn test_order_status_state_ids() {
        assert_eq!(OrderStatus::Pending.state_id(), 0);
        assert_eq!(OrderStatus::Partial.state_id(), 1);
        assert_eq!(OrderStatus::Filled.state_id(), 2);
        assert_eq!(OrderStatus::Canceled(CancelReason::UserRequested).state_id(), 3);
        assert_eq!(OrderStatus::Rejected(RejectReason::InvalidPrice).state_id(), 4);
        assert_eq!(OrderStatus::Expired.state_id(), 5);
    }

    #[test]
    fn test_order_serialization() {
        let order = Order::new(
            AccountId::new(),
            MarketId::new("ETH/USDC"),
            Side::SELL,
            Price::from_str("3000.50").unwrap(),
            Quantity::from_str("2.5").unwrap(),
            TimeInForce::IOC,
            1708123456789000000,
        );

        let json = serde_json::to_string(&order).unwrap();
        let deserialized: Order = serde_json::from_str(&json).unwrap();
        
        assert_eq!(order.order_id, deserialized.order_id);
        assert_eq!(order.side, deserialized.side);
        assert_eq!(order.price, deserialized.price);
    }
}

