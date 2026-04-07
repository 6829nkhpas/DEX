# Phase 11 — WASM Feasibility Design

**Status**: Design Complete — Go/No-Go Recommendation Included  
**Spec Version**: 1.0.0  
**Date**: 2026-04-08

---

## 1. Executive Summary

This document evaluates whether WebAssembly (WASM) integration is justified for the DEX, which WASM target to pursue first, and how to integrate it without weakening the Rust-first architecture or any core exchange invariant.

### Verdict: **Conditional GO — `wasm-core` margin/simulation modules**

The `libs/wasm-core` crate (`margin.rs`, `simulation.rs`, `portfolio.rs`, `signing.rs`) is already designed for client-side use. It is:

- **Pure**: no system calls, no clocks, no RNG, no external I/O
- **Deterministic**: fixed-point `Decimal`, `BTreeMap` iteration, HALF_UP/AwayFromZero rounding
- **Isolated**: depends only on `libs/types` (frozen at v1.0.0)
- **Well-tested**: 60+ unit tests with determinism assertions

Compiling `wasm-core` to `wasm32-unknown-unknown` via `wasm-bindgen` is the lowest-risk and highest-value first WASM target. It would let the React/TypeScript UI run margin previews, fill simulations, and signature verification **locally in the browser**, eliminating round-trips to the gateway for preview operations.

> [!IMPORTANT]
> WASM outputs are **advisory only**. They must never become the source of truth for exchange state. All values displayed from WASM must be labeled as estimates, and every action based on WASM output must be validated server-side by the authoritative Rust services.

---

## 2. Codebase WASM Suitability Audit

### 2.1 Module Classification

| Module | Location | Lines | WASM Suitable | Rationale |
|--------|----------|-------|:---:|-----------|
| **Fixed-point types** | `libs/types/src/numeric.rs` | 341 | ✅ | Pure `Decimal` wrappers, no I/O, string-serialized |
| **Margin preview** | `libs/wasm-core/src/margin.rs` | 661 | ✅ | Pure cross-margin engine, no side effects |
| **Order simulation** | `libs/wasm-core/src/simulation.rs` | 603 | ✅ | Fill estimation against mock order books, pure |
| **Portfolio aggregation** | `libs/wasm-core/src/portfolio.rs` | 489 | ✅ | PnL/equity calculations, `BTreeMap`, pure |
| **Signing/verification** | `libs/wasm-core/src/signing.rs` | 511 | ✅ | Ed25519 + SHA-256, deterministic, no RNG in signing path |
| **IDs** | `libs/types/src/ids.rs` | ~540 | ⚠️ | UUID v7 generation requires a clock — read-only use only |
| **Risk engine margin** | `services/risk-engine/src/margin.rs` | 430 | ⛔ | Authoritative calculation — must remain in native Rust service |
| **Risk engine liquidation** | `services/risk-engine/src/liquidation.rs` | 334 | ⛔ | Authoritative liquidation trigger — server-only |
| **Risk engine exposure** | `services/risk-engine/src/exposure.rs` | 220 | ⛔ | Authoritative exposure — server-only |
| **Risk engine validator** | `services/risk-engine/src/validator.rs` | 233 | ⛔ | Pre-trade validation requires live account state |
| **Matching engine** | `services/matching-engine/src/engine.rs` | 343 | ⛔ | HOT PATH, stateful order books, must be native Rust |
| **Matching crossing** | `services/matching-engine/src/matching/` | ~200 | ⛔ | Part of matching engine's stateful pipeline |
| **Vault contract** | `chain/contracts/src/vault.rs` | ~18K | ⛔ | Custody, on-chain trust boundary |
| **Withdrawal queue** | `chain/contracts/src/withdrawal.rs` | ~15K | ⛔ | Settlement lifecycle, on-chain |
| **Commitment store** | `chain/contracts/src/commitment.rs` | 460 | ⛔ | Fraud proofs, admin access control |
| **Contract security** | `chain/contracts/src/security.rs` | ~9K | ⛔ | Access control, admin roles |

### 2.2 Dependency Analysis for `wasm-core`

