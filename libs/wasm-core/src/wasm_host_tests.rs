//! Phase 13 — WASM Host Integration Tests
//!
//! Tests for the Phase 13 integration layer:
//! - Execution mode selection (Native, Boundary, WasmRuntime)
//! - Fallback behavior for all failure scenarios
//! - Deterministic equivalence across execution paths
//! - Output validation before state usage
//! - Benchmarking hooks
//! - Regression safety for Phase 12 behavior
//!
//! Tests that require a WASM binary are gated behind the `wasm-host` feature
//! and will skip gracefully if the binary is not available.

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

    // =======================================================================
    // 1. Execution Mode Selection Tests
    // =======================================================================

    #[test]
    fn test_native_mode_produces_valid_output() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok(), "Native mode should always succeed");
        assert!(validate_output(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_boundary_mode_produces_valid_output() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok(), "Boundary mode should succeed");
        assert!(validate_output(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_default_mode_is_native() {
        let mode = WasmExecutionMode::default();
        match mode {
            WasmExecutionMode::Native => {} // expected
            _ => panic!("Default mode should be Native"),
        }
    }

    #[test]
    fn test_native_mode_empty_positions() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = empty_positions_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_boundary_mode_empty_positions() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = empty_positions_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok());
    }

    // =======================================================================
    // 2. Deterministic Equivalence Across Modes
    // =======================================================================

    #[test]
    fn test_native_vs_boundary_equivalence_standard() {
        let input = standard_input();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let native_result = native.compute(&input).unwrap();
        let boundary_result = boundary.compute(&input).unwrap();

        assert_eq!(
            native_result, boundary_result,
            "Native and Boundary must produce identical output"
        );
    }

    #[test]
    fn test_native_vs_boundary_equivalence_empty_positions() {
        let input = empty_positions_input();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        assert_eq!(
            native.compute(&input).unwrap(),
            boundary.compute(&input).unwrap()
        );
    }

    #[test]
    fn test_native_vs_boundary_equivalence_near_liquidation() {
        let input = near_liquidation_input();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        assert_eq!(
            native.compute(&input).unwrap(),
            boundary.compute(&input).unwrap()
        );
    }

    #[test]
    fn test_mode_equivalence_with_multiple_positions() {
        let input = MarginPreviewInput {
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
        };

        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        assert_eq!(
            native.compute(&input).unwrap(),
            boundary.compute(&input).unwrap()
        );
    }

    #[test]
    fn test_repeated_native_calls_are_deterministic() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();

        let r1 = adapter.compute(&input).unwrap();
        let r2 = adapter.compute(&input).unwrap();
        let r3 = adapter.compute(&input).unwrap();

        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    #[test]
    fn test_repeated_boundary_calls_are_deterministic() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();

        let r1 = adapter.compute(&input).unwrap();
        let r2 = adapter.compute(&input).unwrap();
        let r3 = adapter.compute(&input).unwrap();

        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    // =======================================================================
    // 3. Backward Compatibility (Phase 12 interface)
    // =======================================================================

    #[test]
    fn test_phase12_disabled_flag_still_works() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let input = standard_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_phase12_enabled_flag_still_works() {
        let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
        let input = standard_input();
        let result = adapter.compute(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_phase12_default_flag_is_disabled() {
        let flag = WasmFeatureFlag::default();
        assert_eq!(flag, WasmFeatureFlag::Disabled);
    }

    #[test]
    fn test_phase12_vs_phase13_equivalence() {
        let input = standard_input();

        // Phase 12 interface
        let p12_disabled = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
        let p12_enabled = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);

        // Phase 13 interface
        let p13_native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let p13_boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let disabled_result = p12_disabled.compute(&input).unwrap();
        let enabled_result = p12_enabled.compute(&input).unwrap();
        let native_result = p13_native.compute(&input).unwrap();
        let boundary_result = p13_boundary.compute(&input).unwrap();

        // All four paths must produce identical output
        assert_eq!(disabled_result, enabled_result);
        assert_eq!(enabled_result, native_result);
        assert_eq!(native_result, boundary_result);
    }

    // =======================================================================
    // 4. Fallback Behavior
    // =======================================================================

    #[test]
    fn test_boundary_falls_back_to_native_on_invalid_input() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = MarginPreviewInput {
            account_id: "bad-uuid".into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        // Both boundary and native will fail on bad UUID - this is an input error
        let result = adapter.compute(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_native_mode_handles_invalid_input_gracefully() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = MarginPreviewInput {
            account_id: "not-a-valid-uuid".into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        let result = adapter.compute(&input);
        assert!(result.is_err());
        match result.unwrap_err() {
            AdapterError::InputError(_) => {} // expected
            other => panic!("Expected InputError, got: {other:?}"),
        }
    }

    #[test]
    fn test_native_mode_rejects_non_numeric_balance() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "not_a_number".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        let result = adapter.compute(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_native_mode_rejects_invalid_side() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = MarginPreviewInput {
            account_id: TEST_ACCOUNT_ID.into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "INVALID".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        let result = adapter.compute(&input);
        assert!(result.is_err());
    }

    // =======================================================================
    // 5. Output Validation
    // =======================================================================

    #[test]
    fn test_validation_rejects_tampered_equity() {
        let mut output = MarginPreviewOutput {
            equity_after: "102000".into(),
            margin_used_after: "11500".into(),
            margin_available_after: "90500".into(),
            margin_ratio_after: "204".into(),
            liquidation_price: "2860.50000000".into(),
            leverage_ratio: "1.27450980".into(),
            risk_level: "Healthy".into(),
            has_negative_balance: false,
        };

        // Tamper with equity — should break margin consistency
        output.equity_after = "999999".into();
        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::InconsistentMargins { .. } => {} // expected
            other => panic!("Expected InconsistentMargins, got: {other:?}"),
        }
    }

    #[test]
    fn test_validation_rejects_nan_field() {
        let output = MarginPreviewOutput {
            equity_after: "NaN".into(),
            margin_used_after: "11500".into(),
            margin_available_after: "90500".into(),
            margin_ratio_after: "204".into(),
            liquidation_price: "45250".into(),
            leverage_ratio: "1.27".into(),
            risk_level: "Healthy".into(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationFailure::InvalidDecimal { field, .. } => {
                assert_eq!(field, "equity_after");
            }
            other => panic!("Expected InvalidDecimal, got: {other:?}"),
        }
    }

    #[test]
    fn test_validation_rejects_infinity() {
        let output = MarginPreviewOutput {
            equity_after: "Infinity".into(),
            margin_used_after: "11500".into(),
            margin_available_after: "90500".into(),
            margin_ratio_after: "204".into(),
            liquidation_price: "45250".into(),
            leverage_ratio: "1.27".into(),
            risk_level: "Healthy".into(),
            has_negative_balance: false,
        };

        let result = validate_output(&output);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_passes_for_computed_output() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();
        let output = adapter.compute(&input).unwrap();
        assert!(
            validate_output(&output).is_ok(),
            "Computed output must pass validation"
        );
    }

    #[test]
    fn test_validation_passes_for_boundary_output() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();
        let output = adapter.compute(&input).unwrap();
        assert!(
            validate_output(&output).is_ok(),
            "Boundary output must pass validation"
        );
    }

    // =======================================================================
    // 6. Value Verification (ensures compute results are correct)
    // =======================================================================

    #[test]
    fn test_native_mode_standard_values() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();
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

    #[test]
    fn test_boundary_mode_standard_values() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();
        let output = adapter.compute(&input).unwrap();

        let equity: Decimal = output.equity_after.parse().unwrap();
        assert_eq!(equity, Decimal::from(102_000));

        let margin_used: Decimal = output.margin_used_after.parse().unwrap();
        assert_eq!(margin_used, Decimal::from(11_500));
    }

    // =======================================================================
    // 7. Benchmarking Hooks
    // =======================================================================

    #[test]
    fn test_benchmark_compute_captures_metrics() {
        use crate::wasm_bench::{benchmark_compute, ExecutionPath};

        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = standard_input();

        let (result, metrics) = benchmark_compute(&adapter, &input, ExecutionPath::Native);

        assert!(result.is_ok());
        assert!(metrics.success);
        assert!(metrics.validation_ok);
        assert_eq!(metrics.path, ExecutionPath::Native);
        assert!(metrics.latency_ns > 0);
    }

    #[test]
    fn test_benchmark_compute_boundary_path() {
        use crate::wasm_bench::{benchmark_compute, ExecutionPath};

        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();

        let (result, metrics) = benchmark_compute(&adapter, &input, ExecutionPath::Boundary);

        assert!(result.is_ok());
        assert!(metrics.success);
        assert_eq!(metrics.path, ExecutionPath::Boundary);
    }

    #[test]
    fn test_benchmark_collector_statistics() {
        use crate::wasm_bench::{benchmark_compute, BenchmarkCollector, ExecutionPath};

        let mut collector = BenchmarkCollector::new();

        let native_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();

        // Run multiple iterations
        for _ in 0..5 {
            let (_, m1) = benchmark_compute(&native_adapter, &input, ExecutionPath::Native);
            let (_, m2) = benchmark_compute(&boundary_adapter, &input, ExecutionPath::Boundary);
            collector.record(m1);
            collector.record(m2);
        }

        assert_eq!(collector.success_count(), 10);
        assert_eq!(collector.failure_count(), 0);
        assert!(collector.avg_latency_ns().is_some());
        assert!(collector.avg_latency_for_path(ExecutionPath::Native).is_some());
        assert!(collector.avg_latency_for_path(ExecutionPath::Boundary).is_some());
    }

    #[test]
    fn test_benchmark_comparison_summary() {
        use crate::wasm_bench::{BenchmarkCollector, ExecutionPath, compare_native_vs_boundary};

        let mut collector = BenchmarkCollector::new();
        let input = standard_input();

        let parity = compare_native_vs_boundary(&input, &mut collector);
        assert!(parity, "Native and Boundary should produce identical output");

        let summary = collector.comparison_summary(ExecutionPath::Native, ExecutionPath::Boundary);
        assert!(summary.avg_latency_a_ns.is_some());
        assert!(summary.avg_latency_b_ns.is_some());
        assert_eq!(summary.parity_match_count, 2); // both paths report parity
        assert_eq!(summary.parity_mismatch_count, 0);
    }

    #[test]
    fn test_benchmark_failed_compute_captures_error() {
        use crate::wasm_bench::{benchmark_compute, ExecutionPath};

        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let input = MarginPreviewInput {
            account_id: "bad-uuid".into(),
            total_balance: "100000".into(),
            positions: vec![],
            order: OrderInput {
                symbol: "BTC/USDT".into(),
                side: "BUY".into(),
                price: "50000".into(),
                quantity: "1.0".into(),
                leverage: 10,
            },
        };

        let (result, metrics) = benchmark_compute(&adapter, &input, ExecutionPath::Native);

        assert!(result.is_err());
        assert!(!metrics.success);
        assert!(metrics.error.is_some());
    }

    // =======================================================================
    // 8. WASM Runtime Tests (require wasm-host feature and .wasm binary)
    // =======================================================================

    #[cfg(feature = "wasm-host")]
    mod wasm_runtime_tests {
        use super::*;
        use crate::wasm_host::WasmRuntime;
        use std::sync::Arc;

        /// Try to load the WASM binary from known locations.
        /// Returns None if the binary isn't available (tests will skip).
        fn try_load_wasm_runtime() -> Option<Arc<WasmRuntime>> {
            // Check environment variable first
            if let Ok(path) = std::env::var("WASM_MODULE_PATH") {
                if let Ok(runtime) = WasmRuntime::from_file(std::path::Path::new(&path)) {
                    return Some(Arc::new(runtime));
                }
            }

            // Check common build output locations
            let paths = [
                "target/wasm32-unknown-unknown/release/wasm_core.wasm",
                "target/wasm32-unknown-unknown/debug/wasm_core.wasm",
                "../../target/wasm32-unknown-unknown/release/wasm_core.wasm",
                "../../target/wasm32-unknown-unknown/debug/wasm_core.wasm",
            ];

            for path in &paths {
                if let Ok(runtime) = WasmRuntime::from_file(std::path::Path::new(path)) {
                    return Some(Arc::new(runtime));
                }
            }

            None
        }

        #[test]
        fn test_wasm_runtime_rejects_invalid_bytes() {
            let result = WasmRuntime::new(b"not a wasm module");
            assert!(result.is_err());
        }

        #[test]
        fn test_wasm_runtime_rejects_empty_bytes() {
            let result = WasmRuntime::new(b"");
            assert!(result.is_err());
        }

        #[test]
        fn test_wasm_runtime_rejects_missing_file() {
            let result = WasmRuntime::from_file(std::path::Path::new("/nonexistent/file.wasm"));
            assert!(result.is_err());
        }

        #[test]
        fn test_wasm_runtime_mode_execution() {
            let runtime = match try_load_wasm_runtime() {
                Some(r) => r,
                None => {
                    eprintln!("SKIP: WASM binary not available");
                    return;
                }
            };

            let adapter =
                MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));
            let input = standard_input();
            let result = adapter.compute(&input);
            assert!(result.is_ok(), "WasmRuntime mode should produce valid output");
            assert!(validate_output(&result.unwrap()).is_ok());
        }

        #[test]
        fn test_wasm_runtime_deterministic_equivalence() {
            let runtime = match try_load_wasm_runtime() {
                Some(r) => r,
                None => {
                    eprintln!("SKIP: WASM binary not available");
                    return;
                }
            };

            let input = standard_input();

            let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
            let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
            let wasm_rt =
                MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));

            let native_result = native.compute(&input).unwrap();
            let boundary_result = boundary.compute(&input).unwrap();
            let wasm_result = wasm_rt.compute(&input).unwrap();

            assert_eq!(
                native_result, boundary_result,
                "Native vs Boundary mismatch"
            );
            assert_eq!(
                native_result, wasm_result,
                "Native vs WasmRuntime mismatch"
            );
        }

        #[test]
        fn test_wasm_runtime_repeated_calls_deterministic() {
            let runtime = match try_load_wasm_runtime() {
                Some(r) => r,
                None => {
                    eprintln!("SKIP: WASM binary not available");
                    return;
                }
            };

            let adapter =
                MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));
            let input = standard_input();

            let r1 = adapter.compute(&input).unwrap();
            let r2 = adapter.compute(&input).unwrap();
            let r3 = adapter.compute(&input).unwrap();

            assert_eq!(r1, r2);
            assert_eq!(r2, r3);
        }

        #[test]
        fn test_wasm_runtime_fallback_on_invalid_input() {
            let runtime = match try_load_wasm_runtime() {
                Some(r) => r,
                None => {
                    eprintln!("SKIP: WASM binary not available");
                    return;
                }
            };

            let adapter =
                MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));

            // Invalid UUID — both WASM and native will fail
            let input = MarginPreviewInput {
                account_id: "bad-uuid".into(),
                total_balance: "100000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "1.0".into(),
                    leverage: 10,
                },
            };

            // WASM will fail → fallback to native → native also fails on bad UUID
            let result = adapter.compute(&input);
            // Both fail with input error — this is correct behavior
            assert!(result.is_err());
        }

        #[test]
        fn test_wasm_runtime_benchmark_hooks() {
            let runtime = match try_load_wasm_runtime() {
                Some(r) => r,
                None => {
                    eprintln!("SKIP: WASM binary not available");
                    return;
                }
            };

            use crate::wasm_bench::{benchmark_compute, BenchmarkCollector, ExecutionPath};

            let mut collector = BenchmarkCollector::new();
            let input = standard_input();

            // Benchmark WASM runtime
            let wasm_adapter =
                MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));
            let (wasm_result, wasm_metrics) =
                benchmark_compute(&wasm_adapter, &input, ExecutionPath::WasmRuntime);
            collector.record(wasm_metrics);

            // Benchmark native
            let native_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
            let (native_result, native_metrics) =
                benchmark_compute(&native_adapter, &input, ExecutionPath::Native);
            collector.record(native_metrics);

            assert!(wasm_result.is_ok());
            assert!(native_result.is_ok());
            assert_eq!(wasm_result.unwrap(), native_result.unwrap());

            let summary = collector.comparison_summary(
                ExecutionPath::WasmRuntime,
                ExecutionPath::Native,
            );
            assert!(summary.avg_latency_a_ns.is_some());
            assert!(summary.avg_latency_b_ns.is_some());
        }
    }

    // =======================================================================
    // 9. Regression Safety
    // =======================================================================

    #[test]
    fn test_regression_boundary_function_still_works() {
        let input = standard_input();
        let input_json = serde_json::to_string(&input).unwrap();
        let result = margin_preview_json(&input_json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_regression_native_engine_direct() {
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

        assert_eq!(preview.equity_after, Decimal::from(100_000));
        assert_eq!(preview.margin_used_after, Decimal::from(5_000));
    }

    #[test]
    fn test_regression_validation_rules_unchanged() {
        // Valid output
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

        // Invalid: empty equity
        let invalid_empty = MarginPreviewOutput {
            equity_after: "".into(),
            ..valid.clone()
        };
        assert!(validate_output(&invalid_empty).is_err());

        // Invalid: bad risk level
        let invalid_risk = MarginPreviewOutput {
            risk_level: "Unknown".into(),
            ..valid.clone()
        };
        assert!(validate_output(&invalid_risk).is_err());
    }
}
