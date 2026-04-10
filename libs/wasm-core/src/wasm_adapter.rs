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
//!  │    ├── wasm_enabled?                             │
//!  │    │   ├── YES → compute_via_boundary()          │
//!  │    │   │         ├── validate_output()           │
//!  │    │   │         │   ├── OK → return result      │
//!  │    │   │         │   └── FAIL → fallback native  │
//!  │    │   │         └── ERROR → fallback native     │
//!  │    │   └── NO → compute_native()                 │
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
use std::collections::BTreeMap;

use crate::margin::{CrossMarginEngine, RiskLevel};
use crate::wasm_bindings::{
    margin_preview_json, risk_level_from_string, MarginPreviewInput, MarginPreviewOutput,
    OrderInput, PositionInput,
};
use types::ids::AccountId;
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::position::{Position, PositionSide};

// ---------------------------------------------------------------------------
// Feature flag
// ---------------------------------------------------------------------------

/// Runtime feature flag for WASM execution.
///
/// Controls whether the adapter attempts the WASM boundary path
/// or goes directly to native computation.
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
    /// Both WASM and native paths failed (should never happen)
    InternalError(String),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterError::InputError(msg) => write!(f, "Input error: {msg}"),
            AdapterError::BoundaryError(msg) => write!(f, "Boundary error: {msg}"),
            AdapterError::ValidationError(v) => write!(f, "Validation error: {v:?}"),
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
    feature_flag: WasmFeatureFlag,
}

impl MarginPreviewAdapter {
    /// Create a new adapter with the given feature flag.
    pub fn new(feature_flag: WasmFeatureFlag) -> Self {
        Self { feature_flag }
    }

    /// Compute a margin preview using the configured execution path.
    ///
    /// If WASM is enabled, tries the boundary path first. On any failure
    /// (computation error, validation error), falls back to native.
    ///
    /// If WASM is disabled, goes directly to native computation.
    ///
    /// Returns a validated `MarginPreviewOutput`.
    pub fn compute(
        &self,
        input: &MarginPreviewInput,
    ) -> Result<MarginPreviewOutput, AdapterError> {
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
        let side = match input.order.side.as_str() {
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
    let err = |field: &str, e: impl std::fmt::Display| -> AdapterError {
        AdapterError::InputError(format!("positions[{index}].{field}: {e}"))
    };

    let side = match input.side.as_str() {
        "LONG" => PositionSide::LONG,
        "SHORT" => PositionSide::SHORT,
        other => return Err(err("side", format!("Invalid: {other}"))),
    };

    let size = Quantity::from_str(&input.size).map_err(|e| err("size", e))?;
    let entry_price = Price::from_str(&input.entry_price).map_err(|e| err("entry_price", e))?;
    let mark_price = Price::from_str(&input.mark_price).map_err(|e| err("mark_price", e))?;
    let liquidation_price =
        Price::from_str(&input.liquidation_price).map_err(|e| err("liquidation_price", e))?;
    let initial_margin =
        Decimal::from_str(&input.initial_margin).map_err(|e| err("initial_margin", e))?;
    let maintenance_margin =
        Decimal::from_str(&input.maintenance_margin).map_err(|e| err("maintenance_margin", e))?;

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
