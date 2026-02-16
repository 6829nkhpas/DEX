//! Types library for the distributed exchange
//!
//! This library provides all core type definitions used across the exchange system,
//! ensuring type safety, deterministic behavior, and backward compatibility.
//!
//! # Version
//! v1.0.0 - Frozen specification compliant
//!
//! # Modules
//! - `ids`: Unique identifiers (OrderId, TradeId, AccountId, MarketId)
//! - `numeric`: Fixed-point decimal types (Price, Quantity)
//! - `order`: Order lifecycle types
//! - `trade`: Trade execution types
//! - `account`: Account and balance types
//! - `position`: Position tracking types
//! - `fee`: Fee calculation types
//! - `risk`: Risk management types
//! - `errors`: Error taxonomy

// Public modules
pub mod ids;
pub mod numeric;
pub mod order;
pub mod trade;
pub mod account;
pub mod position;
pub mod fee;
pub mod risk;
pub mod errors;

// Library version constant
pub const LIB_VERSION: &str = "1.0.0";

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::ids::*;
    pub use crate::numeric::*;
    pub use crate::order::*;
    pub use crate::trade::*;
    pub use crate::account::*;
    pub use crate::position::*;
    pub use crate::fee::*;
    pub use crate::risk::*;
    pub use crate::errors::*;
}
