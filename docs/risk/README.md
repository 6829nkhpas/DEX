# Risk Dashboard — User & Ops Guide

## Overview

The Risk Dashboard (`/risk` route) provides traders and operators with real-time margin visibility, liquidation simulations, and admin safety controls. It is part of the DEX web UI (Phase 17).

## Sections

### 1. Account Summary

Displays aggregated account-level margin metrics:

| Metric             | Description                                    |
| ------------------ | ---------------------------------------------- | ---- | ------------------- |
| **Equity**         | Balance + unrealized PnL across all positions  |
| **Unrealised PnL** | Sum of (mark − entry) × size for all positions |
| **Initial Margin** | Sum of (                                       | size | × entry) / leverage |
| **Maint. Margin**  | Sum of                                         | size | × mark × mm_rate    |
| **Free Margin**    | Equity − total initial margin                  |
| **Margin Ratio**   | Equity / maintenance margin                    |

**Health status colors:**

- 🟢 **Healthy**: margin_ratio ≥ 2.0
- 🟡 **Warning**: 1.5 ≤ margin_ratio < 2.0
- 🟠 **Danger**: 1.1 ≤ margin_ratio < 1.5
- 🔴 **Liquidation**: margin_ratio < 1.1

### 2. Margin Requirements (Positions)

Per-position table showing:

- Symbol, size, entry price, mark price
- Unrealized PnL
- Initial margin required
- Maintenance margin required
- Estimated liquidation price

Mark prices update live from the store's ticker state (driven by WS market data events).

### 3. Liquidation Simulator

Interactive tool for "what-if" analysis:

- **Global Mark Shift**: slider and text input to apply a uniform mark price change across all positions
- **Simulated Metrics**: shows projected PnL, equity, margin ratio, and health after the shift
- **Per-position Breakdown**: individual PnL and margin at the new marks
- **Liquidation Cascade**: ordered list showing which positions would liquidate first and at what mark price

**Keyboard accessible**: slider works with arrow keys, text input accepts Enter, Reset button is focusable.

### 4. Admin Safeties (Stubs)

Toggle switches for operational safety controls:

| Toggle                          | Effect (stub)                                   |
| ------------------------------- | ----------------------------------------------- |
| **Emergency Liquidation Pause** | Halts all liquidations system-wide              |
| **Reduce Leverage Limit**       | Caps max leverage at 10x                        |
| **Increase Margin Buffer**      | Adds 20% to all maintenance margin calculations |

Each toggle:

- Emits a telemetry event via `TelemetryClient`
- Writes to a mock admin config endpoint (`/admin/risk-config`)
- Updates local state (accessible via `getLastMockConfig()`)

## Architecture & Data Flow

```
[WS Market Data] → [Store (tickers)] → [RiskPage]
                                           ↓
                                    [lib/risk/margin.ts]     → Account Summary
                                    [lib/risk/liquidation.ts] → Simulator
                                    [AdminSafeties]          → Telemetry + Mock Config
```

All math uses `decimal.js` with:

- String inputs/outputs (no floating-point)
- `ROUND_UP` for margin calculations (per spec §05.9.2)
- `ROUND_DOWN` for available margin (conservative)

## Model Sources

Calculations follow specifications:

- **Margin**: `/spec/05-margin-methodology.md` — §2.1 (IM), §2.2 (MM), §3.3 (PnL), §3 (margin ratio)
- **Liquidation**: `/spec/06-liquidation-process.md` — §2.1 (trigger), §4.3 (bankruptcy price)
- **Leverage Tiers**: `/spec/05-margin-methodology.md` — §4.1

## Known Limitations

1. **Cross-margin multi-position liquidation** is approximate — solving the exact cascading ODE is deferred.
2. **Portfolio margin (VaR)** mode is not implemented — requires historical data infrastructure.
3. **Concentration risk add-on** is not included in margin calculations.
4. **Positions are mock data** — backend integration pending (wiring to store account state).
5. **Admin toggles are stubs** — they emit telemetry and write to mock, but don't affect backend behavior.
6. **Isolated margin mode** is not supported — all calculations assume cross-margin.

## Model Verification

A verification suite replays golden snapshots and compares computed vs expected results.

- Report: `/apps/web-ui/perf/risk-model-verification.md`
- Source: `/apps/web-ui/src/lib/risk/verification.ts`
- Run: `cd apps/web-ui && npx tsx -e "const v = require('./src/lib/risk/verification'); console.log(v.runVerification())"`

## Ops Notes

### When Risk Alerts Fire

1. **Check margin ratio distribution** on the Risk Dashboard
2. **Use the simulator** to gauge cascade severity at projected price levels
3. **Toggle Emergency Liquidation Pause** if cascading liquidations detected
4. Cross-reference with runbooks:
   - `/ops/runbooks/` — incident response procedures
   - `/spec/06-liquidation-process.md` §10 — edge case handling
   - `/docs/07-incident-response.md` — escalation paths

### Runbook Integration

| Scenario               | Runbook                                | Dashboard Action       |
| ---------------------- | -------------------------------------- | ---------------------- |
| Mass liquidation event | `ops/runbooks/`                        | Toggle Emergency Pause |
| Leverage abuse         | `spec/05-margin-methodology.md §14`    | Toggle Reduce Leverage |
| High volatility        | `spec/06-liquidation-process.md §10.1` | Toggle Margin Buffer   |

### Health Check Procedure

1. Open `/risk` route
2. Verify all sections render with current data
3. Move simulator slider to confirm responsiveness
4. Check console for telemetry events when toggling admin controls
5. Verify `npx tsc --noEmit && npm test` pass

## File Structure

```
apps/web-ui/src/
├── lib/risk/
│   ├── index.ts              # Barrel export
│   ├── margin.ts             # IM, MM, PnL, account metrics
│   ├── liquidation.ts        # Liq price, simulation, cascade
│   └── verification.ts       # Golden snapshot replay engine
├── components/Risk/
│   ├── AccountSummary.tsx     # Aggregated metrics display
│   ├── MarginPositionsPanel.tsx # Per-position margin table
│   ├── LiquidationSimulator.tsx # Interactive what-if simulator
│   └── AdminSafeties.tsx      # Admin toggle stubs
├── pages/
│   └── RiskPage.tsx           # Main risk dashboard page
└── tests/__tests__/
    └── risk-models.test.ts    # 60+ tests covering all models
```

## Testing

- **Unit tests**: All model functions have edge-case and boundary coverage
- **Stress tests**: 1000-iteration determinism, 50-position ticker churn
- **Verification**: 8 golden snapshots with HIGH confidence
- **Accessibility**: ARIA labels, keyboard controls, role attributes

Run: `cd apps/web-ui && npm test`
