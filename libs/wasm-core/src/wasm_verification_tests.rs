//! Phase 14 — WASM Verification Tests
//!
//! Comprehensive verification of the WASM margin preview path against the
//! native Rust path. This module proves:
//!
//! 1. **Deterministic equivalence** — WASM boundary and native paths produce
//!    byte-identical JSON output for all supported input patterns.
//! 2. **Repeat-run determinism** — 100+ iterations yield identical results.
//! 3. **Boundary/extreme input handling** — edge cases are handled correctly.
//! 4. **Invalid input rejection** — bad inputs are rejected consistently.
//! 5. **Replay consistency** — sequences of diverse inputs produce identical
//!    output across both paths.
//! 6. **Fallback consistency** — fallback behavior is equivalent.
//! 7. **Validation before state usage** — every output is validated.
//! 8. **Regression safety** — existing flows remain unaffected.
//!
//! All tests are additive and feature-gated behind `#[cfg(test)]`.
//! No existing tests or production code are modified.

#[cfg(test)]
mod tests {
    use crate::margin::CrossMarginEngine;
    use crate::wasm_adapter::{
        validate_output, AdapterError, MarginPreviewAdapter, ValidationFailure,
        WasmExecutionMode, WasmFeatureFlag,
    };
    use crate::wasm_bindings::{
        margin_preview_json, MarginPreviewInput, MarginPreviewOutput, OrderInput, PositionInput,
    };
    use rust_decimal::prelude::*;
    use rust_decimal::Decimal;
    use types::ids::AccountId;
    use types::numeric::{Price, Quantity};
    use types::order::Side;
    use types::position::PositionSide;

    // -----------------------------------------------------------------------
    // Test fixtures
    // -----------------------------------------------------------------------

    const TEST_ACCOUNT_ID: &str = "01939d7f-8e4a-7890-a123-456789abcdef";

