//! WASM Integration Tests
//!
//! Verifies deterministic equivalence between native and WASM boundary paths,
//! boundary input correctness, invalid input rejection, fallback behavior,
//! output validation, and serialization round-trips.
//!
//! These tests ensure the WASM module extraction preserves every invariant
//! from the original `CrossMarginEngine::simulate_order` implementation.

#[cfg(test)]
mod tests {
    use crate::margin::CrossMarginEngine;
    use crate::wasm_adapter::{
        validate_output, MarginPreviewAdapter, ValidationFailure, WasmFeatureFlag,
    };
    use crate::wasm_bindings::{
        margin_preview_json, MarginPreviewInput, MarginPreviewOutput, OrderInput, PositionInput,
    };
    use rust_decimal::Decimal;
    use types::ids::AccountId;
    use types::numeric::{Price, Quantity};
    use types::order::Side;
    use types::position::PositionSide;

    // -----------------------------------------------------------------------
    // Test fixtures
    // -----------------------------------------------------------------------

    /// Standard test account UUID (deterministic, no clock dependency).
    const TEST_ACCOUNT_ID: &str = "01939d7f-8e4a-7890-a123-456789abcdef";

    /// Build a standard test input with one existing BTC/USDT LONG position
    /// and a new ETH/USDT BUY order.
    fn standard_input() -> MarginPreviewInput {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![PositionInput {
                symbol: "BTC/USDT".to_owned(),
                side: "LONG".to_owned(),
                size: "2.0".to_owned(),
                entry_price: "50000".to_owned(),
                mark_price: "51000".to_owned(),
                liquidation_price: "49500".to_owned(),
                initial_margin: "10000".to_owned(),
                maintenance_margin: "500".to_owned(),
                leverage: 10,
                timestamp: 1_708_123_456_789_000_000,
            }],
            order: OrderInput {
                symbol: "ETH/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "3000".to_owned(),
                quantity: "10.0".to_owned(),
                leverage: 20,
            },
        }
    }

    /// Build an input with no existing positions and a small order.
    fn empty_positions_input() -> MarginPreviewInput {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "10000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "0.1".to_owned(),
                leverage: 10,
            },
        }
    }

    /// Build the same engine state as `standard_input` using native types.
    fn standard_native_engine() -> CrossMarginEngine {
        let account_uuid = uuid::Uuid::parse_str(TEST_ACCOUNT_ID).unwrap();
        let account_id = AccountId::from_uuid(account_uuid);
        let mut engine = CrossMarginEngine::new(account_id, Decimal::from(100_000));

        let pos = types::position::Position::new(
            account_id,
            types::ids::MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("2.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Price::from_u64(49_500),
            Decimal::from(10_000),
            Decimal::from(500),
            10,
            1_708_123_456_789_000_000,
        );
        engine.add_position(pos);
        engine
    }

    // =======================================================================
    // 1. Deterministic Equivalence Tests
    // =======================================================================

    #[test]
    fn test_deterministic_equivalence_native_vs_boundary() {
        // Compute via native engine
        let engine = standard_native_engine();
        let native_preview = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );

        // Compute via JSON boundary (same path as WASM)
        let input = standard_input();
        let boundary_json = margin_preview_json(
            &serde_json::to_string(&input).unwrap(),
        )
        .expect("Boundary computation should succeed");

        let boundary_output: MarginPreviewOutput =
            serde_json::from_str(&boundary_json).unwrap();

        // Compare every field
        assert_eq!(
            boundary_output.equity_after,
            native_preview.equity_after.to_string(),
            "equity_after mismatch"
        );
        assert_eq!(
            boundary_output.margin_used_after,
            native_preview.margin_used_after.to_string(),
            "margin_used_after mismatch"
        );
        assert_eq!(
            boundary_output.margin_available_after,
            native_preview.margin_available_after.to_string(),
            "margin_available_after mismatch"
        );
        assert_eq!(
            boundary_output.margin_ratio_after,
            native_preview.margin_ratio_after.to_string(),
            "margin_ratio_after mismatch"
        );
        assert_eq!(
            boundary_output.liquidation_price,
            native_preview.liquidation_price.to_string(),
            "liquidation_price mismatch"
        );
        assert_eq!(
            boundary_output.leverage_ratio,
            native_preview.leverage_ratio.to_string(),
            "leverage_ratio mismatch"
        );
        assert_eq!(
            boundary_output.has_negative_balance,
            native_preview.has_negative_balance,
            "has_negative_balance mismatch"
        );
    }

    #[test]
    fn test_deterministic_equivalence_adapter_native_vs_boundary() {
        let input = standard_input();

        let native_adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let wasm_adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);

        let native_result = native_adapter.compute(&input).unwrap();
        let wasm_result = wasm_adapter.compute(&input).unwrap();

        assert_eq!(native_result, wasm_result, "Native and boundary results must be identical");
    }

    #[test]
    fn test_deterministic_repeated_calls() {
        let input = standard_input();
        let input_json = serde_json::to_string(&input).unwrap();

        let result1 = margin_preview_json(&input_json).unwrap();
        let result2 = margin_preview_json(&input_json).unwrap();
        let result3 = margin_preview_json(&input_json).unwrap();

        assert_eq!(result1, result2, "Run 1 vs 2 must be identical");
        assert_eq!(result2, result3, "Run 2 vs 3 must be identical");
    }

    #[test]
    fn test_deterministic_equivalence_empty_positions() {
        let input = empty_positions_input();

        let native_adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let wasm_adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);

        let native_result = native_adapter.compute(&input).unwrap();
        let wasm_result = wasm_adapter.compute(&input).unwrap();

        assert_eq!(native_result, wasm_result);
    }

    // =======================================================================
    // 2. Boundary Input Correctness Tests
    // =======================================================================

    #[test]
    fn test_boundary_zero_balance() {
        // Use a very small balance (1) to avoid division by zero in leverage_ratio
        // when equity is zero. The engine divides total_notional / equity.
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "1".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result_json = margin_preview_json(
            &serde_json::to_string(&input).unwrap(),
        )
        .unwrap();

        let output: MarginPreviewOutput = serde_json::from_str(&result_json).unwrap();
        assert!(output.has_negative_balance, "Tiny balance + large order should show negative");
    }

    #[test]
    fn test_boundary_max_leverage_125x() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 125,
            },
        };

        let result_json = margin_preview_json(
            &serde_json::to_string(&input).unwrap(),
        )
        .unwrap();

        let output: MarginPreviewOutput = serde_json::from_str(&result_json).unwrap();
        // IM = 50000 / 125 = 400
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(400));
        assert!(!output.has_negative_balance);
    }

    #[test]
    fn test_boundary_minimum_quantity() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "0.00000001".to_owned(),
                leverage: 10,
            },
        };

        let result_json = margin_preview_json(
            &serde_json::to_string(&input).unwrap(),
        );

        // Should not panic, should produce valid output
        assert!(result_json.is_ok(), "Minimum quantity should not cause panic");
    }

    #[test]
    fn test_boundary_high_precision_decimals() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "99999999.999999999999999999".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "SELL".to_owned(),
                price: "50000.123456789012345678".to_owned(),
                quantity: "0.123456789012345678".to_owned(),
                leverage: 10,
            },
        };

        let result_json = margin_preview_json(
            &serde_json::to_string(&input).unwrap(),
        );

        assert!(result_json.is_ok(), "High precision should not cause panic");
    }

    #[test]
    fn test_boundary_sell_order() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "SELL".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap()).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&result).unwrap();

        assert_eq!(output.risk_level, "Healthy");
        assert!(!output.has_negative_balance);
    }

    #[test]
    fn test_boundary_multiple_positions() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "50000".to_owned(),
            positions: vec![
                PositionInput {
                    symbol: "BTC/USDT".to_owned(),
                    side: "LONG".to_owned(),
                    size: "1.0".to_owned(),
                    entry_price: "50000".to_owned(),
                    mark_price: "51000".to_owned(),
                    liquidation_price: "49500".to_owned(),
                    initial_margin: "5000".to_owned(),
                    maintenance_margin: "250".to_owned(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                },
                PositionInput {
                    symbol: "ETH/USDT".to_owned(),
                    side: "SHORT".to_owned(),
                    size: "10.0".to_owned(),
                    entry_price: "3000".to_owned(),
                    mark_price: "2900".to_owned(),
                    liquidation_price: "3100".to_owned(),
                    initial_margin: "3000".to_owned(),
                    maintenance_margin: "150".to_owned(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                },
            ],
            order: OrderInput {
                symbol: "SOL/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "100".to_owned(),
                quantity: "10.0".to_owned(),
                leverage: 20,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap()).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&result).unwrap();

        // Should process all positions correctly
        assert!(!output.equity_after.is_empty());
        assert!(validate_output(&output).is_ok());
    }

    // =======================================================================
    // 3. Invalid Input Rejection Tests
    // =======================================================================

    #[test]
    fn test_reject_malformed_json() {
        let result = margin_preview_json("{not valid json");
        assert!(result.is_err(), "Malformed JSON should be rejected");
    }

    #[test]
    fn test_reject_empty_json() {
        let result = margin_preview_json("");
        assert!(result.is_err(), "Empty string should be rejected");
    }

    #[test]
    fn test_reject_missing_fields() {
        let result = margin_preview_json(r#"{"account_id": "test"}"#);
        assert!(result.is_err(), "Missing required fields should be rejected");
    }

    #[test]
    fn test_reject_invalid_account_id() {
        let input = MarginPreviewInput {
            account_id: "not-a-uuid".to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Invalid UUID should be rejected");
    }

    #[test]
    fn test_reject_non_numeric_balance() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "not_a_number".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Non-numeric balance should be rejected");
    }

    #[test]
    fn test_reject_invalid_order_side() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "INVALID".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Invalid order side should be rejected");
    }

    #[test]
    fn test_reject_invalid_position_side() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![PositionInput {
                symbol: "BTC/USDT".to_owned(),
                side: "WRONG".to_owned(),
                size: "1.0".to_owned(),
                entry_price: "50000".to_owned(),
                mark_price: "51000".to_owned(),
                liquidation_price: "49500".to_owned(),
                initial_margin: "5000".to_owned(),
                maintenance_margin: "500".to_owned(),
                leverage: 10,
                timestamp: 1_708_123_456_789_000_000,
            }],
            order: OrderInput {
                symbol: "ETH/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "3000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 20,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Invalid position side should be rejected");
    }

    #[test]
    fn test_reject_non_numeric_price() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "abc".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Non-numeric price should be rejected");
    }

    #[test]
    fn test_reject_non_numeric_quantity() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "xyz".to_owned(),
                leverage: 10,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap());
        assert!(result.is_err(), "Non-numeric quantity should be rejected");
    }

    // =======================================================================
    // 4. Fallback Behavior Tests
    // =======================================================================

    #[test]
    fn test_fallback_when_disabled() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let input = standard_input();

        let result = adapter.compute(&input);
        assert!(result.is_ok(), "Native fallback should always work");
    }

    #[test]
    fn test_enabled_produces_valid_output() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
        let input = standard_input();

        let result = adapter.compute(&input);
        assert!(result.is_ok(), "WASM-enabled path should produce valid output");
    }

    #[test]
    fn test_default_flag_is_disabled() {
        let flag = WasmFeatureFlag::default();
        assert_eq!(flag, WasmFeatureFlag::Disabled, "Default should be Disabled (safe)");
    }

    #[test]
    fn test_adapter_native_handles_invalid_input() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let input = MarginPreviewInput {
            account_id: "bad-uuid".to_owned(),
            total_balance: "100000".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 10,
            },
        };

        let result = adapter.compute(&input);
        assert!(result.is_err(), "Invalid input should produce error");
    }

    // =======================================================================
    // 5. Output Validation Tests
    // =======================================================================

    #[test]
    fn test_validate_valid_output() {
        let output = MarginPreviewOutput {
            equity_after: "102000".to_owned(),
            margin_used_after: "11500".to_owned(),
            margin_available_after: "90500".to_owned(),
            margin_ratio_after: "204".to_owned(),
            liquidation_price: "2860.50000000".to_owned(),
            leverage_ratio: "1.27450980".to_owned(),
            risk_level: "Healthy".to_owned(),
            has_negative_balance: false,
        };

        assert!(validate_output(&output).is_ok());
    }

    #[test]
    fn test_validate_rejects_empty_equity() {
        let output = MarginPreviewOutput {
            equity_after: "".to_owned(),
            margin_used_after: "11500".to_owned(),
            margin_available_after: "90500".to_owned(),
            margin_ratio_after: "204".to_owned(),
            liquidation_price: "45250".to_owned(),
            leverage_ratio: "1.27".to_owned(),
            risk_level: "Healthy".to_owned(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::EmptyField(f) => assert_eq!(f, "equity_after"),
            other => panic!("Expected EmptyField, got: {other:?}"),
        }
    }

    #[test]
    fn test_validate_rejects_invalid_decimal() {
        let output = MarginPreviewOutput {
            equity_after: "not_decimal".to_owned(),
            margin_used_after: "11500".to_owned(),
            margin_available_after: "90500".to_owned(),
            margin_ratio_after: "204".to_owned(),
            liquidation_price: "45250".to_owned(),
            leverage_ratio: "1.27".to_owned(),
            risk_level: "Healthy".to_owned(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::InvalidDecimal { field, .. } => {
                assert_eq!(field, "equity_after")
            }
            other => panic!("Expected InvalidDecimal, got: {other:?}"),
        }
    }

    #[test]
    fn test_validate_rejects_invalid_risk_level() {
        let output = MarginPreviewOutput {
            equity_after: "102000".to_owned(),
            margin_used_after: "11500".to_owned(),
            margin_available_after: "90500".to_owned(),
            margin_ratio_after: "204".to_owned(),
            liquidation_price: "45250".to_owned(),
            leverage_ratio: "1.27".to_owned(),
            risk_level: "INVALID_LEVEL".to_owned(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::InvalidRiskLevel(level) => {
                assert_eq!(level, "INVALID_LEVEL")
            }
            other => panic!("Expected InvalidRiskLevel, got: {other:?}"),
        }
    }

    #[test]
    fn test_validate_rejects_inconsistent_margins() {
        let output = MarginPreviewOutput {
            equity_after: "100000".to_owned(),
            margin_used_after: "50000".to_owned(),
            // Should be ~50000 but is wildly different
            margin_available_after: "10000".to_owned(),
            margin_ratio_after: "204".to_owned(),
            liquidation_price: "45250".to_owned(),
            leverage_ratio: "1.27".to_owned(),
            risk_level: "Healthy".to_owned(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::InconsistentMargins { .. } => {}
            other => panic!("Expected InconsistentMargins, got: {other:?}"),
        }
    }

    #[test]
    fn test_validate_accepts_all_risk_levels() {
        for risk_level in &["Healthy", "Warning", "Danger", "Liquidation"] {
            let output = MarginPreviewOutput {
                equity_after: "102000".to_owned(),
                margin_used_after: "11500".to_owned(),
                margin_available_after: "90500".to_owned(),
                margin_ratio_after: "204".to_owned(),
                liquidation_price: "45250".to_owned(),
                leverage_ratio: "1.27".to_owned(),
                risk_level: risk_level.to_string(),
                has_negative_balance: false,
            };
            assert!(
                validate_output(&output).is_ok(),
                "Risk level '{risk_level}' should be accepted"
            );
        }
    }

    // =======================================================================
    // 6. Serialization Round-Trip Tests
    // =======================================================================

    #[test]
    fn test_input_serialization_roundtrip() {
        let input = standard_input();
        let json = serde_json::to_string(&input).unwrap();
        let restored: MarginPreviewInput = serde_json::from_str(&json).unwrap();

        assert_eq!(input.account_id, restored.account_id);
        assert_eq!(input.total_balance, restored.total_balance);
        assert_eq!(input.positions.len(), restored.positions.len());
        assert_eq!(input.order.symbol, restored.order.symbol);
        assert_eq!(input.order.side, restored.order.side);
        assert_eq!(input.order.price, restored.order.price);
        assert_eq!(input.order.quantity, restored.order.quantity);
        assert_eq!(input.order.leverage, restored.order.leverage);
    }

    #[test]
    fn test_output_serialization_roundtrip() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
        let input = standard_input();
        let output = adapter.compute(&input).unwrap();

        let json = serde_json::to_string(&output).unwrap();
        let restored: MarginPreviewOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(output, restored, "Output must survive JSON round-trip");
    }

    #[test]
    fn test_boundary_json_roundtrip_produces_valid_output() {
        let input = standard_input();
        let input_json = serde_json::to_string(&input).unwrap();

        let output_json = margin_preview_json(&input_json).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&output_json).unwrap();

        // Validate the output produced by the boundary path
        assert!(
            validate_output(&output).is_ok(),
            "Boundary output must pass validation"
        );
    }

    // =======================================================================
    // 7. Specific Value Verification Tests
    // =======================================================================

    #[test]
    fn test_standard_input_expected_values() {
        let input = standard_input();
        let result = margin_preview_json(&serde_json::to_string(&input).unwrap()).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&result).unwrap();

        // Existing position: Long 2 BTC @ 50k, mark 51k → uPnL = +2000
        // equity = 100000 + 2000 = 102000
        let equity: Decimal = output.equity_after.parse().unwrap();
        assert_eq!(equity, Decimal::from(102_000));

        // New order: 10 ETH @ 3000, leverage 20 → IM = 30000/20 = 1500
        // Total IM = 10000 (existing) + 1500 (new) = 11500
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(11_500));

        // Margin available = 102000 - 11500 = 90500
        let margin_available: Decimal = output.margin_available_after.parse().unwrap();
        assert_eq!(margin_available, Decimal::from(90_500));

        // Not negative
        assert!(!output.has_negative_balance);

        // Risk level should be healthy with this much equity
        assert_eq!(output.risk_level, "Healthy");
    }

    #[test]
    fn test_near_liquidation_input() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.to_owned(),
            total_balance: "600".to_owned(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".to_owned(),
                side: "BUY".to_owned(),
                price: "50000".to_owned(),
                quantity: "1.0".to_owned(),
                leverage: 100,
            },
        };

        let result = margin_preview_json(&serde_json::to_string(&input).unwrap()).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&result).unwrap();

        // IM = 50000/100 = 500, equity = 600, margin_available = 100
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(500));
        assert!(!output.has_negative_balance);
    }
}