```
wasm-core
├── types (frozen v1.0.0)
│   ├── rust_decimal (pure, no-std compatible with serde)
│   ├── serde / serde_json (WASM-compatible)
│   ├── uuid (can be made WASM-safe if we avoid v7 generation)
│   ├── chrono (read-only use viable)
│   └── thiserror (WASM-compatible)
├── rust_decimal (serde, serde-str)
├── sha2 (pure, WASM-compatible)
├── ed25519-dalek (pure, WASM-compatible via rand_core)
├── hex (pure, WASM-compatible)
└── rand (dev-only; NOT needed at WASM runtime)
```

All dependencies are WASM-compatible. `rand` is only used in `dev-dependencies` and test fixtures. The `signing.rs` `sign_message()` function uses a deterministic `SigningKey` — no runtime RNG.

### 2.3 Summary

| Category | Count | Details |
|----------|:---:|---------|
| ✅ Suitable for WASM | 5 modules | `margin`, `simulation`, `portfolio`, `signing`, `numeric` |
| ⛔ Must remain native Rust | 11 modules | All `services/*`, `chain/contracts/*` |
| ⚠️ Partial/read-only | 1 module | `ids` (can deserialize but not generate UUIDs in WASM) |

---

## 3. WASM Candidate Module List

### Priority 1 — First WASM Target: `wasm-core`

These four modules are already grouped in `libs/wasm-core` and share identical design philosophies:

#### 3.1 `margin.rs` — Margin Preview Engine

| Property | Value |
|----------|-------|
| **Purpose** | Client-side margin preview for hypothetical orders |
| **Key Types** | `CrossMarginEngine`, `MarginPreview`, `RiskLevel` |
| **Operations** | `simulate_order`, `equity`, `margin_ratio`, `risk_level` |
| **Determinism** | ✅ Fixed-point `Decimal`, HALF_UP rounding, `BTreeMap` |
| **Side Effects** | None (immutable `&self` on simulate_order) |
| **WASM Value** | Eliminates round-trip to risk service for order previews |

#### 3.2 `simulation.rs` — Order Fill Simulation

| Property | Value |
|----------|-------|
| **Purpose** | Estimate fills, slippage, and fees against mock order books |
| **Key Types** | `SimulationEngine`, `SimResult`, `SimFill`, `MockOrderBook` |
| **Operations** | `simulate`, `simulate_batch`, `simulate_cancel`, helpers |
| **Determinism** | ✅ Ordered price levels, fixed-point arithmetic |
| **Side Effects** | None (`&self` only) |
| **WASM Value** | Instant fill preview, slippage estimation, fee calculation |

#### 3.3 `portfolio.rs` — Portfolio Aggregation

| Property | Value |
|----------|-------|
| **Purpose** | Client-side PnL, equity, and balance aggregation |
| **Key Types** | `Portfolio`, `PortfolioSummary` |
| **Operations** | `total_equity`, `total_unrealized_pnl`, `total_balance_value`, `summary` |
| **Determinism** | ✅ `BTreeMap` iteration, 18-dp internal / 8-dp display rounding |
| **Side Effects** | None |
| **WASM Value** | Real-time portfolio updates without server polling |

#### 3.4 `signing.rs` — Transaction Signing

| Property | Value |
|----------|-------|
| **Purpose** | Deterministic message serialization, Ed25519 signing, nonce tracking |
| **Key Types** | `SignableMessage`, `SignedMessage`, `NonceTracker` |
| **Operations** | `sign_message`, `verify_signature`, `validate_and_advance` |
| **Determinism** | ✅ `BTreeMap` payload, SHA-256, deterministic Ed25519 |
| **Side Effects** | `NonceTracker::validate_and_advance` mutates internal state (contained) |
| **WASM Value** | Client-side signing without exposing keys to JS |

### Priority 2 — Future Candidates (Not Phase 11)

| Module | Rationale for Deferral |
|--------|----------------------|
| `libs/types/src/numeric.rs` | Already pulled in transitively. Standalone WASM exposure adds little marginal value. |
| Invariant validators (new) | Could be created as pure WASM functions to validate input before sending. Requires new code. |
| Replay helpers (new) | Event replay / audit verification in browser. Requires design work. |

---

## 4. Module Boundary Specification

### 4.1 `CrossMarginEngine.simulate_order` (recommended first export)

