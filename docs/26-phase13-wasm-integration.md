# Phase 13 — WASM Integration Layer

**Status**: ✅ Complete  
**Date**: 2026-04-13  
**Predecessor**: Phase 12 (WASM Module Extraction — `25-phase12-wasm-extraction.md`)

---

## 1. What Was Integrated

Phase 13 adds the host-side integration layer that loads, executes, validates,
and falls back from the WASM module extracted in Phase 12. The system can now
choose between WASM and native Rust execution without changing exchange behavior.

### New Files

| File | Purpose |
|------|---------|
| `libs/wasm-core/src/wasm_host.rs` | Host-side WASM runtime via wasmtime — loads `.wasm`, manages memory, executes FFI protocol |
| `libs/wasm-core/src/wasm_bench.rs` | Lightweight benchmarking hooks — ExecutionMetrics, BenchmarkCollector, comparison utilities |
| `libs/wasm-core/src/wasm_host_tests.rs` | 42 Phase 13 integration tests covering all execution paths |

### Modified Files

| File | Change |
|------|--------|
| `libs/wasm-core/Cargo.toml` | Added `wasm-host` feature, `wasmtime = "40"` optional dependency |
| `libs/wasm-core/src/lib.rs` | Registered `wasm_host`, `wasm_bench`, `wasm_host_tests` modules with feature gates |
| `libs/wasm-core/src/wasm_adapter.rs` | Added `WasmExecutionMode` enum, `WasmRuntime` dispatch, `RuntimeError` variant, full fallback chain |

---

## 2. How Selection Works

### Execution Modes

The adapter now supports three execution modes via `WasmExecutionMode`:

```rust
// Mode 1: Native Rust (fastest, always available, safest default)
let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);

// Mode 2: Boundary path (same-process JSON FFI — tests WASM code path)
let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Boundary);

// Mode 3: Actual WASM runtime execution (requires wasm-host feature)
let runtime = Arc::new(WasmRuntime::from_file(Path::new("wasm_core.wasm"))?);
let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime));
```

### Feature Gating

| Feature | Compile Target | Purpose |
|---------|---------------|---------|
| `wasm` | Marker for WASM-compiled code | WASM FFI exports, cfg gates |
| `wasm-host` | Host that runs WASM | Enables wasmtime dependency, `WasmRuntime` type |
| *(none)* | Default build | Native + Boundary paths only |

### Backward Compatibility

The Phase 12 `WasmFeatureFlag` interface is fully preserved:

```rust
// Phase 12 — still works exactly as before
let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled);
let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);
```

---

## 3. How Fallback Works

```
WasmRuntime mode:
  WASM execution OK → validate → OK → return result
  WASM execution OK → validate → FAIL → native computation
  WASM execution FAIL → native computation
  WASM deserialization FAIL → native computation
  WASM serialization FAIL → return input error

Boundary mode:
  Boundary execution OK → validate → OK → return result
  Boundary execution OK → validate → FAIL → native computation
  Boundary execution FAIL → native computation

Native mode:
  Native computation OK → return result
  Native computation FAIL → return error (no further fallback)
```

Every failure in the WASM path results in a silent, automatic fallback to
native Rust computation. The caller never needs to know which path produced
the result. Both paths are proven to produce identical output for all
test vectors.

### Failure scenarios covered:

| Scenario | Behavior |
|----------|----------|
| WASM binary not available | `WasmRuntime::from_file` returns error; caller uses Native mode |
| WASM binary invalid/corrupt | `WasmRuntime::new` returns `ModuleCompilation` error |
| WASM execution panics/traps | wasmtime catches trap → `Execution` error → fallback to native |
| WASM output malformed | `serde_json` deserialization fails → fallback to native |
| WASM output fails validation | `validate_output()` rejects → fallback to native |
| Feature flag disabled | Direct native computation |

---

## 4. How Validation Works

Every output — regardless of source — passes through `validate_output()` before
it can affect any downstream logic:

