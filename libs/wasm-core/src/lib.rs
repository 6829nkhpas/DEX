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
//! # Host Integration (Phase 13)
//!
//! The `wasm_host` module (behind `wasm-host` feature) provides actual
//! WASM runtime execution via wasmtime. The `wasm_bench` module provides
//! lightweight benchmarking hooks for comparing execution paths.
//! The adapter now supports three execution modes: Native, Boundary,
//! and WasmRuntime, with automatic fallback to native on any failure.
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
// computation based on a runtime feature flag or execution mode.
pub mod wasm_adapter;

// Host-side WASM runtime — loads and executes .wasm modules via wasmtime.
// Only available when the `wasm-host` feature is enabled.
#[cfg(feature = "wasm-host")]
pub mod wasm_host;

// Benchmarking hooks — lightweight instrumentation for comparing execution paths.
// Available on native targets only (uses std::time::Instant).
#[cfg(not(target_arch = "wasm32"))]
pub mod wasm_bench;

// WASM integration tests (Phase 12)
#[cfg(test)]
mod wasm_tests;

// WASM host integration tests (Phase 13)
#[cfg(test)]
mod wasm_host_tests;

// WASM verification tests (Phase 14)
#[cfg(test)]
mod wasm_verification_tests;

// WASM benchmark tests (Phase 14)
#[cfg(test)]
mod wasm_benchmark_tests;

/// Crate version constant
pub const WASM_CORE_VERSION: &str = "1.0.0";
