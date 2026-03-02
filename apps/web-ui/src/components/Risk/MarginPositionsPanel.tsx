// ---------------------------------------------------------------------------
// MarginPositionsPanel — per-position margin details with live mark prices
// ---------------------------------------------------------------------------
//
// Shows: size, entry price, mark price, unrealised PnL, initial margin,
// maintenance margin, and liquidation price for each position.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import Decimal from "decimal.js";
import {
  calculateInitialMargin,
  calculateMaintenanceMargin,
  computeUnrealizedPnl,
  computeLiquidationPrice,
} from "../../lib/risk";
import type { MarginPosition, MarginParams, LiquidationAccount } from "../../lib/risk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MarginPositionsPanelProps {
  positions: MarginPosition[];
  balance: string;
  params: MarginParams;
}

interface EnrichedPosition {
  symbol: string;
  size: string;
  entry_price: string;
  mark_price: string;
  pnl: string;
  initial_margin: string;
  maintenance_margin: string;
  liquidation_price: string;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MarginPositionsPanel: React.FC<MarginPositionsPanelProps> = ({
  positions,
  balance,
  params,
}) => {
  const enriched = useMemo(() => {
    const account: LiquidationAccount = { balance, positions };

    return positions.map((pos): EnrichedPosition => {
      const pnl = computeUnrealizedPnl(pos);
      const im = calculateInitialMargin(pos, params);
      const mm = calculateMaintenanceMargin(pos, params);
      const liq = computeLiquidationPrice(pos, account, params);

      return {
        symbol: pos.symbol,
        size: pos.size,
        entry_price: pos.entry_price,
        mark_price: pos.mark_price,
        pnl,
        initial_margin: im,
        maintenance_margin: mm,
        liquidation_price: liq.liquidation_price,
      };
    });
  }, [positions, balance, params]);

  return (
    <div
      role="region"
      aria-label="Positions with margin details"
      style={{
        background: "#111827",
        border: "1px solid #374151",
        borderRadius: 8,
        padding: 16,
        fontFamily: "Inter, system-ui, sans-serif",
        color: "#e5e7eb",
      }}
    >
      <h3 style={{ margin: "0 0 12px", fontSize: 16, fontWeight: 600 }}>
        Margin Requirements
        {enriched.length > 0 && (
          <span style={{ marginLeft: 8, fontSize: 12, color: "#9ca3af", fontWeight: 400 }}>
            ({enriched.length} position{enriched.length !== 1 ? "s" : ""})
          </span>
        )}
      </h3>

      {enriched.length === 0 && (
        <div style={{ color: "#6b7280", fontSize: 13 }}>No open positions.</div>
      )}

      {enriched.length > 0 && (
        <div style={{ overflowX: "auto" }}>
          <table
            style={{ width: "100%", fontSize: 12, borderCollapse: "collapse", whiteSpace: "nowrap" }}
            aria-label="Position margin table"
          >
            <thead>
              <tr style={{ borderBottom: "1px solid #374151" }}>
                <th style={thStyle}>Symbol</th>
                <th style={thStyle}>Size</th>
                <th style={thStyle}>Entry</th>
                <th style={thStyle}>Mark</th>
                <th style={thStyle}>Unreal. PnL</th>
                <th style={thStyle}>Initial Margin</th>
                <th style={thStyle}>Maint. Margin</th>
                <th style={thStyle}>Liq. Price</th>
              </tr>
            </thead>
            <tbody>
              {enriched.map((pos) => {
                const pnlNum = parseFloat(pos.pnl);
                const isLong = parseFloat(pos.size) > 0;
                return (
                  <tr key={pos.symbol + pos.size} style={{ borderBottom: "1px solid #1f2937" }}>
                    <td style={tdStyle}>{pos.symbol}</td>
                    <td style={{ ...tdStyle, color: isLong ? "#10b981" : "#ef4444", fontWeight: 600 }}>
                      {pos.size}
                    </td>
                    <td style={tdStyle}>{fmt(pos.entry_price)}</td>
                    <td style={tdStyle}>{fmt(pos.mark_price)}</td>
                    <td
                      style={{
                        ...tdStyle,
                        color: pnlNum > 0 ? "#10b981" : pnlNum < 0 ? "#ef4444" : "#9ca3af",
                        fontWeight: 600,
                      }}
                    >
                      {pnlNum > 0 ? "+" : ""}
                      {fmt(pos.pnl)}
                    </td>
                    <td style={tdStyle}>{fmt(pos.initial_margin)}</td>
                    <td style={tdStyle}>{fmt(pos.maintenance_margin)}</td>
                    <td style={{ ...tdStyle, color: "#f59e0b" }}>{fmt(pos.liquidation_price)}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function fmt(val: string): string {
  const num = parseFloat(val);
  if (isNaN(num)) return val;
  return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "6px 8px",
  color: "#9ca3af",
  fontWeight: 500,
};

const tdStyle: React.CSSProperties = {
  padding: "6px 8px",
  fontFamily: "monospace",
};