1. **Decimal parsing**: All 6 numeric string fields must parse as valid `Decimal`
2. **Non-empty**: No numeric field may be empty
3. **Risk level**: Must be one of `Healthy`, `Warning`, `Danger`, `Liquidation`
4. **Margin consistency**: `margin_used + margin_available ≈ equity` (tolerance: 2×10⁻⁸)
5. **No NaN/Infinity**: Rejected at the Decimal parse step (Decimal doesn't support these)

The validation layer is unchanged from Phase 12 — it applies equally to native,
boundary, and WASM runtime outputs.

---

## 5. What Tests Were Added

### Phase 13 Tests (42 new tests)

| Category | Count | Tests |
|----------|-------|-------|
| Execution mode selection | 5 | Native valid, Boundary valid, default mode, empty positions both modes |
| Deterministic equivalence | 6 | Native vs Boundary (3 inputs), multi-position, repeated calls (2 modes) |
| Backward compatibility | 4 | Phase 12 Disabled/Enabled flags, default flag, P12 vs P13 equivalence |
| Fallback behavior | 4 | Invalid UUID, non-numeric balance, invalid side, boundary fallback |
| Output validation | 5 | Tampered equity, NaN, Infinity, computed output, boundary output |
| Value verification | 2 | Standard values native mode, standard values boundary mode |
| Benchmarking hooks | 5 | Metrics capture, boundary metrics, collector stats, comparison, failure metrics |
| WASM runtime (feature-gated) | 8 | Invalid bytes, empty bytes, missing file, execution, equivalence, repeats, fallback, benchmarks |
| Regression safety | 3 | Boundary function, native engine, validation rules |

### Total Test Counts

| Build | Tests |
|-------|-------|
| Without `wasm-host` feature | **138** (104 Phase 12 + 34 Phase 13 non-runtime) |
| With `wasm-host` feature | **146** (138 + 8 WASM runtime tests) |

### All tests pass:

```
test result: ok. 146 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## 6. Benchmarking Hooks

### ExecutionMetrics

Captured per computation:

```rust
ExecutionMetrics {
    latency_ns: u64,        // Wall-clock time
    path: ExecutionPath,     // Native | Boundary | WasmRuntime | FallbackToNative
    validation_ok: bool,     // Whether output passed validation
    success: bool,           // Whether computation succeeded
    parity_match: Option<bool>, // Whether output matches reference
    error: Option<String>,   // Error message if failed
}
```

### BenchmarkCollector

Accumulates metrics and provides:
- `success_count()` / `failure_count()`
- `avg_latency_ns()` — overall average
- `avg_latency_for_path(path)` — per-path average
- `comparison_summary(path_a, path_b)` — side-by-side comparison

### Compare function

```rust
// Run same input through native and boundary, check parity
let parity = compare_native_vs_boundary(&input, &mut collector);
assert!(parity); // true = identical output
```

---

## 7. Risks and Mitigations

| Risk | Severity | Mitigation |
|------|:--------:|------------|
| wasmtime dependency size | Medium | Feature-gated behind `wasm-host`; only linked when explicitly enabled |
| WASM output diverges from native | Critical | 6 deterministic equivalence tests; same `CrossMarginEngine::simulate_order` code path |
| WASM runtime trap (panic) | Medium | Caught by wasmtime; automatic fallback to native |
| Memory leak in WASM calls | Low | Dealloc calls in cleanup path; fresh Store per call (no state leakage) |
| wasmtime version compatibility | Low | Pinned to v40 (MSRV 1.89); documented in Cargo.toml |
| Regression in existing flows | Critical | All 104 Phase 12 tests unchanged and pass; 34 new non-runtime tests pass without wasm-host |

---

## 8. Final Status

| Criterion | Status |
|-----------|:------:|
| System executes compute path through WASM when enabled | ✅ |
| System safely falls back to native when WASM unavailable/disabled | ✅ |
| Validation prevents unsafe outputs from touching exchange state | ✅ |
| Rust core remains authoritative | ✅ |
| Tests pass for both WASM and native paths | ✅ 146/146 |
| Existing DEX behavior unchanged outside integration boundary | ✅ |
| Feature-gated, additive, reversible | ✅ |
| No browser/TypeScript integration | ✅ Not introduced |
| No wasm-pack | ✅ Not used |
| No floating-point drift | ✅ All Decimal |
| Decimals and timestamps string-encoded | ✅ |
| WASM module pure and deterministic | ✅ |

### Build/Release

```bash
# Default build (no WASM runtime)
cargo build -p wasm-core

# Build with WASM host runtime
cargo build -p wasm-core --features wasm-host

# Build the WASM module itself
cargo build -p wasm-core --target wasm32-unknown-unknown --release

# Run all tests
cargo test -p wasm-core --lib --features wasm-host

# Enable/disable at runtime
let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::Native);    // off
let adapter = MarginPreviewAdapter::with_mode(WasmExecutionMode::WasmRuntime(runtime)); // on
```

### What Was NOT Changed

- ❌ No changes to trading, market data, auth, wallet, replay, or settlement
- ❌ No authoritative logic moved to WASM
- ❌ No existing test modified or removed
- ❌ No browser bundling or TypeScript glue
- ❌ No wasm-pack usage
- ❌ No floating-point arithmetic
- ❌ No services modified (gateway, matching-engine, risk-engine, etc.)
