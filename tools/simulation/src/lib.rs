//! Simulation & Liquidity Testing Framework
//!
//! Production-grade simulation framework for the distributed exchange.
//! Exercises order matching, market-making, risk scenarios, and stress testing
//! with deterministic, spec-compliant behavior.
//!
//! # Modules
//! - `engine` — Deterministic matching engine with order book
//! - `bots` — Market maker and retail trader bots
//! - `scenarios` — Volatility, latency, flood, liquidation, incentive scenarios
//! - `metrics` — Performance counters and latency histograms
//! - `reports` — Depth, slippage, and profitability reports
//! - `multi_market` — Multi-market concurrent simulation
//! - `replay` — Event log and deterministic replay validation
//! - `export` — Metrics and report JSON export

pub mod engine;
pub mod bots;
pub mod scenarios;
pub mod metrics;
pub mod reports;
pub mod multi_market;
pub mod replay;
pub mod export;

/// Crate version constant
pub const VERSION: &str = "1.0.0";
