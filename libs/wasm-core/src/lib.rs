//! Wasm Core — Client Computation Layer
//!
//! Provides deterministic, client-side computation for:
//! - Portfolio aggregation and PnL tracking
//! - Margin preview and risk assessment
//! - Order fill simulation against mock order books
//! - Transaction signing and verification
//!
//! # WASM Module (Phase 12)
//!
//! The `wasm_bindings` module defines the narrow WASM boundary for
//! `CrossMarginEngine::simulate_order`. It accepts JSON input, performs
//! pure deterministic computation, and returns JSON output. The
//! `wasm_adapter` module provides host-side invocation with validation
//! and native fallback.
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

// WASM boundary module — JSON FFI entry point for margin preview.
// Available on all targets so boundary logic can be tested natively.
// WASM-specific FFI functions (alloc/dealloc/entry) are cfg-gated
// behind `#[cfg(target_arch = "wasm32")]` inside the module.
pub mod wasm_bindings;

// Host-side adapter — invocation, validation, and fallback.
// Provides a unified API that dispatches to WASM boundary or native
// computation based on a runtime feature flag.
pub mod wasm_adapter;

// WASM integration tests
#[cfg(test)]
mod wasm_tests;

/// Crate version constant
pub const WASM_CORE_VERSION: &str = "1.0.0";
