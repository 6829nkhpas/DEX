//! Risk Engine Service
//!
//! Implements risk management per specs:
//! - §5 (Margin Methodology)
//! - §6 (Liquidation Process)
//! - §9.3.6 (Risk Service Boundaries)
//!
//! Provides pre-trade validation, margin calculations,
//! liquidation monitoring, and exposure tracking.

pub mod margin;
pub mod exposure;
pub mod liquidation;
pub mod validator;
pub mod events;
pub mod engine;
