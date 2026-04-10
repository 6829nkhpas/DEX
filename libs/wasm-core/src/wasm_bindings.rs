//! WASM Bindings — FFI entry point for margin preview
//!
//! This module defines the narrow, explicit boundary between the WASM module
//! and its host. All data crosses the boundary as JSON-encoded strings with
//! decimals and IDs represented as strings (never as floats or raw bytes).
//!
//! # Boundary Rules
//!
//! ## Allowed
//! - Memory allocation/deallocation for string passing
//! - Pure deterministic computation (no side effects)
//! - JSON serialization/deserialization
//!
//! ## Forbidden
//! - Network calls, I/O, file access
//! - System clock / time access
//! - Random number generation
//! - State mutation outside the call boundary
//! - Wallet trust decisions, order submission
//! - Any operation that could produce non-deterministic output
//!
//! # Determinism Guarantees
//! - All arithmetic uses `rust_decimal::Decimal` (fixed-point)
//! - All maps use `BTreeMap` (sorted iteration)
//! - Rounding strategies: AwayFromZero (margins), ToZero (available), MidpointAwayFromZero (display)
//! - String-encoded decimals across the boundary (no f64 conversion)
//!
//! # Safety
//! All outputs are advisory. The Rust core services remain authoritative
//! for every state transition. WASM results must be validated before display.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::margin::{CrossMarginEngine, MarginPreview, RiskLevel};
use types::ids::AccountId;
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::position::{Position, PositionSide};

// ---------------------------------------------------------------------------
// JSON Boundary Types — Input
// ---------------------------------------------------------------------------

/// Top-level input for margin preview computation.
///
/// All numeric fields are string-encoded decimals. All IDs are string-encoded
/// UUIDs. This matches the project's canonical wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginPreviewInput {
    /// Account UUID as string
    pub account_id: String,
    /// Total account balance in quote currency (decimal string)
    pub total_balance: String,
    /// Existing positions (may be empty)
    pub positions: Vec<PositionInput>,
    /// The hypothetical order to simulate
    pub order: OrderInput,
}

/// A position snapshot passed into WASM.
///
/// Mirrors `types::position::Position` but with all fields as strings
/// for cross-boundary safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInput {
    /// Trading pair (e.g. "BTC/USDT")
    pub symbol: String,
    /// "LONG" or "SHORT"
    pub side: String,
    /// Position size (decimal string)
    pub size: String,
    /// Entry price (decimal string)
    pub entry_price: String,
    /// Current mark price (decimal string)
    pub mark_price: String,
    /// Liquidation price (decimal string)
    pub liquidation_price: String,
    /// Initial margin allocated (decimal string)
    pub initial_margin: String,
    /// Maintenance margin required (decimal string)
    pub maintenance_margin: String,
    /// Leverage tier (1-125)
    pub leverage: u8,
    /// Position open timestamp (Unix nanos)
    pub timestamp: i64,
}

/// The hypothetical order to simulate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInput {
    /// Trading pair (e.g. "ETH/USDT")
    pub symbol: String,
    /// "BUY" or "SELL"
    pub side: String,
    /// Order price (decimal string)
    pub price: String,
    /// Order quantity (decimal string)
    pub quantity: String,
    /// Leverage tier (1-125)
    pub leverage: u8,
}

// ---------------------------------------------------------------------------
// JSON Boundary Types — Output
// ---------------------------------------------------------------------------

/// Result of a margin preview computation.
///
/// All numeric fields are string-encoded decimals. Risk level is a string
/// enum value. This is the canonical output format for the WASM boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginPreviewOutput {
    /// Equity after hypothetical trade (decimal string)
    pub equity_after: String,
    /// Total margin used after trade (decimal string)
    pub margin_used_after: String,
    /// Available margin after trade (decimal string)
    pub margin_available_after: String,
    /// Margin ratio after trade (decimal string)
    pub margin_ratio_after: String,
    /// Estimated liquidation price (decimal string)
    pub liquidation_price: String,
    /// Effective leverage ratio (decimal string)
    pub leverage_ratio: String,
    /// Risk classification: "Healthy", "Warning", "Danger", or "Liquidation"
    pub risk_level: String,
    /// Whether the computed balance would become negative
    pub has_negative_balance: bool,
}

