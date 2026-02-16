//! Matching Engine Service
//!
//! High-performance order matching engine implementing price-time priority
//! matching per spec §1 (Order Lifecycle) and §3 (Trade Lifecycle).
//!
//! **Performance Targets:**
//! - Matching latency: <500μs (p99)
//! - Throughput: 100,000 orders/sec per symbol
//!
//! **Key Invariants:**
//! - Price-time priority strictly enforced
//! - Deterministic matching (same inputs → same outputs)
//! - No self-trades
//! - Conservation of quantity

pub mod book;
pub mod matching;
pub mod engine;
pub mod events;

pub use engine::MatchingEngine;