```
┌──────────────────────────────┐
│       WASM MODULE            │
│                              │
│  Input (JSON over FFI):      │
│   {                          │
│     account_id: string,      │
│     total_balance: string,   │ ← decimal-as-string
│     positions: [{            │
│       symbol, side, size,    │
│       entry_price,           │
│       mark_price,            │
│       liquidation_price,     │
│       initial_margin,        │
│       maintenance_margin,    │
│       leverage, timestamp    │
│     }],                      │
│     order: {                 │
│       symbol: string,        │
│       side: "BUY"|"SELL",    │
│       price: string,         │ ← decimal-as-string
│       quantity: string,      │ ← decimal-as-string
│       leverage: u8           │
│     }                        │
│   }                          │
│                              │
│  Output (JSON over FFI):     │
│   {                          │
│     equity_after: string,    │
│     margin_used_after: str,  │
│     margin_available: str,   │
│     margin_ratio: string,    │
│     liquidation_price: str,  │
│     leverage_ratio: string,  │
│     risk_level: string,      │
│     has_negative_balance: b  │
│   }                          │
│                              │
│  Allowed Side Effects:       │
│   ❌ Network calls            │
│   ❌ Clock/time access        │
│   ❌ RNG                      │
│   ❌ State mutations outside  │
│     module boundary           │
│   ❌ Wallet trust decisions   │
│   ✅ Memory allocation       │
│   ✅ Logging to console      │
│                              │
│  Forbidden Outputs:          │
│   ❌ Direct state mutation   │
│   ❌ Order submission        │
│   ❌ Balance changes         │
│                              │
│  Determinism:                │
│   ✅ Same inputs → same out  │
│   ✅ Decimal fixed-point     │
│   ✅ No floating-point       │
│   ✅ BTreeMap iteration ord  │
│                              │
│  Validation Before Use:      │
│   All outputs are advisory.  │
│   Server-side Rust risk      │
│   engine is authoritative.   │
└──────────────────────────────┘
```

### 4.2 `SimulationEngine.simulate`

| Property | Contract |
|----------|----------|
| **Input** | `{ bids: PriceLevel[], asks: PriceLevel[], fee_tier: FeeTier, order: SimOrder }` |
| **Output** | `SimResult { fills, filled_qty, unfilled_qty, avg_price, slippage, fee, total_cost, is_fully_filled }` |
| **Side Effects** | None (read-only against provided book snapshot) |
| **Forbidden** | Accessing live order book, submitting real orders |
| **Validation** | WASM result is a UI preview only; actual execution goes through matching engine |

### 4.3 `Portfolio.summary`

| Property | Contract |
|----------|----------|
| **Input** | `{ account_id, balances: {asset→Balance}[], positions: {symbol→Position}[], prices: {symbol→Price}[] }` |
| **Output** | `PortfolioSummary { total_equity, total_balance_value, total_unrealized_pnl, total_realized_pnl, position_count, asset_count }` |
| **Side Effects** | None |
| **Forbidden** | Writing to any authoritative store |
| **Validation** | Display-only; server is authoritative for actual balances |

### 4.4 `sign_message` / `verify_signature`

| Property | Contract |
|----------|----------|
| **Input** | `{ action, payload: BTreeMap, timestamp: i64, nonce: u64, signing_key_bytes: [u8;32] }` |
| **Output** | `SignedMessage { message, signature_hex, public_key_hex }` |
| **Side Effects** | None (stateless) |
| **Forbidden** | Storing private keys persistently, making trust decisions, broadcasting |
| **Validation** | Server always re-verifies the signature before processing |

---

## 5. WASM Safety Rules

These rules are **non-negotiable** and must be enforced at the crate, FFI, and adapter layers:

### 5.1 Determinism Rules

1. **No floating-point arithmetic** — all monetary values use `rust_decimal::Decimal`
2. **No system clock access** — timestamps must be passed in as explicit arguments
3. **No PRNG or RNG** — `rand` must not be linked in the WASM build; use feature-gated exclusion
4. **Sorted iteration only** — all maps must be `BTreeMap`, never `HashMap`
5. **Fixed rounding strategies** — `MidpointAwayFromZero` for display, `AwayFromZero` for margin requirements, `ToZero` for available margin
6. **String-encoded decimals** — Price, Quantity, Timestamp always serialized as strings across the FFI boundary

### 5.2 State Rules

7. **No hidden state** — WASM module holds no persistent state across calls; each call receives all required inputs
8. **No direct mutation of exchange state** — WASM outputs are advisory; they travel back to JS, and any action (order placement, cancellation) goes through the normal API→Gateway→Rust pipeline
9. **Fallback to server** — if WASM is unavailable (load failure, browser incompatibility), the UI must fall back to server-side calculation APIs

