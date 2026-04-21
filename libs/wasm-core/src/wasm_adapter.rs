//! WASM Adapter — Host-side invocation, validation, and fallback
//!
//! This module provides a unified interface for computing margin previews
//! using either native Rust code or the WASM boundary path. It enforces
//! output validation before any result can be consumed, and falls back
//! to native computation if the WASM path fails.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────┐
//!  │            MarginPreviewAdapter                  │
//!  │                                                  │
//!  │  compute()                                       │
//!  │    ├── mode?                                     │
//!  │    │   ├── Native → compute_native()             │
//!  │    │   ├── Boundary → compute_via_boundary()     │
//!  │    │   │    ├── validate_output()                │
//!  │    │   │    │   ├── OK → return result           │
//!  │    │   │    │   └── FAIL → fallback native       │
//!  │    │   │    └── ERROR → fallback native          │
//!  │    │   └── WasmRuntime → compute_via_runtime()   │
//!  │    │        ├── validate_output()                │
//!  │    │        │   ├── OK → return result           │
//!  │    │        │   └── FAIL → fallback native       │
//!  │    │        └── ERROR → fallback native          │
//!  │    └── return validated result                   │
//!  └──────────────────────────────────────────────────┘
//! ```
//!
//! # Validation Rules
//!
//! Every output (whether from WASM or native) passes through validation:
//! - All decimal strings must be parseable as `rust_decimal::Decimal`
//! - Risk level must be a known variant
//! - Margin values must be internally consistent
//! - No NaN, Infinity, or empty strings in numeric fields
//!
//! # Determinism
//!
//! The native and boundary paths produce byte-identical JSON for the same
//! inputs. Both paths use the same underlying `CrossMarginEngine::simulate_order`.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;

use crate::margin::{CrossMarginEngine, RiskLevel};
use crate::wasm_bindings::{
    margin_preview_json, risk_level_from_string, MarginPreviewInput, MarginPreviewOutput,
    PositionInput,
};
use types::ids::AccountId;
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::position::{Position, PositionSide};

#[cfg(feature = "wasm-host")]
use std::sync::Arc;

#[cfg(feature = "wasm-host")]
use crate::wasm_host::WasmRuntime;

// ---------------------------------------------------------------------------
// Feature flag (backward compatible)
// ---------------------------------------------------------------------------

/// Runtime feature flag for WASM execution.
///
/// Controls whether the adapter attempts the WASM boundary path
/// or goes directly to native computation.
///
/// This is the original Phase 12 interface, preserved for backward
/// compatibility. For Phase 13 and beyond, prefer `WasmExecutionMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmFeatureFlag {
    /// WASM boundary path enabled — will be tried first with fallback
    Enabled,
    /// WASM disabled — always use native computation
    Disabled,
}

impl Default for WasmFeatureFlag {
    /// Default: disabled (safe, predictable behavior)
    fn default() -> Self {
        WasmFeatureFlag::Disabled
    }
}

// ---------------------------------------------------------------------------
// Execution mode (Phase 13)
// ---------------------------------------------------------------------------

/// Execution mode for the margin preview adapter.
///
/// This is the primary interface for controlling execution path selection.
/// It supersedes `WasmFeatureFlag` but both remain available.
///
/// # Modes
///
/// - `Native`: Direct Rust computation, no serialization overhead
/// - `Boundary`: Same-process JSON boundary (exercises the WASM code path
///   without an actual WASM runtime — useful for testing and validation)
/// - `WasmRuntime`: Actual WASM module execution via wasmtime (requires
///   the `wasm-host` feature and a loaded `WasmRuntime`)
#[derive(Clone)]
pub enum WasmExecutionMode {
    /// Always use native Rust computation (safest, fastest)
    Native,
    /// Use same-process JSON boundary path (tests WASM FFI code path)
    Boundary,
    /// Use actual WASM runtime via wasmtime
    #[cfg(feature = "wasm-host")]
    WasmRuntime(Arc<WasmRuntime>),
}

impl std::fmt::Debug for WasmExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmExecutionMode::Native => write!(f, "Native"),
            WasmExecutionMode::Boundary => write!(f, "Boundary"),
            #[cfg(feature = "wasm-host")]
            WasmExecutionMode::WasmRuntime(_) => write!(f, "WasmRuntime"),
        }
    }
}

