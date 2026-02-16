//! Unique identifier types for exchange entities
//!
//! All IDs use UUID v7 for time-sortable ordering, enabling efficient
//! chronological queries and replay capabilities per spec §13 (Timestamp Policy).

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for an order
///
/// Uses UUID v7 for time-based sorting. Orders can be efficiently
/// queried in chronological order using the embedded timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrderId(Uuid);

impl OrderId {
    /// Create a new OrderId with current timestamp
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Create from existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get inner UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a trade
///
/// Uses UUID v7 for time-based sorting and global trade sequence tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TradeId(Uuid);

impl TradeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TradeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TradeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an account
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Market identifier (trading pair)
///
/// Format: "BASE/QUOTE" (e.g., "BTC/USDT", "ETH/USDC")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketId(String);

impl MarketId {
    /// Create a new MarketId from a string
    ///
    /// # Panics
    /// Panics if the format is invalid (must contain '/')
    pub fn new(symbol: impl Into<String>) -> Self {
        let s = symbol.into();
        assert!(s.contains('/'), "MarketId must be in BASE/QUOTE format");
        Self(s)
    }

    /// Try to create a MarketId, returning None if invalid
    pub fn try_new(symbol: impl Into<String>) -> Option<Self> {
        let s = symbol.into();
        if s.contains('/') {
            Some(Self(s))
        } else {
            None
        }
    }

    /// Get the symbol string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Split into base and quote assets
    pub fn split(&self) -> (&str, &str) {
        let parts: Vec<&str> = self.0.split('/').collect();
        (parts[0], parts[1])
    }
}

impl fmt::Display for MarketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MarketId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_id_creation() {
        let id1 = OrderId::new();
        let id2 = OrderId::new();
        assert_ne!(id1, id2, "OrderIds should be unique");
    }

    #[test]
    fn test_order_id_serialization() {
        let id = OrderId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: OrderId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_trade_id_creation() {
        let id1 = TradeId::new();
        let id2 = TradeId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_account_id_creation() {
        let id1 = AccountId::new();
        let id2 = AccountId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_market_id_creation() {
        let market = MarketId::new("BTC/USDT");
        assert_eq!(market.as_str(), "BTC/USDT");
        
        let (base, quote) = market.split();
        assert_eq!(base, "BTC");
        assert_eq!(quote, "USDT");
    }

    #[test]
    fn test_market_id_try_new() {
        assert!(MarketId::try_new("BTC/USDT").is_some());
        assert!(MarketId::try_new("INVALID").is_none());
    }

    #[test]
    #[should_panic(expected = "MarketId must be in BASE/QUOTE format")]
    fn test_market_id_invalid_format() {
        MarketId::new("INVALID");
    }

    #[test]
    fn test_market_id_serialization() {
        let market = MarketId::new("ETH/USDC");
        let json = serde_json::to_string(&market).unwrap();
        assert_eq!(json, "\"ETH/USDC\"");
        
        let deserialized: MarketId = serde_json::from_str(&json).unwrap();
        assert_eq!(market, deserialized);
    }
}
