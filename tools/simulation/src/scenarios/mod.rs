//! Scenario simulation modules
//!
//! Each scenario exercises specific exchange behavior under stress conditions.

pub mod volatility_spike;
pub mod latency_injection;
pub mod order_flood;
pub mod liquidation_cascade;
pub mod incentive;

use crate::engine::SimEngine;
use serde::{Deserialize, Serialize};

/// Result of a scenario run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub name: String,
    pub ticks_run: u64,
    pub orders_submitted: u64,
    pub trades_executed: u64,
    pub events_emitted: usize,
    pub passed: bool,
    pub details: String,
}