// ---------------------------------------------------------------------------
// Error type for boundary operations
// ---------------------------------------------------------------------------

/// Errors that can occur at the WASM boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WasmError {
    pub code: String,
    pub message: String,
}

impl WasmError {
    fn input_error(msg: impl Into<String>) -> Self {
        Self {
            code: "INPUT_ERROR".to_owned(),
            message: msg.into(),
        }
    }

    fn computation_error(msg: impl Into<String>) -> Self {
        Self {
            code: "COMPUTATION_ERROR".to_owned(),
            message: msg.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Core boundary function
// ---------------------------------------------------------------------------

/// Compute a margin preview from a JSON input string.
///
/// This is the core function that defines the WASM boundary. It:
/// 1. Deserializes the JSON input into typed boundary structs
/// 2. Parses and validates all fields (strings → Decimals, strings → IDs)
/// 3. Constructs a `CrossMarginEngine` with the provided snapshot
/// 4. Calls `simulate_order` with the hypothetical order
/// 5. Converts the result to the boundary output format
/// 6. Serializes to JSON string
///
/// # Determinism
/// Given identical JSON input, this function always produces identical JSON output.
/// No system state, clock, or RNG is consulted.
///
/// # Errors
/// Returns a JSON-encoded `WasmError` if:
/// - Input JSON is malformed
/// - Any decimal string is unparseable
/// - Side string is not "BUY"/"SELL" or "LONG"/"SHORT"
/// - Account ID is not a valid UUID
pub fn margin_preview_json(input_json: &str) -> Result<String, String> {
    // 1. Deserialize input
    let input: MarginPreviewInput = serde_json::from_str(input_json)
        .map_err(|e| {
            serde_json::to_string(&WasmError::input_error(format!(
                "JSON parse error: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"JSON parse error"}"#.to_owned())
        })?;

    // 2. Parse account ID
    let account_uuid = uuid::Uuid::parse_str(&input.account_id)
        .map_err(|e| {
            serde_json::to_string(&WasmError::input_error(format!(
                "Invalid account_id UUID: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"Invalid account_id"}"#.to_owned())
        })?;
    let account_id = AccountId::from_uuid(account_uuid);

    // 3. Parse total balance
    let total_balance = Decimal::from_str(&input.total_balance)
        .map_err(|e| {
            serde_json::to_string(&WasmError::input_error(format!(
                "Invalid total_balance: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"Invalid total_balance"}"#.to_owned())
        })?;

    // 4. Build engine
    let mut engine = CrossMarginEngine::new(account_id, total_balance);

    // 5. Parse and add positions
    for (i, pos_input) in input.positions.iter().enumerate() {
        let position = parse_position_input(pos_input, account_id, i)?;
        engine.add_position(position);
    }

    // 6. Parse order
    let order_side = parse_order_side(&input.order.side)?;
    let order_price = Price::from_str(&input.order.price)
        .map_err(|e| {
            serde_json::to_string(&WasmError::input_error(format!(
                "Invalid order price: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"Invalid order price"}"#.to_owned())
        })?;
    let order_qty = Quantity::from_str(&input.order.quantity)
        .map_err(|e| {
            serde_json::to_string(&WasmError::input_error(format!(
                "Invalid order quantity: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"Invalid order quantity"}"#.to_owned())
        })?;

    // 7. Compute
    let preview = engine.simulate_order(
        &input.order.symbol,
        order_side,
        order_price,
        order_qty,
        input.order.leverage,
    );

    // 8. Convert to boundary output
    let output = margin_preview_to_output(&preview);

    // 9. Serialize
    serde_json::to_string(&output)
        .map_err(|e| {
            serde_json::to_string(&WasmError::computation_error(format!(
                "Output serialization error: {e}"
            )))
            .unwrap_or_else(|_| r#"{"code":"COMPUTATION_ERROR","message":"Serialization error"}"#.to_owned())
        })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a `PositionInput` into a `Position`.
fn parse_position_input(
    input: &PositionInput,
    account_id: AccountId,
    index: usize,
) -> Result<Position, String> {
    let ctx = |field: &str, e: impl std::fmt::Display| -> String {
        serde_json::to_string(&WasmError::input_error(format!(
            "positions[{index}].{field}: {e}"
        )))
        .unwrap_or_else(|_| format!(r#"{{"code":"INPUT_ERROR","message":"positions[{index}].{field}"}}"#))
    };

    let side = parse_position_side(&input.side)
        .map_err(|e| ctx("side", e))?;
    let size = Quantity::from_str(&input.size)
        .map_err(|e| ctx("size", e))?;
    let entry_price = Price::from_str(&input.entry_price)
        .map_err(|e| ctx("entry_price", e))?;
    let mark_price = Price::from_str(&input.mark_price)
        .map_err(|e| ctx("mark_price", e))?;
    let liquidation_price = Price::from_str(&input.liquidation_price)
        .map_err(|e| ctx("liquidation_price", e))?;
    let initial_margin = Decimal::from_str(&input.initial_margin)
        .map_err(|e| ctx("initial_margin", e))?;
    let maintenance_margin = Decimal::from_str(&input.maintenance_margin)
        .map_err(|e| ctx("maintenance_margin", e))?;

    let market_id = types::ids::MarketId::new(&input.symbol);

    Ok(Position::new(
        account_id,
        market_id,
        side,
        size,
        entry_price,
        mark_price,
        liquidation_price,
        initial_margin,
        maintenance_margin,
        input.leverage,
        input.timestamp,
    ))
}

/// Parse order side from string.
fn parse_order_side(s: &str) -> Result<Side, String> {
    match s {
        "BUY" => Ok(Side::BUY),
        "SELL" => Ok(Side::SELL),
        other => Err(
            serde_json::to_string(&WasmError::input_error(format!(
                "Invalid order side: '{other}'. Expected 'BUY' or 'SELL'"
            )))
            .unwrap_or_else(|_| r#"{"code":"INPUT_ERROR","message":"Invalid order side"}"#.to_owned()),
        ),
    }
}

/// Parse position side from string.
fn parse_position_side(s: &str) -> Result<PositionSide, &'static str> {
    match s {
        "LONG" => Ok(PositionSide::LONG),
        "SHORT" => Ok(PositionSide::SHORT),
        _ => Err("Expected 'LONG' or 'SHORT'"),
    }
}

/// Convert `MarginPreview` to the boundary output format.
///
/// All `Decimal` values are converted to their canonical string representation.
/// `RiskLevel` is converted to its string name.
fn margin_preview_to_output(preview: &MarginPreview) -> MarginPreviewOutput {
    MarginPreviewOutput {
        equity_after: preview.equity_after.to_string(),
        margin_used_after: preview.margin_used_after.to_string(),
        margin_available_after: preview.margin_available_after.to_string(),
        margin_ratio_after: preview.margin_ratio_after.to_string(),
        liquidation_price: preview.liquidation_price.to_string(),
        leverage_ratio: preview.leverage_ratio.to_string(),
        risk_level: risk_level_to_string(preview.risk_level),
        has_negative_balance: preview.has_negative_balance,
    }
}

/// Convert `RiskLevel` to its canonical string representation.
fn risk_level_to_string(level: RiskLevel) -> String {
    match level {
        RiskLevel::Healthy => "Healthy".to_owned(),
        RiskLevel::Warning => "Warning".to_owned(),
        RiskLevel::Danger => "Danger".to_owned(),
        RiskLevel::Liquidation => "Liquidation".to_owned(),
    }
}

/// Parse a risk level from its string representation.
pub fn risk_level_from_string(s: &str) -> Option<RiskLevel> {
    match s {
        "Healthy" => Some(RiskLevel::Healthy),
        "Warning" => Some(RiskLevel::Warning),
        "Danger" => Some(RiskLevel::Danger),
        "Liquidation" => Some(RiskLevel::Liquidation),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// WASM FFI Exports (only compiled for wasm32 target)
// ---------------------------------------------------------------------------

/// WASM-specific FFI functions for memory management and entry points.
///
/// These functions use raw pointers and `extern "C"` ABI for compatibility
/// with WASM host runtimes (wasmtime, wasmer, browser).
///
/// Memory protocol:
/// 1. Host calls `wasm_alloc(size)` to get a pointer in WASM memory
/// 2. Host writes input JSON bytes to that pointer
/// 3. Host calls `wasm_margin_preview(ptr, len)` → returns result pointer
/// 4. Result layout: [4 bytes: u32 length][N bytes: JSON string]
/// 5. Host reads length, then reads that many bytes of JSON
/// 6. Host calls `wasm_dealloc(ptr, len)` to free input and result buffers
#[cfg(target_arch = "wasm32")]
pub mod wasm_ffi {
    use super::*;
    use std::alloc::{alloc, dealloc, Layout};

    /// Allocate `size` bytes in WASM linear memory.
    ///
    /// Returns a pointer to the allocated block, or null on failure.
    #[no_mangle]
    pub extern "C" fn wasm_alloc(size: usize) -> *mut u8 {
        if size == 0 {
            return std::ptr::null_mut();
        }
        let layout = Layout::from_size_align(size, 1).expect("Invalid layout");
        unsafe { alloc(layout) }
    }

    /// Deallocate a previously allocated block.
    #[no_mangle]
    pub extern "C" fn wasm_dealloc(ptr: *mut u8, size: usize) {
        if ptr.is_null() || size == 0 {
            return;
        }
        let layout = Layout::from_size_align(size, 1).expect("Invalid layout");
        unsafe { dealloc(ptr, layout) }
    }

    /// WASM entry point for margin preview.
    ///
    /// Takes a pointer and length to a UTF-8 JSON input string in WASM memory.
    /// Returns a pointer to a result buffer laid out as:
    ///   [4 bytes: u32 little-endian length][N bytes: UTF-8 JSON result]
    ///
    /// On success, the JSON result is a `MarginPreviewOutput`.
    /// On error, the JSON result is a `WasmError`.
    ///
    /// The caller must free the returned buffer using `wasm_dealloc` with
    /// size = 4 + length_from_header.
    #[no_mangle]
    pub extern "C" fn wasm_margin_preview(ptr: *const u8, len: usize) -> *mut u8 {
        // Read input from WASM memory
        let input_bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        let input_str = match std::str::from_utf8(input_bytes) {
            Ok(s) => s,
            Err(_) => return encode_result(
                r#"{"code":"INPUT_ERROR","message":"Input is not valid UTF-8"}"#,
            ),
        };

        // Compute
        let result_json = match margin_preview_json(input_str) {
            Ok(json) => json,
            Err(err_json) => err_json,
        };

        encode_result(&result_json)
    }

    /// Encode a JSON string into the WASM result buffer format.
    ///
    /// Layout: [4 bytes: u32 LE length][N bytes: UTF-8 JSON]
    fn encode_result(json: &str) -> *mut u8 {
        let json_bytes = json.as_bytes();
        let total_size = 4 + json_bytes.len();

        let layout = Layout::from_size_align(total_size, 1).expect("Invalid layout");
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return ptr;
        }

        // Write length header (little-endian u32)
        let len_bytes = (json_bytes.len() as u32).to_le_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(len_bytes.as_ptr(), ptr, 4);
            std::ptr::copy_nonoverlapping(json_bytes.as_ptr(), ptr.add(4), json_bytes.len());
        }

        ptr
    }
}
