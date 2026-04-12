//! WASM Benchmarking Hooks — Lightweight instrumentation for execution comparison
//!
//! Provides minimal, deterministic instrumentation to compare WASM vs native
//! execution paths. Captures latency, success/failure, validation outcomes,
//! and output parity between paths.
//!
//! # Design Principles
//!
//! - Lightweight: no external benchmark framework dependency
//! - Deterministic: uses `std::time::Instant` (monotonic clock) for timing
//! - Non-intrusive: metrics are returned alongside results, not logged
//! - Production-safe: all instrumentation is zero-cost when not used

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use crate::wasm_adapter::{AdapterError, MarginPreviewAdapter, WasmExecutionMode};
use crate::wasm_bindings::MarginPreviewInput;

// ---------------------------------------------------------------------------
// Execution path identifier
// ---------------------------------------------------------------------------

/// Identifies which execution path was used for a computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionPath {
    /// Direct native Rust computation (no serialization)
    Native,
    /// Same-process JSON boundary path (simulates WASM FFI)
    Boundary,
    /// Actual WASM module execution via wasmtime
    WasmRuntime,
    /// Fallback was triggered — original path failed, fell back to native
    FallbackToNative,
}

impl std::fmt::Display for ExecutionPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionPath::Native => write!(f, "Native"),
            ExecutionPath::Boundary => write!(f, "Boundary"),
            ExecutionPath::WasmRuntime => write!(f, "WasmRuntime"),
            ExecutionPath::FallbackToNative => write!(f, "FallbackToNative"),
        }
    }
}

// ---------------------------------------------------------------------------
// Execution metrics
// ---------------------------------------------------------------------------

/// Metrics captured for a single execution of the margin preview computation.
#[derive(Debug, Clone)]
pub struct ExecutionMetrics {
    /// Latency in nanoseconds
    pub latency_ns: u64,
    /// Which execution path was used
    pub path: ExecutionPath,
    /// Whether the output passed validation
    pub validation_ok: bool,
    /// Whether the computation succeeded
    pub success: bool,
    /// Optional: whether the output matched the reference (native) output
    pub parity_match: Option<bool>,
    /// Error message if the computation failed
    pub error: Option<String>,
}

impl ExecutionMetrics {
    /// Create metrics for a successful execution.
    fn success(latency_ns: u64, path: ExecutionPath, validation_ok: bool) -> Self {
        Self {
            latency_ns,
            path,
            validation_ok,
            success: true,
            parity_match: None,
            error: None,
        }
    }

