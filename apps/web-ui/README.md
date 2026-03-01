# DEX Web UI

React-based trading interface for the Distributed Exchange.

## Quick Start

```bash
npm install
npm run dev          # Vite dev server on http://localhost:3000
npm run typecheck    # TypeScript type-check (no emit)
npm test             # Run unit + integration tests
```

## How to Test Order Submission with Mock Server

### 1. Run the dev UI (uses mock event simulation built-in)

```bash
cd apps/web-ui
npm run dev
```

Navigate to `http://localhost:3000/trade`. The MarketPage includes:
- **Orderbook** — live bids/asks from mock WS events
- **TradeTape** — streaming trades
- **TickerPanel** — current price and 24h stats
- **OrderEntry** — submit LIMIT/MARKET orders

### 2. Order Entry Form

| Field | Description |
|-------|-------------|
| Side | BUY or SELL |
| Type | LIMIT (requires price) or MARKET |
| Price | Decimal string, e.g. `50000.00` |
| Quantity | Decimal string, e.g. `1.0` |
| Time-in-Force | GTC, IOC, FOK, or GTD (with date) |

- **Validation**: Uses `decimal.js` — price and quantity must be positive decimals.
- **Debounce**: 500ms cooldown between submissions.
- **Rate Limit**: HTTP 429 shows user-friendly message with 5s cooldown.

### 3. Run Tests

```bash
# Type-check + tests
cd apps/web-ui
npx tsc --noEmit && npm test
```

Test coverage:
- **Unit**: validation rules (decimal parsing), payload composition, error shapes
- **Integration**: REST → WS → Store sync flow, failure paths (400/422/429/500)

### 4. Architecture

```
OrderEntry.tsx
  ├── validateOrder()       — pure validation with decimal.js
  ├── buildCreateOrderRequest() — DTO composition
  ├── handleSubmit()        — calls DexApiClient.createOrder()
  └── Local submitted-orders panel
        └── Syncs with DexStateStore via onStateChange listener
            (WS OrderSubmitted events update authoritative state)
```

No optimistic local-only matching. The store is the single source of truth,
updated only by WS events.
