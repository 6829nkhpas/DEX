# Store Partition Audit — Phase 15

**Date**: 2026-03-01  
**Scope**: `DexStateStore` multi-symbol isolation, memory scaling, buffer bounding

---

## 1. Is state isolated per symbol?

**YES** — full isolation by design.

### Orderbooks

- Stored in `Map<string, OrderbookState>` keyed by `symbol`.
- Snapshot replaces **only** the target symbol entry (`applyOrderbookSnapshot`).
- Delta merges levels **only** for the event's `payload.symbol`.
- No cross-symbol reference exists in any reducer.

### Tickers

- Stored in `Map<string, TickerState>` keyed by `symbol`.
- Each delta updates only the addressed symbol entry.
- Field-level merge (`applyTickerDelta`) never cross-reads another symbol.

### Trades

- Stored in `Map<string, TradeRecord[]>` keyed by `symbol`.
- `applyTrade` appends to the symbol-local array only.
- Bounded to `MAX_TRADES_PER_SYMBOL = 500` per symbol.

### Account

- Single `AccountState | null`. Not symbol-scoped.
- Orthogonal to market data — no cross-contamination possible.

### Verdict

State is **fully partitioned** per symbol. Adding symbol N never touches data for symbol N-1.

---

## 2. Does memory scale linearly?

**YES** — with bounded constants per symbol.

| Domain       | Per-symbol memory overhead (approximate)                         |
| ------------ | ---------------------------------------------------------------- |
| Orderbook    | 25 bids + 25 asks × 2 strings ≈ 100 tuples ≈ 5 KB                |
| Ticker       | 6 string fields ≈ 0.3 KB                                         |
| Trades       | 500 records × ~200 B ≈ 100 KB                                    |
| SeqMeta      | 1 lastSeq string + Set (up to 10,000 IDs) ≈ 800 KB at saturation |
| Delta buffer | capped at MAX_BUFFER_SIZE = 10,000 events per stream             |

**Per-symbol total (steady state, no gap)**: ~105 KB  
**Per-symbol total (max dedup set saturation)**: ~905 KB

Scaling:

- 10 symbols ≈ 1 MB – 9 MB
- 25 symbols ≈ 2.6 MB – 23 MB
- 50 symbols ≈ 5.3 MB – 45 MB

All constants are bounded. Memory grows **O(n)** in number of symbols.

---

## 3. Are buffers bounded per symbol?

**YES** — with hard cap enforcement.

### Delta buffers

- `deltaBuffers: Map<string, BaseEvent[]>` — keyed by domain key (`source::symbol`).
- Each buffer independently capped at `MAX_BUFFER_SIZE = 10,000`.
- On overflow: buffer is **cleared** and a full snapshot is requested (`sinceSeq: 0`).
- Overflow policy is per-stream, so one symbol overflowing cannot affect another.

### Dedup seenIds

- `SeqMeta.seenIds: Set<string>` — per domain key.
- Capped at `MAX_SEEN_IDS = 10,000` — oldest evicted on overflow (insertion-order iteration).
- Each domain key has its own set — no shared eviction pressure.

### Trade lists

- Per-symbol, bounded to `MAX_TRADES_PER_SYMBOL = 500`.
- Oldest trades pruned via `slice()`.

### Verdict

Every buffer structure is:

1. **Per-symbol** (keyed by domain key).
2. **Hard capped** (overflow triggers eviction or snapshot recovery).
3. **Independent** (one symbol's overflow cannot destabilize another).

---

## 4. Any cross-stream bleed risk?

**NO** — with one observation.

### Domain key construction

```typescript
private domainKey(event: BaseEvent<unknown>): string {
    const payload = event.payload as Record<string, unknown> | null;
    const symbol = payload && typeof payload === "object" && "symbol" in payload
        ? String(payload.symbol)
        : "";
    return symbol ? `${event.source}::${symbol}` : event.source;
}
```

- `market_data::BTC/USDT` and `market_data::ETH/USDT` are distinct keys.
- `trades::BTC/USDT` and `market_data::BTC/USDT` are distinct keys (different source).
- Account events (no symbol) use bare `"account"` key — single stream, no bleed risk.

### Shared sequence space within `market_data`

One important observation: the WS protocol uses a **single sequence counter per subscription** (per channel+params). For `market_data::BTC/USDT`, both orderbook deltas and ticker deltas share the same sequence space. This is **correct by design** — the server sequences them in a single stream per subscription. The store's `domainKey` groups them correctly.

### Potential concern: shared `market_data` domain

If the server were to send orderbook deltas and ticker deltas with **different** sequence counters for the same symbol, the current `domainKey` would merge them. However, the WS protocol spec (§4) confirms a single sequence per subscription, so this is **not a risk** in practice.

### Notification broadcasting

`notifyListeners()` fires for **every** state change regardless of which symbol changed. This is a performance concern for multi-symbol scenarios (every symbol update triggers all listeners), but it is **not a correctness issue** — no state corruption occurs.

**Recommendation**: For >10 symbols, consider per-symbol listener registration to avoid unnecessary re-renders. This is addressed by the `SubscriptionOrchestrator` (Mission 2).

---

## 5. Summary

| Question                    | Answer | Risk Level |
| --------------------------- | ------ | ---------- |
| State isolated per symbol?  | ✅ Yes | None       |
| Memory scales linearly?     | ✅ Yes | Low        |
| Buffers bounded per symbol? | ✅ Yes | None       |
| Cross-stream bleed risk?    | ✅ No  | None       |
| Listener over-notification? | ⚠️ Yes | Medium     |

**Overall**: The store is production-ready for multi-symbol operation. The only scaling concern is listener notification frequency at high symbol counts, which the SubscriptionOrchestrator and per-symbol memoization in MarketGrid address.
