//! Matching logic module
//!
//! Implements price-time priority matching algorithm

pub mod crossing;
pub mod executor;

pub use crossing::can_match;
pub use executor::MatchExecutor;