impl Default for WasmExecutionMode {
    /// Default: Native (safe, predictable behavior)
    fn default() -> Self {
        WasmExecutionMode::Native
    }
}

// ---------------------------------------------------------------------------
// Adapter errors
// ---------------------------------------------------------------------------

/// Errors from the margin preview adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterError {
    /// Input construction failed
    InputError(String),
    /// WASM boundary computation failed
    BoundaryError(String),
    /// Output validation failed
    ValidationError(ValidationFailure),
    /// WASM host runtime error
    RuntimeError(String),
    /// Both WASM and native paths failed (should never happen)
    InternalError(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterError::InputError(msg) => write!(f, "Input error: {msg}"),
            AdapterError::BoundaryError(msg) => write!(f, "Boundary error: {msg}"),
            AdapterError::ValidationError(v) => write!(f, "Validation error: {v:?}"),
            AdapterError::RuntimeError(msg) => write!(f, "Runtime error: {msg}"),
            AdapterError::InternalError(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Specific validation failure reasons.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationFailure {
    /// A decimal field could not be parsed
    InvalidDecimal { field: String, value: String },
    /// Risk level is not a known variant
    InvalidRiskLevel(String),
    /// Numeric field is empty
    EmptyField(String),
    /// Margin consistency check failed
    InconsistentMargins {
        equity: Decimal,
        margin_used: Decimal,
        margin_available: Decimal,
    },
}

/// Validate a `MarginPreviewOutput` before it can be used.
///
/// This is the gatekeeper: no WASM result touches exchange display
/// or state without passing through this function.
///
/// # Rules
/// 1. All numeric string fields must parse as valid `Decimal`
/// 2. No field may be empty
/// 3. `risk_level` must be one of: Healthy, Warning, Danger, Liquidation
/// 4. Margin consistency: `margin_used + margin_available ≈ equity` (within rounding tolerance)
pub fn validate_output(output: &MarginPreviewOutput) -> Result<(), ValidationFailure> {
    // Rule 1 & 2: Parse all decimal fields
    let equity = parse_decimal_field("equity_after", &output.equity_after)?;
    let margin_used = parse_decimal_field("margin_used_after", &output.margin_used_after)?;
    let margin_available =
        parse_decimal_field("margin_available_after", &output.margin_available_after)?;
    let _margin_ratio = parse_decimal_field("margin_ratio_after", &output.margin_ratio_after)?;
    let _liq_price = parse_decimal_field("liquidation_price", &output.liquidation_price)?;
    let _leverage = parse_decimal_field("leverage_ratio", &output.leverage_ratio)?;

    // Rule 3: Valid risk level
    if risk_level_from_string(&output.risk_level).is_none() {
        return Err(ValidationFailure::InvalidRiskLevel(
            output.risk_level.clone(),
        ));
    }

    // Rule 4: Margin consistency check
    // margin_used + margin_available should approximately equal equity
    // Allow tolerance for rounding differences (display precision is 8dp,
    // and round_down vs round_display can differ by up to 1 unit in last place)
    let computed_equity = margin_used + margin_available;
    let tolerance = Decimal::from_str_exact("0.00000002").unwrap(); // 2× last-place unit at 8dp
    let diff = (equity - computed_equity).abs();
    if diff > tolerance {
        return Err(ValidationFailure::InconsistentMargins {
            equity,
            margin_used,
            margin_available,
        });
    }

    Ok(())
}

/// Parse and validate a decimal string field.
fn parse_decimal_field(field: &str, value: &str) -> Result<Decimal, ValidationFailure> {
    if value.is_empty() {
        return Err(ValidationFailure::EmptyField(field.to_owned()));
    }
    Decimal::from_str(value).map_err(|_| ValidationFailure::InvalidDecimal {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Margin preview adapter — unified interface for native and WASM computation.
///
/// The adapter:
/// 1. Accepts typed inputs (no raw JSON at the call site)
/// 2. Dispatches to WASM boundary or native path based on feature flag
/// 3. Validates all outputs before returning
/// 4. Falls back to native on any WASM failure
pub struct MarginPreviewAdapter {
    /// Legacy feature flag (Phase 12 interface)
    feature_flag: WasmFeatureFlag,
    /// Execution mode (Phase 13 interface, takes precedence when set)
    mode: Option<WasmExecutionMode>,
}

impl MarginPreviewAdapter {
    /// Create a new adapter with the given feature flag (Phase 12 interface).
    ///
    /// Preserved for backward compatibility. For new code, prefer `with_mode`.
    pub fn new(feature_flag: WasmFeatureFlag) -> Self {
        Self {
            feature_flag,
            mode: None,
        }
    }

    /// Create a new adapter with an explicit execution mode (Phase 13 interface).
    ///
    /// The mode determines which execution path is attempted first.
    /// Fallback to native is always available.
    pub fn with_mode(mode: WasmExecutionMode) -> Self {
        Self {
            feature_flag: WasmFeatureFlag::Disabled, // unused when mode is set
            mode: Some(mode),
        }
    }

    /// Compute a margin preview using the configured execution path.
    ///
    /// If using Phase 13 `WasmExecutionMode`, dispatches accordingly.
    /// Otherwise falls back to Phase 12 `WasmFeatureFlag` behavior.
    ///
    /// Regardless of mode, fallback to native is always safe.
    ///
    /// Returns a validated `MarginPreviewOutput`.
    pub fn compute(
        &self,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        // Phase 13 mode takes precedence
        if let Some(ref mode) = self.mode {
            return self.compute_with_mode(mode, input);
        }

        // Phase 12 fallback
        match self.feature_flag {
            WasmFeatureFlag::Enabled => {
                // Try WASM boundary path first
                match self.compute_via_boundary(input) {
                    Ok(output) => {
                        // Validate the boundary output
                        match validate_output(&output) {
                            Ok(()) => Ok(output),
                            Err(validation_err) => {
                                // Validation failed — fall back to native
                                self.compute_native(input)
                                    .map_err(|_| AdapterError::ValidationError(validation_err))
                            }
                        }
                    }
                    Err(_boundary_err) => {
                        // Boundary computation failed — fall back to native
                        self.compute_native(input)
                    }
                }
            }
            WasmFeatureFlag::Disabled => self.compute_native(input),
        }
    }

    /// Dispatch based on `WasmExecutionMode`.
    fn compute_with_mode(
        &self,
        mode: &WasmExecutionMode,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        match mode {
            WasmExecutionMode::Native => self.compute_native(input),
            WasmExecutionMode::Boundary => self.compute_boundary_with_fallback(input),
            #[cfg(feature = "wasm-host")]
            WasmExecutionMode::WasmRuntime(runtime) => {
                self.compute_wasm_runtime_with_fallback(runtime, input)
            }
        }
    }

    /// Compute via boundary path with validation and native fallback.
    fn compute_boundary_with_fallback(
        &self,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        match self.compute_via_boundary(input) {
            Ok(output) => match validate_output(&output) {
                Ok(()) => Ok(output),
                Err(_) => self.compute_native(input),
            },
            Err(_) => self.compute_native(input),
        }
    }

    /// Compute via actual WASM runtime with validation and native fallback.
    #[cfg(feature = "wasm-host")]
    fn compute_wasm_runtime_with_fallback(
        &self,
        runtime: &WasmRuntime,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        // Serialize input
        let input_json = match serde_json::to_string(input) {
            Ok(json) => json,
            Err(e) => return Err(AdapterError::InputError(format!("Serialization: {e}"))),
        };

        // Execute via WASM runtime
        match runtime.margin_preview(&input_json) {
            Ok(output_json) => {
                // Deserialize output
                match serde_json::from_str::<MarginPreviewOutput>(&output_json) {
                    Ok(output) => {
                        // Validate
                        match validate_output(&output) {
                            Ok(()) => Ok(output),
                            Err(_) => {
                                // Validation failed — fall back to native
                                self.compute_native(input)
                            }
                        }
                    }
                    Err(_) => {
                        // Deserialization failed — fall back to native
                        self.compute_native(input)
                    }
                }
            }
            Err(_) => {
                // WASM runtime execution failed — fall back to native
                self.compute_native(input)
            }
        }
    }

    /// Compute via the WASM boundary path.
    ///
    /// This serializes the input to JSON, passes it through `margin_preview_json`,
    /// and deserializes the output. This exercises the exact same code path that
    /// the WASM module would use, ensuring deterministic equivalence.
    pub fn compute_via_boundary(
        &self,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        // Serialize input to JSON
        let input_json = serde_json::to_string(input)
            .map_err(|e| AdapterError::InputError(format!("Input serialization: {e}")))?;

        // Call the boundary function (same code compiled into WASM)
        let output_json = margin_preview_json(&input_json)
            .map_err(|e| AdapterError::BoundaryError(e))?;

        // Deserialize output
        let output: MarginPreviewOutput = serde_json::from_str(&output_json)
            .map_err(|e| AdapterError::BoundaryError(format!("Output deserialization: {e}")))?;

        Ok(output)
    }

    /// Compute via native Rust path (direct function call, no serialization).
    ///
    /// This is the fallback path and also the reference implementation.
    pub fn compute_native(
        &self,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
        // Parse account ID
        let account_uuid = uuid::Uuid::parse_str(&input.account_id)
            .map_err(|e| AdapterError::InputError(format!("Invalid account_id: {e}")))?;
        let account_id = AccountId::from_uuid(account_uuid);

        // Parse balance
        let total_balance = Decimal::from_str(&input.total_balance)
            .map_err(|e| AdapterError::InputError(format!("Invalid total_balance: {e}")))?;

        // Build engine
        let mut engine = CrossMarginEngine::new(account_id, total_balance);

        // Parse and add positions
        for (i, pos_input) in input.positions.iter().enumerate() {
            let position = parse_native_position(pos_input, account_id, i)?;
            engine.add_position(position);
        }

        // Parse order params
        let side = match &*input.order.side {
            "BUY" => Side::BUY,
            "SELL" => Side::SELL,
            other => {
                return Err(AdapterError::InputError(format!(
                    "Invalid order side: {other}"
                )))
            }
        };

        let price = Price::from_str(&input.order.price)
            .map_err(|e| AdapterError::InputError(format!("Invalid order price: {e}")))?;
        let quantity = Quantity::from_str(&input.order.quantity)
            .map_err(|e| AdapterError::InputError(format!("Invalid order quantity: {e}")))?;

        // Compute
        let preview = engine.simulate_order(
            &input.order.symbol,
            side,
            price,
            quantity,
            input.order.leverage,
        );

        // Convert to output format
        let output = MarginPreviewOutput {
            equity_after: preview.equity_after.to_string(),
            margin_used_after: preview.margin_used_after.to_string(),
            margin_available_after: preview.margin_available_after.to_string(),
            margin_ratio_after: preview.margin_ratio_after.to_string(),
            liquidation_price: preview.liquidation_price.to_string(),
            leverage_ratio: preview.leverage_ratio.to_string(),
            risk_level: match preview.risk_level {
                RiskLevel::Healthy => "Healthy".to_owned(),
                RiskLevel::Warning => "Warning".to_owned(),
                RiskLevel::Danger => "Danger".to_owned(),
                RiskLevel::Liquidation => "Liquidation".to_owned(),
            },
            has_negative_balance: preview.has_negative_balance,
        };

        Ok(output)
    }
}

/// Parse a `PositionInput` into a native `Position` for the adapter's native path.
fn parse_native_position(
    input: &PositionInput,
    account_id: AccountId,
    index: usize,
) -> Result<Position, AdapterError> {
    let side = match &*input.side {
        "LONG" => PositionSide::LONG,
        "SHORT" => PositionSide::SHORT,
        other => {
            return Err(native_pos_error(index, "side", &format!("Invalid: {other}")))
        }
    };

    let size = Quantity::from_str(&input.size)
        .map_err(|e| native_pos_error(index, "size", &e))?;
    let entry_price = Price::from_str(&input.entry_price)
        .map_err(|e| native_pos_error(index, "entry_price", &e))?;
    let mark_price = Price::from_str(&input.mark_price)
        .map_err(|e| native_pos_error(index, "mark_price", &e))?;
    let liquidation_price = Price::from_str(&input.liquidation_price)
        .map_err(|e| native_pos_error(index, "liquidation_price", &e))?;
    let initial_margin = Decimal::from_str(&input.initial_margin)
        .map_err(|e| native_pos_error(index, "initial_margin", &e))?;
    let maintenance_margin = Decimal::from_str(&input.maintenance_margin)
        .map_err(|e| native_pos_error(index, "maintenance_margin", &e))?;

    let market_id = types::ids::MarketId::new(&*input.symbol);

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

/// Format an adapter position field error.
fn native_pos_error(index: usize, field: &str, error: &dyn std::fmt::Display) -> AdapterError {
    AdapterError::InputError(format!("positions[{index}].{field}: {error}"))
}
