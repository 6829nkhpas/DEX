# Phase 17 — Freeze Review

**Status**: ✅ Complete  
**Date**: 2026-04-21  
**Predecessors**: Phase 11-16 (WASM Feasibility through Production Test Expansion)

---

## 1. Frozen Behaviors

The following behavioral boundaries are permanently locked and must not mutate in future development, hardening, or operation.

| Behavior | Description |
|---|---|
| **Authoritative Rust Core** | The Rust services are exclusively authoritative for state mutation, matching, margin enforcement, and settlement. WASM modules and the Web UI are strictly advisory and capable only of previews or display. |
| **Deterministic Arithmetic** | All price, quantity, and balance computations use fixed-point `rust_decimal` arithmetic exclusively. `f64` and JavaScript native numbers are strictly banned from state logic. Rounding rules (e.g. `AwayFromZero` for requirements, `ToZero` for availability) are fixed. |
| **String-Encoded Canonical Data** | `Price`, `Quantity`, `Balance`, and `Timestamp` values remain string-encoded across all layers: WASM boundaries, WebSockets, REST APIs, and UI internal state. No casting to numerical primitive types is permitted. |
| **Silent Fallback Chains** | WASM failures (deserialization bugs, panics/traps, missing files, or validation logic rejections) must gracefully and automatically cascade to the native Rust computation path without failing the parent request or prompting user intervention. |
| **Reducer Purity & Event Sequencing** | All UI states derive from a unidirectional Redux-style store where gaps in monotonic sequence numbers trigger snapshot refresh queries. Out-of-order dupes are deterministically ignored via the boundary window `MAX_SEEN_IDS`. |

---

## 2. Frozen Interfaces

The following internal and public contracts are stable and must not experience breaking schema changes.

- **WASM JSON-In/Out FFI:** The boundary between the WASM execution and the host engine consists solely of deterministic JSON string serialization. Functions must be signature-compatible with the `simulate_order` paradigm (accepting strings, returning strings, devoid of system clocks, local storage, network capabilities, and PRNG access).
- **Core WebSocket Deltas:** The `OrderbookSnapshotPayload`, `OrderbookDeltaPayload`, `TickerDeltaPayload`, `TradePayload`, `AccountSnapshotPayload`, and `AccountDeltaPayload` definitions are frozen.
- **Risk Enums:** Output values defining state (e.g., `RiskLevel`: `Healthy`, `Warning`, `Danger`, `Liquidation`) are exhaustively validated and bounded.
- **Wallet Auth Session:** Nonce generation and signed message payload structures for verifying account ownership are fixed. Decoupling ECDSA signatures from nonces is forbidden.

---

## 3. Remaining Risks

These known edges remain vulnerable to regression if disturbed, though currently structurally contained:

1. **Panic-driven Fallbacks on Unexpected Strings:** The current parsing layer (e.g., `Decimal::from_str`) relies on system panics for structurally invalid strings (negative amounts or division by zero levering). In the WASM boundary, this is safely contained by the `wasmtime` runtime as a recoverable trap, but it represents an architectural quirk.
2. **Snapshot Cascades Under Load:** An event gap safely triggers an `onRequestSnapshot` re-sync, but rapidly shifting unstable network environments could trigger repeated snapshot cascading, leading to back-pressure latency.
3. **Client Clock Skew:** Wallet session expiration (`AuthSession`) handles clock skew rigidly. Significant client-clock disparities might trigger sudden connection failures and un-graceful user logouts during active sessions.

---

## 4. Validation Notes

Each frozen candidate is supported by deterministic testing and formal validation evidence from prior phases.

- **WASM Parity is Proven (Phase 14):** 100 benchmark-driven assertions proved exact equivalent properties and byte-identical JSON outputs comparing native vs boundary execution over 500 batches. Latency p50 tests measure ~128μs.
- **UI State Determinism is Proven (Phase 15):** 392 structural tests evaluated loading, skeleton, and empty states directly mapped to connection and auth permutations ensuring deterministic front-end interactions.
- **Store Mutation Purity is Proven (Phase 16):** Expanding matrices confirmed array manipulation inside `applyOrderbookDelta`, descending bid mapping, ascending ask mapping, and correct 0-quantity level eviction occur strictly within pure functions.

---

## 5. Allowed-Change Policy

To permit continuous improvement without degrading the frozen perimeter, future modifications must align with the following rules:

### Safe Additive Changes (Allowed)
- Adding new pure functions to `wasm-core` using the existing JSON boundary wrapper (e.g. `portfolio_summary` or `simulate_fill`).
- Enhancing CSS animations, typography styling, layout properties, and accessibility (a11y) markers inside existing design tokens.
- Replacing primitive `.unwrap()` and panic bounds with explicit `TryFrom` mapping producing standard `Result<T, E>`.

### Require Revalidation
- Adding new REST API endpoints or expanding GraphQL parameters requires extending the current JSON ABI verification outputs.
- Any change altering the memory alignment logic or optimization flags passed to `wasm-pack` compilation runs must execute the Phase 14 `benchmark_tests` harness to certify latency safety constraints.

### Strictly Forbidden
- Demoting the authoritative standing of native Rust execution backends, or allowing WASM components or UI states to submit raw market delta adjustments.
- Stripping `.to_string()` / `.from_str()` conversions in favor of floats, BigInt, or floating precision mapping inside the application lifecycle.

---

## 6. Final Freeze Status

**Verdict:** 🛳️ **READY FOR HARdenING**

The DEX exchange logic, interface boundaries, and deterministic math behaviors are fully stable. There are no remaining blockages stemming from computation architecture, regression vulnerability, or execution latency constraints. 

All core assumptions have been formally locked. Future development (Phase 18 and beyond) must treat this document as the system's foundational canon.