### 5.3 Security Rules

10. **No network calls from WASM** — the module must not import any WASI networking or fetch APIs
11. **No wallet trust decisions** — WASM signs messages but never decides whether to trust a wallet, approve a withdrawal, or authorize a trade
12. **Key isolation** — signing keys passed to WASM must remain in memory only for the duration of the call; WASM must not persist them
13. **All signatures re-verified server-side** — the authoritative Rust services always verify every signed message regardless of WASM verification

### 5.4 Build Rules

14. **No WASI imports** — target `wasm32-unknown-unknown` (browser sandbox), not `wasm32-wasi`
15. **No `std::time`** — guarded by `#[cfg(not(target_arch = "wasm32"))]`
16. **No `std::net`** — guarded by cfg
17. **Feature-gated `rand`** — `rand` dependency only active under `[dev-dependencies]` or a `test` feature

---

## 6. Integration Model

### 6.1 Recommended Architecture

```
┌────────────────────────────────────────────────────────────┐
│                     Browser (React/TS UI)                  │
│                                                            │
│  ┌──────────────┐    ┌─────────────────────────────────┐   │
│  │  UI Component │←──│  wasm-adapter.ts (JS adapter)   │   │
│  │  (React)      │   │  - loads .wasm module           │   │
│  │              │   │  - marshals JSON ↔ FFI          │   │
│  │  Shows       │   │  - catches WASM panics          │   │
│  │  "estimate"  │   │  - implements fallback path     │   │
│  │  badge       │   └───────┬─────────────────────────┘   │
│  └──────────────┘          │                              │
│                             ▼                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │            wasm-core.wasm (Rust→WASM)               │   │
│  │                                                     │   │
│  │  #[wasm_bindgen]                                    │   │
│  │  pub fn margin_preview(json: &str) → String         │   │
│  │  pub fn simulate_fill(json: &str) → String          │   │
│  │  pub fn portfolio_summary(json: &str) → String      │   │
│  │  pub fn sign_message(json: &str, key: &[u8]) → Str  │   │
│  │  pub fn verify_signature(json: &str) → bool         │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                            │
│  ─── Authoritative path (unchanged) ────────────────────── │
│  │                                                        │
│  │  API calls / WebSocket ──→ Gateway ──→ Rust services   │
│  │  (orders, cancels, queries, settlements)               │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

### 6.2 FFI Contract

```rust
// In libs/wasm-core/src/wasm_bindings.rs (new file, additive)
use wasm_bindgen::prelude::*;

/// Margin preview — pure, deterministic, advisory only.
#[wasm_bindgen]
pub fn margin_preview(input_json: &str) -> Result<String, JsValue> {
    // 1. Deserialize input
    // 2. Construct CrossMarginEngine
    // 3. Call simulate_order()
    // 4. Serialize MarginPreview to JSON string
    // 5. Return string (all decimals as string-encoded)
}
```

### 6.3 JS/TS Adapter Pattern

```typescript
// apps/web-ui/src/lib/wasm-adapter.ts (new file)

let wasmModule: WasmCore | null = null;

export async function initWasm(): Promise<void> {
  try {
    const mod = await import("../../wasm-core/pkg/wasm_core.js");
    await mod.default(); // init WASM
    wasmModule = mod;
  } catch {
    console.warn("WASM unavailable — falling back to server calculations");
  }
}

export function marginPreview(input: MarginPreviewInput): MarginPreviewResult | null {
  if (!wasmModule) return null; // fallback signal
  try {
    const json = wasmModule.margin_preview(JSON.stringify(input));
    return JSON.parse(json);
  } catch {
    return null; // fallback signal
  }
}
```

### 6.4 Fallback Path

Every WASM-backed calculation **must** have a fallback:

| Trigger | Behavior |
|---------|----------|
| `wasmModule === null` (load failed) | UI calls server API for margin preview |
| WASM function throws / panics | Adapter returns `null`, UI calls server API |
| Result validation fails | UI discards WASM result, calls server API |
| Browser doesn't support WASM | Detected at init; adapter stays `null` |

### 6.5 Build Pipeline Addition

```
# New Makefile target (additive)
wasm-build:
    cd libs/wasm-core && \
    wasm-pack build --target web --release \
      --out-dir ../../apps/web-ui/wasm-core/pkg \
      -- --features wasm
