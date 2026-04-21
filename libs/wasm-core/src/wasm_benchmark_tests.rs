//! Phase 14 — WASM Benchmark Tests
//!
//! Performance benchmarking for the WASM margin preview path vs native Rust.
//! Measures latency, throughput, failure rate, validation overhead, fallback
//! frequency, and percentile distributions.
//!
//! These are integration tests that leverage the Phase 13 `BenchmarkCollector`
//! and `benchmark_compute` infrastructure. All benchmarks are deterministic
//! and reproducible — they use fixed input data and measure wall-clock time.
//!
//! Run with `--nocapture` to see the benchmark report:
//! ```bash
//! cargo test -p wasm-core --lib wasm_benchmark_tests -- --nocapture
//! ```

#[cfg(test)]
mod tests {
    use crate::wasm_adapter::{
        validate_output, MarginPreviewAdapter, WasmExecutionMode,
    };
    use crate::wasm_bench::{
        benchmark_compute, compare_native_vs_boundary, BenchmarkCollector, ExecutionMetrics,
        ExecutionPath,
    };
    use crate::wasm_bindings::{
        margin_preview_json, MarginPreviewInput, MarginPreviewOutput, OrderInput, PositionInput,
    };
    use rust_decimal::Decimal;
    use std::time::Instant;

    // -----------------------------------------------------------------------
    // Test fixtures
    // -----------------------------------------------------------------------

    const TEST_ACCOUNT_ID: &str = "01939d7f-8e4a-7890-a123-456789abcdef";
    const WARMUP_ITERATIONS: usize = 50;
    const MEASURED_ITERATIONS: usize = 500;

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