    /// Create metrics for a failed execution.
    fn failure(latency_ns: u64, path: ExecutionPath, error: String) -> Self {
        Self {
            latency_ns,
            path,
            validation_ok: false,
            success: false,
            parity_match: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// Benchmark collector
// ---------------------------------------------------------------------------

/// Collects execution metrics for comparison between paths.
///
/// Accumulates individual runs and provides summary statistics.
#[derive(Debug, Default)]
pub struct BenchmarkCollector {
    /// All collected metrics
    pub metrics: Vec<ExecutionMetrics>,
}

impl BenchmarkCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self {
            metrics: Vec::new(),
        }
    }

    /// Add a metric to the collector.
    pub fn record(&mut self, metric: ExecutionMetrics) {
        self.metrics.push(metric);
    }

    /// Count of successful executions.
    pub fn success_count(&self) -> usize {
        self.metrics.iter().filter(|m| m.success).count()
    }

    /// Count of failed executions.
    pub fn failure_count(&self) -> usize {
        self.metrics.iter().filter(|m| !m.success).count()
    }

    /// Average latency in nanoseconds for successful runs.
    pub fn avg_latency_ns(&self) -> Option<u64> {
        let successful: Vec<_> = self.metrics.iter().filter(|m| m.success).collect();
        if successful.is_empty() {
            return None;
        }
        let total: u64 = successful.iter().map(|m| m.latency_ns).sum();
        Some(total / successful.len() as u64)
    }

    /// Average latency for a specific execution path.
    pub fn avg_latency_for_path(&self, path: ExecutionPath) -> Option<u64> {
        let matching: Vec<_> = self
            .metrics
            .iter()
            .filter(|m| m.success && m.path == path)
            .collect();
        if matching.is_empty() {
            return None;
        }
        let total: u64 = matching.iter().map(|m| m.latency_ns).sum();
        Some(total / matching.len() as u64)
    }

    /// Generate a comparison summary between two paths.
    pub fn comparison_summary(
        &self,
        path_a: ExecutionPath,
        path_b: ExecutionPath,
    ) -> ComparisonSummary {
        let a_latency = self.avg_latency_for_path(path_a);
        let b_latency = self.avg_latency_for_path(path_b);

        let parity_checks: Vec<_> = self
            .metrics
            .iter()
            .filter_map(|m| m.parity_match)
            .collect();

        ComparisonSummary {
            path_a,
            path_b,
            avg_latency_a_ns: a_latency,
            avg_latency_b_ns: b_latency,
            parity_match_count: parity_checks.iter().filter(|&&p| p).count(),
            parity_mismatch_count: parity_checks.iter().filter(|&&p| !p).count(),
        }
    }
}

/// Summary of a comparison between two execution paths.
#[derive(Debug)]
pub struct ComparisonSummary {
    pub path_a: ExecutionPath,
    pub path_b: ExecutionPath,
    pub avg_latency_a_ns: Option<u64>,
    pub avg_latency_b_ns: Option<u64>,
    pub parity_match_count: usize,
    pub parity_mismatch_count: usize,
}

impl std::fmt::Display for ComparisonSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Comparison: {} vs {}", self.path_a, self.path_b)?;
        if let Some(a) = self.avg_latency_a_ns {
            writeln!(f, "  {} avg latency: {}ns", self.path_a, a)?;
        }
        if let Some(b) = self.avg_latency_b_ns {
            writeln!(f, "  {} avg latency: {}ns", self.path_b, b)?;
        }
        writeln!(
            f,
            "  Parity: {} match, {} mismatch",
            self.parity_match_count, self.parity_mismatch_count
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Benchmark runner
// ---------------------------------------------------------------------------

/// Run a single margin preview through the given adapter and capture metrics.
///
/// This is the instrumented wrapper around `MarginPreviewAdapter::compute`.
#[cfg(not(target_arch = "wasm32"))]
pub fn benchmark_compute(
    adapter: &MarginPreviewAdapter,
    input: &MarginPreviewInput,
    path_label: ExecutionPath,
) -> (
    Result<crate::wasm_bindings::MarginPreviewOutput, AdapterError>,
    ExecutionMetrics,
) {
    let start = Instant::now();
    let result = adapter.compute(input);
    let elapsed = start.elapsed();
    let latency_ns = elapsed.as_nanos() as u64;

    let metrics = match &result {
        Ok(output) => {
            let validation_ok = crate::wasm_adapter::validate_output(output).is_ok();
            ExecutionMetrics::success(latency_ns, path_label, validation_ok)
        }
        Err(e) => ExecutionMetrics::failure(latency_ns, path_label, e.to_string()),
    };

    (result, metrics)
}

/// Compare native and boundary paths for the same input.
///
/// Returns metrics for both paths and a parity check result.
#[cfg(not(target_arch = "wasm32"))]
pub fn compare_native_vs_boundary(
    input: &MarginPreviewInput,
    collector: &mut BenchmarkCollector,
) -> bool {
    // Run native path
    let native_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);
    let (native_result, mut native_metrics) =
        benchmark_compute(&native_adapter, input, ExecutionPath::Native);

    // Run boundary path
    let boundary_adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);
    let (boundary_result, mut boundary_metrics) =
        benchmark_compute(&boundary_adapter, input, ExecutionPath::Boundary);

    // Check parity
    let parity = match (&native_result, &boundary_result) {
        (Ok(native_output), Ok(boundary_output)) => native_output == boundary_output,
        _ => false,
    };

    native_metrics.parity_match = Some(parity);
    boundary_metrics.parity_match = Some(parity);

    collector.record(native_metrics);
    collector.record(boundary_metrics);

    parity
}