```

This produces `wasm_core_bg.wasm` + `wasm_core.js` (ES module glue).

### 6.6 Validation Layer

Between WASM output and any display in the UI:

```typescript
function validateMarginPreview(result: unknown): MarginPreviewResult | null {
  // 1. Check all required fields present
  // 2. Check all numeric fields are valid decimal strings
  // 3. Check equity_after > 0 or flag warning
  // 4. Check risk_level is one of: Healthy, Warning, Danger, Liquidation
  // 5. Reject if any field is NaN, Infinity, or numeric (must be string)
  return valid ? parsed : null;
}
```

---

## 7. Testing Plan

### 7.1 Deterministic Output Equivalence

| Test | Method | Pass Criteria |
|------|--------|---------------|
| WASM vs native margin preview | Run identical inputs through native `cargo test` and WASM (Node.js harness) | Byte-identical JSON output |
| WASM vs native simulation | Same mock order book, same order | All fields match exactly |
| WASM vs native portfolio | Same positions and prices | Equity, PnL, balance value match |
| WASM vs native signing | Same keypair, same message | Identical signature hex strings |

### 7.2 Boundary Input Coverage

| Input Class | Test Cases |
|-------------|------------|
| Zero balance | `total_balance = "0"`, expect `has_negative_balance = true` for any order |
| Max leverage (125x) | Verify IM = position_value / 125, MM = tier rate |
| Minimum quantity | `"0.00000001"` — verify no panic, no precision loss |
| Maximum precision | `"99999999999999999.999999999999999999"` — verify Decimal handles it |
| Empty order book | Simulation returns `is_fully_filled = false`, all zeros |
| Single price level, exact fill | Fill qty = level qty, no remainder |
| Negative PnL, Liquidation zone | Mark price below entry (LONG), margin_ratio < 1.1 |

### 7.3 Invalid Input Rejection

| Input | Expected Behavior |
|-------|-------------------|
| Negative price string | Deserialization error or panic caught by adapter |
| Non-numeric string in price | `serde_json` error, adapter returns `null` |
| Missing `side` field | Deserialization error |
| `leverage = 0` | Assertion panic caught by `catch_unwind` or adapter |
| Malformed JSON | `serde_json` parse error, adapter returns `null` |

### 7.4 Replay Consistency

1. Record a sequence of 1000 `(order, book_snapshot)` pairs from test fixtures
2. Run them through native Rust and WASM (Node.js)
3. Compare output arrays — must be byte-identical

### 7.5 Benchmark Plan

| Metric | Tool | Target |
|--------|------|--------|
| Margin preview latency (WASM) | `performance.now()` in browser | < 500μs per call |
| Simulation 8-level book (WASM) | Same | < 200μs per call |
| Portfolio summary, 10 positions (WASM) | Same | < 100μs per call |
| Signing (Ed25519) in WASM | Same | < 1ms per call |
| Native equivalent (baseline) | `cargo bench` / `criterion` | Capture as throughput baseline |
| WASM `.wasm` file size | `wc -c` | < 500 KB after `wasm-opt -O3` |

### 7.6 Regression Safety

- All existing Rust tests (`cargo test --workspace`) must continue to pass with no modification
- WASM build is additive: feature-gated behind `--features wasm`
- CI runs `cargo test` (native) and `wasm-pack test --node` (WASM) in parallel

---

## 8. Risks and Mitigations

### 8.1 Risk Matrix

| Risk | Severity | Probability | Mitigation |
|------|:--------:|:-----------:|------------|
| WASM output diverges from native | **Critical** | Low | Byte-identical JSON comparison in CI; shared test vectors |
| User trusts WASM estimate as authoritative | **Critical** | Medium | UI labels all WASM results as "estimate"; server re-validates on submit |
| `rust_decimal` behaves differently in WASM | **High** | Very Low | Same crate, same code path; no platform-specific branches. Verified by determinism tests |
| WASM build bloats `.wasm` file | **Medium** | Medium | `wasm-opt -O3`, dead code elimination, profile bundle size |
| Ed25519 key leakage from WASM memory | **High** | Low | Keys zeroed after signing call; WASM linear memory is sandboxed by browser |
| `rand` accidentally linked in WASM | **High** | Low | `#[cfg(not(target_arch = "wasm32"))]` guard on all `rand` imports; CI lint |
| Browser compatibility (old Safari, etc.) | **Low** | Low | WASM supported in all modern browsers since 2017; fallback path covers edge cases |
| `chrono` pulls in `js-sys`/`wasm-bindgen-time` | **Medium** | Medium | Audit `chrono` features; if needed, replace with explicit timestamp parameter passing (already the pattern) |