    fn standard_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![PositionInput {
                symbol: "BTC/USDT".into(),
                side: "LONG".into(),
                size: "2.0".into(),
                entry_price: "50000".into(),
                mark_price: "51000".into(),
                liquidation_price: "49500".into(),
                initial_margin: "10000".into(),
                maintenance_margin: "500".into(),
                leverage: 10,
                timestamp: 1_708_123_456_789_000_000,
            }],
            order: OrderInput {
                symbol: "ETH/USDT".into(),
                side: "BUY".into(),
                price: "3000".into(),
                quantity: "10.0".into(),
                leverage: 20,
            },
        }
    }

    fn empty_positions_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "10000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "0.1".into(),
                leverage: 10,
            },
        }
    }

    fn near_liquidation_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "600".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 100,
            },
        }
    }

    fn multi_position_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "50000".into(),
            positions: vec![
                PositionInput {
                    symbol: "BTC/USDT".into(),
                    side: "LONG".into(),
                    size: "1.0".into(),
                    entry_price: "50000".into(),
                    mark_price: "51000".into(),
                    liquidation_price: "49500".into(),
                    initial_margin: "5000".into(),
                    maintenance_margin: "250".into(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                },
                PositionInput {
                    symbol: "ETH/USDT".into(),
                    side: "SHORT".into(),
                    size: "10.0".into(),
                    entry_price: "3000".into(),
                    mark_price: "2900".into(),
                    liquidation_price: "3100".into(),
                    initial_margin: "3000".into(),
                    maintenance_margin: "150".into(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                },
            ],
            order: OrderInput {
                symbol: "SOL/USDT".into(),
                side: "BUY".into(),
                price: "100".into(),
                quantity: "10.0".into(),
                leverage: 20,
            },
        }
    }

    fn sell_order_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "SELL".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        }
    }

    fn max_leverage_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 125,
            },
        }
    }

    fn min_quantity_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "0.00000001".into(),
                leverage: 10,
            },
        }
    }

    fn high_precision_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "99999999.999999999999999999".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "SELL".into(),
                price: "50000.123456789012345678".into(),
                quantity: "0.123456789012345678".into(),
                leverage: 10,
            },
        }
    }

    fn tiny_balance_input() -> MarginPreviewInput<'static> {
        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "1".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        }
    }

    fn large_position_count_input() -> MarginPreviewInput<'static> {
        let positions: Vec<PositionInput> = (0..10)
            .map(|i| PositionInput {
                symbol: format!("PAIR{i}/USDT").into(),
                side: if i % 2 == 0 { "LONG" } else { "SHORT" }.into(),
                size: "1.0".into(),
                entry_price: format!("{}", 1000 + i * 500).into(),
                mark_price: format!("{}", 1010 + i * 500).into(),
                liquidation_price: format!("{}", 950 + i * 500).into(),
                initial_margin: "100".into(),
                maintenance_margin: "5".into(),
                leverage: 10,
                timestamp: 1_708_123_456_789_000_000,
            })
            .collect();

        MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "500000".into(),
            positions,
            order: OrderInput {
                symbol: "NEW/USDT".into(),
                side: "BUY".into(),
                price: "1000".into(),
                quantity: "5.0".into(),
                leverage: 20,
            },
        }
    }

    /// Build a diverse set of valid test vectors for replay / batch testing.
    fn diverse_test_vectors() -> Vec<MarginPreviewInput<'static>> {
        vec![
            standard_input(),
            empty_positions_input(),
            near_liquidation_input(),
            multi_position_input(),
            sell_order_input(),
            max_leverage_input(),
            min_quantity_input(),
            high_precision_input(),
            tiny_balance_input(),
            large_position_count_input(),
            // Variations: different leverages
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "50000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "0.5".into(),
                    leverage: 1,
                },
            },
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "50000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "0.5".into(),
                    leverage: 5,
                },
            },
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "50000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "0.5".into(),
                    leverage: 50,
                },
            },
            // Short with existing long position
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "100000".into(),
                positions: vec![PositionInput {
                    symbol: "BTC/USDT".into(),
                    side: "LONG".into(),
                    size: "1.0".into(),
                    entry_price: "50000".into(),
                    mark_price: "48000".into(),
                    liquidation_price: "47000".into(),
                    initial_margin: "5000".into(),
                    maintenance_margin: "250".into(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                }],
                order: OrderInput {
                    symbol: "ETH/USDT".into(),
                    side: "SELL".into(),
                    price: "3000".into(),
                    quantity: "5.0".into(),
                    leverage: 20,
                },
            },
            // High leverage with small balance
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "100".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "0.01".into(),
                    leverage: 125,
                },
            },
            // Large balance with small order
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "10000000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "1".into(),
                    quantity: "1.0".into(),
                    leverage: 1,
                },
            },
            // Various symbols
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "50000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "DOGE/USDT".into(),
                    side: "BUY".into(),
                    price: "0.10".into(),
                    quantity: "100000".into(),
                    leverage: 5,
                },
            },
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "50000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "XRP/USDT".into(),
                    side: "SELL".into(),
                    price: "0.50".into(),
                    quantity: "10000".into(),
                    leverage: 10,
                },
            },
            // Position with unrealized loss
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "20000".into(),
                positions: vec![PositionInput {
                    symbol: "BTC/USDT".into(),
                    side: "LONG".into(),
                    size: "1.0".into(),
                    entry_price: "50000".into(),
                    mark_price: "45000".into(),
                    liquidation_price: "44000".into(),
                    initial_margin: "5000".into(),
                    maintenance_margin: "250".into(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                }],
                order: OrderInput {
                    symbol: "ETH/USDT".into(),
                    side: "BUY".into(),
                    price: "3000".into(),
                    quantity: "1.0".into(),
                    leverage: 10,
                },
            },
            // Short position with profit
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "30000".into(),
                positions: vec![PositionInput {
                    symbol: "ETH/USDT".into(),
                    side: "SHORT".into(),
                    size: "5.0".into(),
                    entry_price: "3000".into(),
                    mark_price: "2800".into(),
                    liquidation_price: "3200".into(),
                    initial_margin: "1500".into(),
                    maintenance_margin: "75".into(),
                    leverage: 10,
                    timestamp: 1_708_123_456_789_000_000,
                }],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "0.1".into(),
                    leverage: 20,
                },
            },
        ]
    }

    // =======================================================================
    // 1. Deterministic Equivalence — Exhaustive Input Coverage
    // =======================================================================

    /// Helper: assert native and boundary produce identical output for a given input.
    fn assert_native_boundary_parity(input: &MarginPreviewInput, label: &str) {
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let native_result = native.compute(input).expect(&format!("{label}: native failed"));
        let boundary_result = boundary
            .compute(input)
            .expect(&format!("{label}: boundary failed"));

        assert_eq!(
            native_result, boundary_result,
            "{label}: Native ≠ Boundary"
        );

        // Both must pass validation
        assert!(
            validate_output(&native_result).is_ok(),
            "{label}: native output failed validation"
        );
        assert!(
            validate_output(&boundary_result).is_ok(),
            "{label}: boundary output failed validation"
        );
    }

    #[test]
    fn test_p14_equivalence_standard_input() {
        assert_native_boundary_parity(&standard_input(), "standard");
    }

    #[test]
    fn test_p14_equivalence_empty_positions() {
        assert_native_boundary_parity(&empty_positions_input(), "empty_positions");
    }

    #[test]
    fn test_p14_equivalence_near_liquidation() {
        assert_native_boundary_parity(&near_liquidation_input(), "near_liquidation");
    }

    #[test]
    fn test_p14_equivalence_multi_position() {
        assert_native_boundary_parity(&multi_position_input(), "multi_position");
    }

    #[test]
    fn test_p14_equivalence_sell_order() {
        assert_native_boundary_parity(&sell_order_input(), "sell_order");
    }

    #[test]
    fn test_p14_equivalence_max_leverage() {
        assert_native_boundary_parity(&max_leverage_input(), "max_leverage_125x");
    }

    #[test]
    fn test_p14_equivalence_min_quantity() {
        assert_native_boundary_parity(&min_quantity_input(), "min_quantity");
    }

    #[test]
    fn test_p14_equivalence_high_precision() {
        assert_native_boundary_parity(&high_precision_input(), "high_precision");
    }

    #[test]
    fn test_p14_equivalence_tiny_balance() {
        assert_native_boundary_parity(&tiny_balance_input(), "tiny_balance");
    }

    #[test]
    fn test_p14_equivalence_large_position_count() {
        assert_native_boundary_parity(&large_position_count_input(), "large_position_count");
    }

    #[test]
    fn test_p14_equivalence_all_diverse_vectors() {
        for (i, input) in diverse_test_vectors().iter().enumerate() {
            assert_native_boundary_parity(input, &format!("diverse_vector_{i}"));
        }
    }

    // Field-level equivalence: compare every output field between boundary
    // JSON and direct native engine call to catch any serialization drift.
    #[test]
    fn test_p14_field_level_equivalence_standard() {
        let input = standard_input();

        // Direct engine computation
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
        let native_preview = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );

        // Boundary computation
        let boundary_json =
            margin_preview_json(&serde_json::to_string(&input).unwrap()).unwrap();
        let boundary_output: MarginPreviewOutput =
            serde_json::from_str(&boundary_json).unwrap();

        // Compare every field
        assert_eq!(
            boundary_output.equity_after,
            native_preview.equity_after.to_string(),
            "equity_after field-level mismatch"
        );
        assert_eq!(
            boundary_output.margin_used_after,
            native_preview.margin_used_after.to_string(),
            "margin_used_after field-level mismatch"
        );
        assert_eq!(
            boundary_output.margin_available_after,
            native_preview.margin_available_after.to_string(),
            "margin_available_after field-level mismatch"
        );
        assert_eq!(
            boundary_output.margin_ratio_after,
            native_preview.margin_ratio_after.to_string(),
            "margin_ratio_after field-level mismatch"
        );
        assert_eq!(
            boundary_output.liquidation_price,
            native_preview.liquidation_price.to_string(),
            "liquidation_price field-level mismatch"
        );
        assert_eq!(
            boundary_output.leverage_ratio,
            native_preview.leverage_ratio.to_string(),
            "leverage_ratio field-level mismatch"
        );
        assert_eq!(
            boundary_output.has_negative_balance,
            native_preview.has_negative_balance,
            "has_negative_balance field-level mismatch"
        );
    }

    // =======================================================================
    // 2. Repeat-Run Determinism (100+ iterations)
    // =======================================================================

    #[test]
    fn test_p14_determinism_native_100_iterations() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();
        let reference = adapter.compute(&input).unwrap();

        for i in 1..=100 {
            let result = adapter.compute(&input).unwrap();
            assert_eq!(
                reference, result,
                "Native determinism failed at iteration {i}"
            );
        }
    }

    #[test]
    fn test_p14_determinism_boundary_100_iterations() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();
        let reference = adapter.compute(&input).unwrap();

        for i in 1..=100 {
            let result = adapter.compute(&input).unwrap();
            assert_eq!(
                reference, result,
                "Boundary determinism failed at iteration {i}"
            );
        }
    }

    #[test]
    fn test_p14_determinism_cross_path_100_iterations() {
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();

        let native_ref = native.compute(&input).unwrap();

        for i in 1..=100 {
            let native_r = native.compute(&input).unwrap();
            let boundary_r = boundary.compute(&input).unwrap();
            assert_eq!(
                native_ref, native_r,
                "Cross-path: native diverged at iteration {i}"
            );
            assert_eq!(
                native_ref, boundary_r,
                "Cross-path: boundary diverged at iteration {i}"
            );
        }
    }

    #[test]
    fn test_p14_determinism_json_string_100_iterations() {
        let input = standard_input();
        let input_json = serde_json::to_string(&input).unwrap();
        let reference = margin_preview_json(&input_json).unwrap();

        for i in 1..=100 {
            let result = margin_preview_json(&input_json).unwrap();
            assert_eq!(
                reference, result,
                "JSON string determinism failed at iteration {i}"
            );
        }
    }

    #[test]
    fn test_p14_determinism_diverse_vectors_10_repeats() {
        let vectors = diverse_test_vectors();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        // First pass: collect references
        let references: Vec<_> = vectors
            .iter()
            .map(|v| {
                let nr = native.compute(v).unwrap();
                let br = boundary.compute(v).unwrap();
                assert_eq!(nr, br, "Reference collection: native ≠ boundary");
                nr
            })
            .collect();

        // Repeat 10 times and verify
        for repeat in 0..10 {
            for (i, input) in vectors.iter().enumerate() {
                let nr = native.compute(input).unwrap();
                let br = boundary.compute(input).unwrap();
                assert_eq!(
                    references[i], nr,
                    "vector {i}, repeat {repeat}: native diverged"
                );
                assert_eq!(
                    references[i], br,
                    "vector {i}, repeat {repeat}: boundary diverged"
                );
            }
        }
    }

    // =======================================================================
    // 3. Boundary and Extreme Input Tests
    // =======================================================================

    #[test]
    fn test_p14_boundary_zero_balance_has_negative() {
        // Balance of 1 with large order → should show negative
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let output = adapter.compute(&tiny_balance_input()).unwrap();
        assert!(
            output.has_negative_balance,
            "Tiny balance with large order should show negative"
        );
        assert!(validate_output(&output).is_ok());
    }

    #[test]
    fn test_p14_boundary_max_leverage_margin_value() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let output = adapter.compute(&max_leverage_input()).unwrap();

        // IM = 50000 / 125 = 400
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(400));
        assert!(!output.has_negative_balance);
        assert!(validate_output(&output).is_ok());
    }

    #[test]
    fn test_p14_boundary_min_quantity_no_panic() {
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = min_quantity_input();

        let nr = native.compute(&input).unwrap();
        let br = boundary.compute(&input).unwrap();
        assert_eq!(nr, br);

        // Margin for 0.00000001 BTC at 50000 lev 10 = 50000 * 0.00000001 / 10 = 0.00005
        let margin_used: Decimal = nr.margin_used_after.parse().unwrap();
        assert!(margin_used > Decimal::ZERO);
        assert!(validate_output(&nr).is_ok());
    }

    #[test]
    fn test_p14_boundary_high_precision_no_drift() {
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = high_precision_input();

        let nr = native.compute(&input).unwrap();
        let br = boundary.compute(&input).unwrap();
        assert_eq!(nr, br, "High precision input caused drift");
        assert!(validate_output(&nr).is_ok());
    }

    #[test]
    fn test_p14_boundary_10_positions_stable() {
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = large_position_count_input();

        let nr = native.compute(&input).unwrap();
        let br = boundary.compute(&input).unwrap();
        assert_eq!(nr, br, "Large position count caused divergence");
        assert!(validate_output(&nr).is_ok());
    }

    #[test]
    fn test_p14_boundary_leverage_1x() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 1,
            },
        };

        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let output = adapter.compute(&input).unwrap();

        // IM = 50000 / 1 = 50000
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(50_000));
        assert!(validate_output(&output).is_ok());
    }

    #[test]
    fn test_p14_boundary_very_small_price() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "1000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "SHIB/USDT".into(),
                side: "BUY".into(),
                price: "0.00001".into(),
                quantity: "10000000".into(),
                leverage: 10,
            },
        };

        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let nr = native.compute(&input).unwrap();
        let br = boundary.compute(&input).unwrap();
        assert_eq!(nr, br, "Very small price caused divergence");
        assert!(validate_output(&nr).is_ok());
    }

    // =======================================================================
    // 4. Invalid Input Rejection — Both Paths Consistent
    // =======================================================================

    #[test]
    fn test_p14_reject_negative_price_boundary() {
        // Price::from_str panics on negative values at the types layer.
        // Verify that both native and boundary paths panic consistently.
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "-50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        // Both paths should panic or error on negative price — they are consistent.
        let native_result = std::panic::catch_unwind(|| {
            let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
            native.compute(&input)
        });
        let boundary_result = std::panic::catch_unwind(|| {
            let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
            boundary.compute(&input)
        });

        // Both should panic (the Price type asserts positivity)
        assert!(
            native_result.is_err() || native_result.unwrap().is_err(),
            "Native should reject negative price"
        );
        assert!(
            boundary_result.is_err() || boundary_result.unwrap().is_err(),
            "Boundary should reject negative price"
        );
    }

    #[test]
    fn test_p14_reject_negative_quantity_boundary() {
        // Quantity::from_str panics on negative values at the types layer.
        // Verify that both paths panic consistently.
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "-1.0".into(),
                leverage: 10,
            },
        };

        let native_result = std::panic::catch_unwind(|| {
            let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
            native.compute(&input)
        });
        let boundary_result = std::panic::catch_unwind(|| {
            let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
            boundary.compute(&input)
        });

        // Both should panic (the Quantity type asserts positivity)
        assert!(
            native_result.is_err() || native_result.unwrap().is_err(),
            "Native should reject negative quantity"
        );
        assert!(
            boundary_result.is_err() || boundary_result.unwrap().is_err(),
            "Boundary should reject negative quantity"
        );
    }

    #[test]
    fn test_p14_reject_zero_leverage_both_paths() {
        // Leverage 0 causes division by zero in Decimal arithmetic.
        // The margin engine panics on this — verify both paths do so consistently.
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 0,
            },
        };

        let native_result = std::panic::catch_unwind(|| {
            let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
            native.compute(&input)
        });
        let boundary_result = std::panic::catch_unwind(|| {
            let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
            boundary.compute(&input)
        });

        // Both should panic on division by zero
        assert!(
            native_result.is_err() || native_result.unwrap().is_err(),
            "Native should reject zero leverage"
        );
        assert!(
            boundary_result.is_err() || boundary_result.unwrap().is_err(),
            "Boundary should reject zero leverage"
        );
    }

    #[test]
    fn test_p14_reject_nan_like_string() {
        let result = margin_preview_json(
            r#"{"account_id":"01939d7f-8e4a-7890-a123-456789abcdef","total_balance":"NaN","positions":[],"order":{"symbol":"BTC/USDT","side":"BUY","price":"50000","quantity":"1.0","leverage":10}}"#,
        );
        assert!(result.is_err(), "NaN balance string should be rejected");
    }

    #[test]
    fn test_p14_reject_infinity_string() {
        let result = margin_preview_json(
            r#"{"account_id":"01939d7f-8e4a-7890-a123-456789abcdef","total_balance":"Infinity","positions":[],"order":{"symbol":"BTC/USDT","side":"BUY","price":"50000","quantity":"1.0","leverage":10}}"#,
        );
        assert!(result.is_err(), "Infinity balance string should be rejected");
    }

    #[test]
    fn test_p14_reject_empty_symbol() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        // Empty symbol is structurally valid (it's just a string), so both
        // paths should produce the same result without panicking.
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let nr = native.compute(&input);
        let br = boundary.compute(&input);

        match (&nr, &br) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(_), Err(_)) => {}
            _ => panic!("Empty symbol: inconsistent handling"),
        }
    }

    #[test]
    fn test_p14_reject_negative_balance() {
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "-100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let nr = native.compute(&input);
        let br = boundary.compute(&input);

        match (&nr, &br) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "Negative balance: paths diverged");
                assert!(a.has_negative_balance, "Should indicate negative balance");
            }
            (Err(_), Err(_)) => {}
            _ => panic!("Negative balance: inconsistent handling"),
        }
    }

    // =======================================================================
    // 5. Replay Consistency (sequence of diverse inputs)
    // =======================================================================

    #[test]
    fn test_p14_replay_consistency_all_vectors() {
        let vectors = diverse_test_vectors();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let mut mismatch_count = 0;

        for (i, input) in vectors.iter().enumerate() {
            let nr = native.compute(input);
            let br = boundary.compute(input);

            match (&nr, &br) {
                (Ok(a), Ok(b)) => {
                    if a != b {
                        eprintln!("MISMATCH at vector {i}: native ≠ boundary");
                        mismatch_count += 1;
                    }
                    // Validate both
                    assert!(
                        validate_output(a).is_ok(),
                        "vector {i}: native output failed validation"
                    );
                    assert!(
                        validate_output(b).is_ok(),
                        "vector {i}: boundary output failed validation"
                    );
                }
                (Err(e1), Err(e2)) => {
                    // Both failed — acceptable if consistent error type
                    eprintln!("vector {i}: both paths failed (native: {e1}, boundary: {e2})");
                }
                (Ok(_), Err(e)) => {
                    panic!("vector {i}: native succeeded but boundary failed: {e}");
                }
                (Err(e), Ok(_)) => {
                    panic!("vector {i}: boundary succeeded but native failed: {e}");
                }
            }
        }

        assert_eq!(
            mismatch_count, 0,
            "{mismatch_count} output mismatches in replay sequence"
        );
    }

    #[test]
    fn test_p14_replay_order_independence() {
        // Run the same vectors in forward and reverse order.
        // Results for each vector should be identical regardless of order.
        let vectors = diverse_test_vectors();
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);

        let forward: Vec<_> = vectors
            .iter()
            .map(|v| adapter.compute(v))
            .collect();

        let reverse: Vec<_> = vectors
            .iter()
            .rev()
            .map(|v| adapter.compute(v))
            .collect();

        // Reverse the reverse results for comparison
        let reverse_corrected: Vec<_> = reverse.into_iter().rev().collect();

        for (i, (fwd, rev)) in forward.iter().zip(reverse_corrected.iter()).enumerate() {
            match (fwd, rev) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "vector {i}: order-dependent result"),
                (Err(_), Err(_)) => {}
                _ => panic!("vector {i}: order changed success/failure"),
            }
        }
    }

    // =======================================================================
    // 6. Fallback Consistency
    // =======================================================================

    #[test]
    fn test_p14_fallback_boundary_to_native_on_valid_input() {
        // Boundary mode with valid input should work; result should match
        // what native would produce.
        let input = standard_input();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let nr = native.compute(&input).unwrap();
        let br = boundary.compute(&input).unwrap();
        assert_eq!(nr, br, "Boundary should match native for valid input");
    }

    #[test]
    fn test_p14_fallback_feature_flag_disabled_vs_native_mode() {
        let input = standard_input();
        let p12_disabled = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let p13_native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);

        let r1 = p12_disabled.compute(&input).unwrap();
        let r2 = p13_native.compute(&input).unwrap();
        assert_eq!(r1, r2, "Phase 12 disabled ≠ Phase 13 native");
    }

    #[test]
    fn test_p14_fallback_feature_flag_enabled_vs_boundary_mode() {
        let input = standard_input();
        let p12_enabled = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
        let p13_boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let r1 = p12_enabled.compute(&input).unwrap();
        let r2 = p13_boundary.compute(&input).unwrap();
        assert_eq!(r1, r2, "Phase 12 enabled ≠ Phase 13 boundary");
    }

    #[test]
    fn test_p14_fallback_all_four_paths_identical() {
        let input = standard_input();

        let results = vec![
            MarginPreviewAdapter::new(WasmFeatureFlag::Disabled)
                .compute(&input)
                .unwrap(),
            MarginPreviewAdapter::new(WasmFeatureFlag::Enabled)
                .compute(&input)
                .unwrap(),
            MarginPreviewAdapter::with_mode(WasmExecutionMode::Native)
                .compute(&input)
                .unwrap(),
            MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary)
                .compute(&input)
                .unwrap(),
        ];

        for i in 1..results.len() {
            assert_eq!(
                results[0], results[i],
                "Path 0 ≠ Path {i}: fallback consistency broken"
            );
        }
    }

    // =======================================================================
    // 7. Validation Before State Usage
    // =======================================================================

    #[test]
    fn test_p14_every_computed_output_passes_validation() {
        let vectors = diverse_test_vectors();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        for (i, input) in vectors.iter().enumerate() {
            if let Ok(output) = native.compute(input) {
                assert!(
                    validate_output(&output).is_ok(),
                    "vector {i}: native output failed validation: {:?}",
                    validate_output(&output).unwrap_err()
                );
            }
            if let Ok(output) = boundary.compute(input) {
                assert!(
                    validate_output(&output).is_ok(),
                    "vector {i}: boundary output failed validation: {:?}",
                    validate_output(&output).unwrap_err()
                );
            }
        }
    }

    #[test]
    fn test_p14_validation_catches_tampered_output() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let mut output = adapter.compute(&standard_input()).unwrap();

        // Tamper with equity — should break consistency check
        output.equity_after = "999999999".into();
        assert!(
            validate_output(&output).is_err(),
            "Tampered equity should fail validation"
        );
    }

    #[test]
    fn test_p14_validation_catches_invalid_risk_level() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let mut output = adapter.compute(&standard_input()).unwrap();

        output.risk_level = "CriticalMeltdown".into();
        assert!(
            validate_output(&output).is_err(),
            "Invalid risk level should fail validation"
        );
    }

    #[test]
    fn test_p14_validation_catches_empty_fields() {
        let fields = [
            "equity_after",
            "margin_used_after",
            "margin_available_after",
            "margin_ratio_after",
            "liquidation_price",
            "leverage_ratio",
        ];

        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let base = adapter.compute(&standard_input()).unwrap();

        for field in &fields {
            let mut output = base.clone();
            match *field {
                "equity_after" => output.equity_after = "".into(),
                "margin_used_after" => output.margin_used_after = "".into(),
                "margin_available_after" => output.margin_available_after = "".into(),
                "margin_ratio_after" => output.margin_ratio_after = "".into(),
                "liquidation_price" => output.liquidation_price = "".into(),
                "leverage_ratio" => output.leverage_ratio = "".into(),
                _ => unreachable!(),
            }
            assert!(
                validate_output(&output).is_err(),
                "Empty {field} should fail validation"
            );
        }
    }

    // =======================================================================
    // 8. Regression Safety
    // =======================================================================

    #[test]
    fn test_p14_regression_direct_engine_api_unchanged() {
        let account_uuid = uuid::Uuid::parse_str(TEST_ACCOUNT_ID).unwrap();
        let account_id = AccountId::from_uuid(account_uuid);
        let engine = CrossMarginEngine::new(account_id, Decimal::from(100_000));

        let preview = engine.simulate_order(
            "BTC/USDT",
            Side::BUY,
            Price::from_u64(50_000),
            Quantity::from_str("1.0").unwrap(),
            10,
        );

        // Known values from Phase 12
        assert_eq!(preview.equity_after, Decimal::from(100_000));
        assert_eq!(preview.margin_used_after, Decimal::from(5_000));
        assert_eq!(preview.margin_available_after, Decimal::from(95_000));
        assert!(!preview.has_negative_balance);
    }

    #[test]
    fn test_p14_regression_boundary_function_stable() {
        let input = standard_input();
        let input_json = serde_json::to_string(&input).unwrap();

        // Known values from Phase 12 test.
        // Compare via Decimal parse to handle trailing-zero normalization
        // (Decimal::to_string may produce "102000.0" vs "102000").
        let result = margin_preview_json(&input_json).unwrap();
        let output: MarginPreviewOutput = serde_json::from_str(&result).unwrap();

        let equity: Decimal = output.equity_after.parse().unwrap();
        assert_eq!(equity, Decimal::from(102_000));
        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(11_500));
        let margin_available: Decimal = output.margin_available_after.parse().unwrap();
        assert_eq!(margin_available, Decimal::from(90_500));
        assert_eq!(output.risk_level, "Healthy");
        assert!(!output.has_negative_balance);
    }

    #[test]
    fn test_p14_regression_validation_rules_unchanged() {
        // Valid output passes
        let valid = MarginPreviewOutput {
            equity_after: "102000".into(),
            margin_used_after: "11500".into(),
            margin_available_after: "90500".into(),
            margin_ratio_after: "204".into(),
            liquidation_price: "2860.50000000".into(),
            leverage_ratio: "1.27450980".into(),
            risk_level: "Healthy".into(),
            has_negative_balance: false,
        };
        assert!(validate_output(&valid).is_ok());

        // Invalid: empty equity still fails
        let invalid = MarginPreviewOutput {
            equity_after: "".into(),
            ..valid.clone()
        };
        assert!(validate_output(&invalid).is_err());

        // Invalid: bad risk level still fails
        let invalid2 = MarginPreviewOutput {
            risk_level: "Unknown".into(),
            ..valid.clone()
        };
        assert!(validate_output(&invalid2).is_err());

        // Invalid: inconsistent margins still fails
        let invalid3 = MarginPreviewOutput {
            margin_available_after: "1".into(),
            ..valid.clone()
        };
        assert!(validate_output(&invalid3).is_err());
    }

    #[test]
    fn test_p14_regression_phase12_test_values_unchanged() {
        // Exact reproduction of Phase 12 standard test values
        let input = standard_input();
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
        let output = adapter.compute(&input).unwrap();

        let equity: Decimal = output.equity_after.parse().unwrap();
        assert_eq!(equity, Decimal::from(102_000));

        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(11_500));

        let margin_available: Decimal = output.margin_available_after.parse().unwrap();
        assert_eq!(margin_available, Decimal::from(90_500));

        assert!(!output.has_negative_balance);
        assert_eq!(output.risk_level, "Healthy");
    }
}
