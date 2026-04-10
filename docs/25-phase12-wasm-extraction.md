# Phase 12 — WASM Module Extraction

**Status**: ✅ Complete  
**Date**: 2026-04-10  
**Predecessor**: Phase 11 (WASM Feasibility Audit — `24-phase11-wasm-feasibility.md`)

---

## Summary

Phase 12 extracts the `CrossMarginEngine::simulate_order` compute path into a
WebAssembly module with a clean, explicit JSON-in/JSON-out boundary. The Rust
core remains authoritative. All changes are **additive** — no existing code was
modified except for WASM-compatibility fixes in `types/Cargo.toml` and
`wasm-core/Cargo.toml`.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     MarginPreviewAdapter                        │
│                                                                 │
│  compute(input)                                                 │
│    ├── WasmFeatureFlag::Enabled?                                │
│    │   ├── YES → compute_via_boundary()                         │
│    │   │         ├── JSON serialize input                       │
│    │   │         ├── margin_preview_json(json)                  │
│    │   │         │   ├── Deserialize → internal types           │
│    │   │         │   ├── CrossMarginEngine::simulate_order()    │
│    │   │         │   └── Serialize → JSON output                │
│    │   │         ├── JSON deserialize output                    │
│    │   │         ├── validate_output()                          │
│    │   │         │   ├── OK → return validated result           │
│    │   │         │   └── FAIL → fallback to native              │
│    │   │         └── ERROR → fallback to native                 │
│    │   └── NO → compute_native()                                │
│    │             ├── Direct Rust function call (no JSON)         │
│    │             └── Same computation, same result               │
│    └── return validated MarginPreviewOutput                     │
└─────────────────────────────────────────────────────────────────┘
```

## Deliverables

### New Files

| File | Purpose |
|------|---------|
| `libs/wasm-core/src/wasm_bindings.rs` | WASM boundary — JSON types, `margin_preview_json()`, FFI exports |
| `libs/wasm-core/src/wasm_adapter.rs` | Host adapter — native/boundary dispatch, validation, fallback |
| `libs/wasm-core/src/wasm_tests.rs` | 38 tests — equivalence, input, rejection, fallback, validation |

### Modified Files

| File | Change |
|------|--------|
| `libs/wasm-core/Cargo.toml` | Added `cdylib` crate type, `wasm` feature flag, moved `rand` to dev-deps |
| `libs/wasm-core/src/lib.rs` | Registered new modules |
| `libs/types/Cargo.toml` | Added `uuid` `js` feature for wasm32 target |

## Boundary Specification

### Input (`MarginPreviewInput`)

```json
{
  "account_id": "01939d7f-8e4a-7890-a123-456789abcdef",
  "total_balance": "100000",
  "positions": [{
    "symbol": "BTC/USDT",
    "side": "LONG",
    "size": "2.0",
    "entry_price": "50000",
    "mark_price": "51000",
    "liquidation_price": "49500",
    "initial_margin": "10000",
    "maintenance_margin": "500",
    "leverage": 10,
    "timestamp": 1708123456789000000
  }],
  "order": {
    "symbol": "ETH/USDT",
    "side": "BUY",
    "price": "3000",
    "quantity": "10.0",
    "leverage": 20
  }
}
```

### Output (`MarginPreviewOutput`)

```json
{
  "equity_after": "102000.0",
  "margin_used_after": "11500.0",
  "margin_available_after": "90500.0",
  "margin_ratio_after": "886.95652173",
  "liquidation_price": "2860.50000000",
  "leverage_ratio": "1.27450980",
  "risk_level": "Healthy",
  "has_negative_balance": false
}
```

### Rules

| Rule | Implementation |
|------|---------------|
| All decimals as strings | `Decimal::to_string()` / `Decimal::from_str()` |
| All IDs as UUID strings | `Uuid::parse_str()` → `AccountId::from_uuid()` |
| Deterministic arithmetic | `rust_decimal` fixed-point, no f64 anywhere |
| Sorted iteration | `BTreeMap` for all maps |
| No clock/RNG/IO | `cfg(not(test))` + structural prohibition |

## Feature Gating

```rust
// Runtime control
let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Disabled); // safe default
let adapter = MarginPreviewAdapter::new(WasmFeatureFlag::Enabled);  // boundary path

