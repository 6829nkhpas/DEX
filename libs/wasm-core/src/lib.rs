//! Wasm Core — Client Computation Layer
//!
//! Provides deterministic, client-side computation for:
//! - Portfolio aggregation and PnL tracking
//! - Margin preview and risk assessment
//! - Order fill simulation against mock order books
//! - Transaction signing and verification
//!
//! # Determinism
//! All functions are pure: no system time, no RNG, no external calls.
//! Uses `Decimal` (fixed-point) and `BTreeMap` (sorted iteration) throughout.
//!
//! # Version
//! v1.0.0 — Frozen specification compliant

pub mod portfolio;
pub mod margin;
pub mod simulation;
pub mod signing;

/// Crate version constant
pub const WASM_CORE_VERSION: &str = "1.0.0";