    fn diverse_inputs() -> Vec<MarginPreviewInput<'static>> {
        vec![
            standard_input(),
            // Empty positions
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
            },
            // Multi-position
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
            },
            // Max leverage
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
            },
            // Sell order
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
            },
        ]
    }

    /// Compute percentile from a sorted slice of latencies.
    fn percentile(sorted: &[u64], p: f64) -> u64 {
        if sorted.is_empty() {
            return 0;
        }
        let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    // =======================================================================
    // 1. Latency Benchmark — Native vs Boundary
    // =======================================================================

    #[test]
    fn test_p14_benchmark_latency_native_vs_boundary() {
        let input = standard_input();
        let native_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        // Warm-up
        for _ in 0..WARMUP_ITERATIONS {
            let _ = native_adapter.compute(&input);
            let _ = boundary_adapter.compute(&input);
        }

        // Measure native
        let mut native_latencies = Vec::with_capacity(MEASURED_ITERATIONS);
        for _ in 0..MEASURED_ITERATIONS {
            let start = Instant::now();
            let _ = native_adapter.compute(&input).unwrap();
            native_latencies.push(start.elapsed().as_nanos() as u64);
        }

        // Measure boundary
        let mut boundary_latencies = Vec::with_capacity(MEASURED_ITERATIONS);
        for _ in 0..MEASURED_ITERATIONS {
            let start = Instant::now();
            let _ = boundary_adapter.compute(&input).unwrap();
            boundary_latencies.push(start.elapsed().as_nanos() as u64);
        }

        native_latencies.sort();
        boundary_latencies.sort();

        let native_avg: u64 = native_latencies.iter().sum::<u64>() / MEASURED_ITERATIONS as u64;
        let boundary_avg: u64 =
            boundary_latencies.iter().sum::<u64>() / MEASURED_ITERATIONS as u64;

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║         LATENCY BENCHMARK — Native vs Boundary          ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!(
            "║  Native   avg: {:>8}ns  p50: {:>8}ns  p95: {:>8}ns ║",
            native_avg,
            percentile(&native_latencies, 50.0),
            percentile(&native_latencies, 95.0)
        );
        eprintln!(
            "║  Boundary avg: {:>8}ns  p50: {:>8}ns  p95: {:>8}ns ║",
            boundary_avg,
            percentile(&boundary_latencies, 50.0),
            percentile(&boundary_latencies, 95.0)
        );
        eprintln!(
            "║  p99  Native: {:>8}ns   Boundary: {:>8}ns            ║",
            percentile(&native_latencies, 99.0),
            percentile(&boundary_latencies, 99.0)
        );
        if native_avg > 0 {
            let ratio = boundary_avg as f64 / native_avg as f64;
            eprintln!("║  Boundary/Native ratio: {:.2}x                            ║", ratio);
        }
        eprintln!(
            "║  Iterations: {} (after {} warmup)              ║",
            MEASURED_ITERATIONS, WARMUP_ITERATIONS
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        // The test passes regardless of performance — we're measuring, not gating.
        // But both paths must produce valid results.
        assert!(native_latencies.len() == MEASURED_ITERATIONS);
        assert!(boundary_latencies.len() == MEASURED_ITERATIONS);
    }

    // =======================================================================
    // 2. Throughput Benchmark
    // =======================================================================

    #[test]
    fn test_p14_benchmark_throughput() {
        let inputs = diverse_inputs();
        let native_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        // Warm-up
        for input in &inputs {
            for _ in 0..10 {
                let _ = native_adapter.compute(input);
                let _ = boundary_adapter.compute(input);
            }
        }

        let batch_count = 100;
        let ops_per_batch = inputs.len();

        // Native throughput
        let start = Instant::now();
        for _ in 0..batch_count {
            for input in &inputs {
                let _ = native_adapter.compute(input).unwrap();
            }
        }
        let native_elapsed = start.elapsed();
        let native_total_ops = batch_count * ops_per_batch;
        let native_ops_per_sec =
            (native_total_ops as f64 / native_elapsed.as_secs_f64()) as u64;

        // Boundary throughput
        let start = Instant::now();
        for _ in 0..batch_count {
            for input in &inputs {
                let _ = boundary_adapter.compute(input).unwrap();
            }
        }
        let boundary_elapsed = start.elapsed();
        let boundary_total_ops = batch_count * ops_per_batch;
        let boundary_ops_per_sec =
            (boundary_total_ops as f64 / boundary_elapsed.as_secs_f64()) as u64;

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║              THROUGHPUT BENCHMARK                       ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!(
            "║  Native:   {:>8} ops/sec  ({} ops in {:>6.2}ms)       ║",
            native_ops_per_sec,
            native_total_ops,
            native_elapsed.as_secs_f64() * 1000.0
        );
        eprintln!(
            "║  Boundary: {:>8} ops/sec  ({} ops in {:>6.2}ms)       ║",
            boundary_ops_per_sec,
            boundary_total_ops,
            boundary_elapsed.as_secs_f64() * 1000.0
        );
        eprintln!(
            "║  Batch size: {} inputs × {} batches                    ║",
            ops_per_batch, batch_count
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        // Sanity: both paths produced valid results
        assert!(native_ops_per_sec > 0);
        assert!(boundary_ops_per_sec > 0);
    }

    // =======================================================================
    // 3. Failure Rate Under Invalid Inputs
    // =======================================================================

    #[test]
    fn test_p14_benchmark_failure_rate() {
        let valid_inputs = diverse_inputs();
        let invalid_inputs: Vec<MarginPreviewInput> = vec![
            // Bad UUID
            MarginPreviewInput {
                account_id: "not-uuid".into(),
                total_balance: "100000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "50000".into(),
                    quantity: "1.0".into(),
                    leverage: 10,
                },
            },
            // Bad balance
            MarginPreviewInput {
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
            },
            // Bad side
            MarginPreviewInput {
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
            },
            // Bad price
            MarginPreviewInput {
                account_id: TEST_ACCOUNT_ID.into(),
                total_balance: "100000".into(),
                positions: vec![],
                order: OrderInput {
                    symbol: "BTC/USDT".into(),
                    side: "BUY".into(),
                    price: "abc".into(),
                    quantity: "1.0".into(),
                    leverage: 10,
                },
            },
        ];

        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let mut native_success = 0u64;
        let mut native_fail = 0u64;
        let mut boundary_success = 0u64;
        let mut boundary_fail = 0u64;

        // Test valid inputs
        for input in &valid_inputs {
            if native.compute(input).is_ok() {
                native_success += 1;
            } else {
                native_fail += 1;
            }
            if boundary.compute(input).is_ok() {
                boundary_success += 1;
            } else {
                boundary_fail += 1;
            }
        }

        // Test invalid inputs
        for input in &invalid_inputs {
            if native.compute(input).is_ok() {
                native_success += 1;
            } else {
                native_fail += 1;
            }
            if boundary.compute(input).is_ok() {
                boundary_success += 1;
            } else {
                boundary_fail += 1;
            }
        }

        let total = (valid_inputs.len() + invalid_inputs.len()) as u64;

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║                FAILURE RATE ANALYSIS                    ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!(
            "║  Native:   {}/{} success,  {}/{} fail                   ║",
            native_success, total, native_fail, total
        );
        eprintln!(
            "║  Boundary: {}/{} success,  {}/{} fail                   ║",
            boundary_success, total, boundary_fail, total
        );
        eprintln!(
            "║  Valid inputs: {}  Invalid inputs: {}                    ║",
            valid_inputs.len(),
            invalid_inputs.len()
        );
        eprintln!(
            "║  Failure rate: Native {:.1}%  Boundary {:.1}%              ║",
            (native_fail as f64 / total as f64) * 100.0,
            (boundary_fail as f64 / total as f64) * 100.0
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        // Both paths should fail on exactly the invalid inputs
        assert_eq!(
            native_fail, boundary_fail,
            "Failure rate differs between paths"
        );
        assert_eq!(native_fail, invalid_inputs.len() as u64);
    }

    // =======================================================================
    // 4. Validation Overhead
    // =======================================================================

    #[test]
    fn test_p14_benchmark_validation_overhead() {
        let input = standard_input();
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);

        // Warm-up
        for _ in 0..WARMUP_ITERATIONS {
            let _ = adapter.compute(&input);
        }

        // Measure compute without explicit validation
        let mut without_validation_ns = Vec::with_capacity(MEASURED_ITERATIONS);
        for _ in 0..MEASURED_ITERATIONS {
            let start = Instant::now();
            let output = adapter.compute(&input).unwrap();
            // Don't validate — just use it
            let _ = &output.equity_after;
            without_validation_ns.push(start.elapsed().as_nanos() as u64);
        }

        // Measure compute with explicit validation
        let mut with_validation_ns = Vec::with_capacity(MEASURED_ITERATIONS);
        for _ in 0..MEASURED_ITERATIONS {
            let start = Instant::now();
            let output = adapter.compute(&input).unwrap();
            let _ = validate_output(&output).unwrap();
            with_validation_ns.push(start.elapsed().as_nanos() as u64);
        }

        without_validation_ns.sort();
        with_validation_ns.sort();

        let avg_without: u64 =
            without_validation_ns.iter().sum::<u64>() / MEASURED_ITERATIONS as u64;
        let avg_with: u64 = with_validation_ns.iter().sum::<u64>() / MEASURED_ITERATIONS as u64;
        let overhead_ns = if avg_with > avg_without {
            avg_with - avg_without
        } else {
            0
        };

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║              VALIDATION OVERHEAD                        ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!("║  Without validation: avg {:>8}ns                     ║", avg_without);
        eprintln!("║  With validation:    avg {:>8}ns                     ║", avg_with);
        eprintln!("║  Overhead:               {:>8}ns                     ║", overhead_ns);
        if avg_without > 0 {
            let pct = (overhead_ns as f64 / avg_without as f64) * 100.0;
            eprintln!("║  Overhead ratio:         {:>7.2}%                      ║", pct);
        }
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        // Both paths must succeed
        assert!(without_validation_ns.len() == MEASURED_ITERATIONS);
        assert!(with_validation_ns.len() == MEASURED_ITERATIONS);
    }

    // =======================================================================
    // 5. Fallback Frequency
    // =======================================================================

    #[test]
    fn test_p14_benchmark_fallback_frequency() {
        let valid_inputs = diverse_inputs();

        // Mixed workload: valid + some that will cause boundary error
        // (boundary handles errors by falling back to native)
        let mixed_inputs: Vec<MarginPreviewInput> = valid_inputs
            .iter()
            .cloned()
            .chain(std::iter::once(MarginPreviewInput {
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
            }))
            .collect();

        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        let mut success_count = 0u64;
        let mut error_count = 0u64;

        let iterations = 100;
        for _ in 0..iterations {
            for input in &mixed_inputs {
                match boundary.compute(input) {
                    Ok(output) => {
                        assert!(validate_output(&output).is_ok());
                        success_count += 1;
                    }
                    Err(_) => {
                        error_count += 1;
                    }
                }
            }
        }

        let total = success_count + error_count;
        let error_rate = (error_count as f64 / total as f64) * 100.0;

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║              FALLBACK FREQUENCY                         ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!(
            "║  Total calls: {}  Success: {}  Errors: {}              ║",
            total, success_count, error_count
        );
        eprintln!("║  Error rate: {:.2}%                                      ║", error_rate);
        eprintln!(
            "║  ({} valid + 1 invalid) × {} iterations              ║",
            valid_inputs.len(),
            iterations
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        // Only the bad-uuid input should error
        let expected_errors = iterations as u64; // 1 bad per iteration
        assert_eq!(error_count, expected_errors, "Unexpected number of errors");
    }

    // =======================================================================
    // 6. BenchmarkCollector Integration (Phase 13 hooks)
    // =======================================================================

    #[test]
    fn test_p14_benchmark_collector_comprehensive() {
        let mut collector = BenchmarkCollector::new();
        let input = standard_input();

        // Run 50 iterations through compare function
        for _ in 0..50 {
            let parity = compare_native_vs_boundary(&input, &mut collector);
            assert!(parity, "Parity should hold for all iterations");
        }

        assert_eq!(collector.success_count(), 100); // 50 native + 50 boundary
        assert_eq!(collector.failure_count(), 0);

        let native_avg = collector
            .avg_latency_for_path(ExecutionPath::Native)
            .unwrap();
        let boundary_avg = collector
            .avg_latency_for_path(ExecutionPath::Boundary)
            .unwrap();

        let summary =
            collector.comparison_summary(ExecutionPath::Native, ExecutionPath::Boundary);

        eprintln!("\n╔══════════════════════════════════════════════════════════╗");
        eprintln!("║         BENCHMARK COLLECTOR SUMMARY                     ║");
        eprintln!("╠══════════════════════════════════════════════════════════╣");
        eprintln!("║  Total runs: {}                                        ║", collector.metrics.len());
        eprintln!("║  Successes: {}  Failures: {}                           ║", collector.success_count(), collector.failure_count());
        eprintln!("║  Native avg:   {:>8}ns                                ║", native_avg);
        eprintln!("║  Boundary avg: {:>8}ns                                ║", boundary_avg);
        eprintln!(
            "║  Parity: {} match, {} mismatch                          ║",
            summary.parity_match_count, summary.parity_mismatch_count
        );
        eprintln!("╚══════════════════════════════════════════════════════════╝\n");

        assert_eq!(summary.parity_mismatch_count, 0);
        assert!(summary.parity_match_count > 0);
    }

    // =======================================================================
    // 7. Stability Under Load
    // =======================================================================

    #[test]
    fn test_p14_stability_1000_sequential_calls() {
        let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
        let input = standard_input();
        let reference = adapter.compute(&input).unwrap();

        let mut success_count = 0u64;

        for i in 0..1000 {
            match adapter.compute(&input) {
                Ok(output) => {
                    assert_eq!(
                        reference, output,
                        "Stability: diverged at call {i}"
                    );
                    assert!(
                        validate_output(&output).is_ok(),
                        "Stability: validation failed at call {i}"
                    );
                    success_count += 1;
                }
                Err(e) => {
                    panic!("Stability: call {i} failed: {e}");
                }
            }
        }

        assert_eq!(success_count, 1000);
        eprintln!("\n  Stability test: 1000/1000 sequential calls succeeded ✓\n");
    }

    #[test]
    fn test_p14_stability_diverse_interleaved() {
        let inputs = diverse_inputs();
        let native = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
        let boundary = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

        // Interleave native and boundary calls with diverse inputs
        for round in 0..50 {
            for (i, input) in inputs.iter().enumerate() {
                let nr = native.compute(input).unwrap();
                let br = boundary.compute(input).unwrap();
                assert_eq!(
                    nr, br,
                    "Interleaved stability: round {round}, input {i}"
                );
            }
        }

        eprintln!(
            "\n  Interleaved stability: {} rounds × {} inputs = {} calls OK ✓\n",
            50,
            inputs.len(),
            50 * inputs.len() * 2
        );
    }
}