// Compile-time control
// WASM FFI exports only compiled for wasm32:
#[cfg(target_arch = "wasm32")]
pub mod wasm_ffi { ... }

// Cargo feature flag:
[features]
wasm = []
```

## Validation Layer

Every WASM output passes through `validate_output()` before consumption:

1. **Decimal fields** — all 6 numeric strings must parse as valid `Decimal`
2. **No empty fields** — rejects empty strings
3. **Risk level** — must be one of: `Healthy`, `Warning`, `Danger`, `Liquidation`
4. **Margin consistency** — `margin_used + margin_available ≈ equity` (tolerance: 2×10⁻⁸)

## Fallback Behavior

```
WASM boundary fails → native computation (automatic, silent)
Validation fails    → native computation (automatic, silent)
WASM disabled       → native computation (direct)
```

The native path and boundary path are proven to produce **identical outputs**
for all test vectors (see deterministic equivalence tests).

## Test Coverage

| Category | Count | Tests |
|----------|-------|-------|
| Deterministic equivalence | 4 | native vs boundary, adapter both paths, repeated calls, empty positions |
| Boundary input correctness | 6 | zero balance, max leverage 125x, min quantity, high precision, sell order, multiple positions |
| Invalid input rejection | 9 | malformed JSON, empty JSON, missing fields, bad UUID, non-numeric balance/price/quantity, bad sides |
| Fallback behavior | 4 | disabled flag, enabled path, default flag, invalid input handling |
| Output validation | 6 | valid output, empty equity, invalid decimal, invalid risk level, inconsistent margins, all risk levels |
| Serialization round-trip | 3 | input round-trip, output round-trip, boundary JSON round-trip validated |
| Value verification | 2 | standard input expected values, near-liquidation values |
| **Total WASM tests** | **34** | |
| **Total all tests** | **157** | types crate (53) + wasm-core (104) |

## WASM Build

```bash
# Compile to wasm32-unknown-unknown (produces .wasm)
cd libs/wasm-core
cargo build --target wasm32-unknown-unknown

# Output: target/wasm32-unknown-unknown/debug/wasm_core.wasm (9.0 MB debug)
# Production: cargo build --release --target wasm32-unknown-unknown (much smaller)
```

### WASM FFI Protocol

```
Host → WASM:
  1. wasm_alloc(len) → ptr
  2. Write JSON bytes to ptr
  3. wasm_margin_preview(ptr, len) → result_ptr
  4. Read [4 bytes: u32 LE length][N bytes: JSON]
  5. wasm_dealloc(ptr, len)  // cleanup input
  6. wasm_dealloc(result_ptr, 4 + N)  // cleanup output
```

## What Was NOT Changed

- ❌ No wasm-pack, no JS bindings, no frontend integration
- ❌ No API/WS layer modifications
- ❌ No changes to trading, market data, auth, wallet, replay, or settlement
- ❌ No authoritative logic moved to WASM
- ❌ No existing test modified or removed
- ❌ No runtime dependency added (no wasmtime/wasmer)

## Invariants Preserved

1. **Determinism** — identical inputs produce identical outputs (native ≡ boundary)
2. **WASM is advisory** — all outputs validated before any consumption
3. **Rust core authoritative** — WASM can never submit orders or mutate state
4. **Additive only** — all changes are new files or additive Cargo.toml entries
5. **Feature-gated** — disabled by default, opt-in at runtime
6. **Clock-free** — no `SystemTime`, no `Uuid::now_v7()` called in WASM code
7. **RNG-free** — `rand` moved to dev-deps only
