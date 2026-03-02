// ---------------------------------------------------------------------------
// RiskPage — Risk dashboard with margin, liquidation preview, and admin tools
// ---------------------------------------------------------------------------
//
// Sections:
//   1. Account Summary — aggregated margin metrics
//   2. Positions List — per-position margin and liquidation details
//   3. Margin Requirements Panel — (embedded in positions list)
//   4. Simulation Panel — interactive mark-shift simulator
//   5. Admin Safeties — stub toggles for ops
//
// Reads live data from the global store (tickers for mark prices).
// Uses mock/sample positions for demonstration until backend integration.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import { useDexStore } from "../state/StoreProvider";
import { AccountSummary } from "../components/Risk/AccountSummary";
import { MarginPositionsPanel } from "../components/Risk/MarginPositionsPanel";
import { LiquidationSimulator } from "../components/Risk/LiquidationSimulator";
import { AdminSafeties } from "../components/Risk/AdminSafeties";
import type { MarginPosition, MarginParams } from "../lib/risk";

// ---------------------------------------------------------------------------
// Sample / mock data for demonstration
// ---------------------------------------------------------------------------

const SAMPLE_POSITIONS: Array<{ symbol: string; size: string; entry_price: string }> = [
  { symbol: "BTC/USDT", size: "0.5", entry_price: "50000.00" },
  { symbol: "ETH/USDT", size: "10", entry_price: "3200.00" },
  { symbol: "SOL/USDT", size: "-100", entry_price: "120.00" },
];

const SAMPLE_BALANCE = "50000.00";

const DEFAULT_PARAMS: MarginParams = {
  leverage: "10",
  maintenance_margin_rate: "0.005",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const RiskPage: React.FC = () => {
  const { state } = useDexStore();

  // Enrich sample positions with live mark prices from store
  const positions: MarginPosition[] = useMemo(() => {
    return SAMPLE_POSITIONS.map((p) => {
      const ticker = state.tickers.get(p.symbol);
      return {
        symbol: p.symbol,
        size: p.size,
        entry_price: p.entry_price,
        mark_price: ticker?.mark_price ?? p.entry_price,
      };
    });
  }, [state.tickers]);

  return (
    <div
      style={{
        maxWidth: 1200,
        margin: "0 auto",
        padding: "24px 16px",
        display: "flex",
        flexDirection: "column",
        gap: 20,
      }}
    >
      <header>
        <h1 style={{ margin: "0 0 4px", fontSize: 24, fontWeight: 700, color: "#f9fafb" }}>
          Risk Dashboard
        </h1>
        <p style={{ margin: 0, fontSize: 13, color: "#9ca3af" }}>
          Monitor margin requirements, simulate liquidation scenarios, and manage safety controls.
        </p>
      </header>

      {/* Section 1: Account Summary */}
      <AccountSummary positions={positions} balance={SAMPLE_BALANCE} params={DEFAULT_PARAMS} />

      {/* Section 2 & 3: Positions with margin requirements */}
      <MarginPositionsPanel positions={positions} balance={SAMPLE_BALANCE} params={DEFAULT_PARAMS} />

      {/* Section 4: Liquidation simulator */}
      <LiquidationSimulator positions={positions} balance={SAMPLE_BALANCE} params={DEFAULT_PARAMS} />

      {/* Section 5: Admin safeties */}
      <AdminSafeties />
    </div>
  );
};