### 8.2 Architectural Invariants Preserved

| Invariant | Impact of WASM | Status |
|-----------|---------------|:------:|
| Rust core is authoritative | WASM is advisory only; never writes to exchange state | ✅ Preserved |
| Deterministic replay | Same code compiled to both targets; test vectors ensure equivalence | ✅ Preserved |
| String-encoded decimals | FFI boundary uses JSON strings; no `f64` conversion | ✅ Preserved |
| String-encoded timestamps | Passed as `i64` nanos in JSON; never converted to JS `Date` | ✅ Preserved |
| Monotonic sequences | WASM does not generate sequences; server-only | ✅ Preserved |
| Event log integrity | WASM does not write events | ✅ Preserved |
| Matching engine HOT PATH | Untouched; remains native Rust service | ✅ Preserved |
| Contract custody layer | Untouched; remains native Rust | ✅ Preserved |
| Frontend is control surface only | WASM adds computation but not authority; UI remains display/input only | ✅ Preserved |

---

## 9. Acceptance Criteria for Proceeding to Implementation

### 9.1 Checklist

| # | Criterion | Status |
|---|-----------|:------:|
| 1 | WASM use case is clearly justified | ✅ Client-side previews eliminate server round-trips |
| 2 | Module boundaries are explicit | ✅ JSON-in, JSON-out, no side effects |
| 3 | Fallback behavior is defined | ✅ `null` return → server API fallback |
| 4 | Determinism is preserved | ✅ Same Decimal + BTreeMap code path, with CI verification |
| 5 | Integration risk is low | ✅ Additive only; feature-gated; no existing code changes |
| 6 | No core exchange invariant is weakened | ✅ See §8.2 |
| 7 | Test plan covers equivalence | ✅ Byte-identical JSON comparison |
| 8 | Benchmark targets defined | ✅ See §7.5 |

### 9.2 Implementation Preconditions

Before writing implementation code:

1. `wasm-pack` must be added to the CI toolchain
2. `wasm-bindgen` + `serde-wasm-bindgen` added to `libs/wasm-core/Cargo.toml` under `[target.'cfg(target_arch = "wasm32")'.dependencies]`
3. `rand` must be gated: `rand = { version = "0.8", optional = true }` with `default-features = false`
4. `libs/types` must be audited for any hidden `std::time` or `HashMap` usage (current audit: clean)
5. A shared test vector JSON file must be created from existing unit tests

---

## 10. Recommended First WASM Target

### `CrossMarginEngine::simulate_order`

**Why this function first:**

1. **Highest user-facing value** — margin preview is the #1 pre-trade information need
2. **Purest boundary** — `&self` method, immutable, returns a new struct
3. **Already tested** — 14 dedicated unit tests including determinism assertion
4. **Smallest surface** — single function with well-defined JSON contract
5. **Validates the full pipeline** — if this works, `simulation`, `portfolio`, and `signing` follow the same pattern

**What the user gains:**

| Before WASM | After WASM |
|-------------|------------|
| User clicks "Preview" → HTTP request → gateway → risk service → response → UI renders | User types order params → WASM computes locally in < 500μs → UI renders instantly |
| Network-dependent, ~50–200ms latency | Offline-capable, < 1ms latency |
| Server load per preview | Zero server load for previews |

---

## 11. Final Recommendation

| Dimension | Assessment |
|-----------|------------|
| **Justified?** | Yes — eliminates network latency for pre-trade previews |
| **Safe?** | Yes — advisory only, fallback path, no state mutation |
| **Deterministic?** | Yes — same Decimal/BTreeMap code, CI-verified equivalence |
| **Reversible?** | Yes — feature-gated, additive, can be removed with zero impact |
| **Risk to existing system?** | Negligible — no existing code changes required |

> [!TIP]
> **Recommendation: Proceed to Phase 11 implementation.**
>
> Begin with `margin_preview` as the first WASM export. Follow the integration model in §6, enforce safety rules in §5, and validate with the test plan in §7. The codebase is already structured for this — `wasm-core` was designed as a client-side compute layer from the start.
