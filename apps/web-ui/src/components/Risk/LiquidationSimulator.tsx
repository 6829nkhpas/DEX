// ---------------------------------------------------------------------------
// LiquidationSimulator — interactive mark price shift simulator
// ---------------------------------------------------------------------------
//
// Provides:
//   - A range slider (and text input) to change global mark price delta
//   - Per-symbol mark override inputs
//   - Timeline of which positions liquidate at what mark shift
//   - Aggregated cascade estimates
//
// Keyboard accessible: slider, inputs, and tab navigation.
// ---------------------------------------------------------------------------

import React, { useState, useMemo, useCallback } from "react";
import Decimal from "decimal.js";
import {
  simulateMarkChange,
  estimateLiquidationCascade,
} from "../../lib/risk";
import type { MarginPosition, MarginParams, LiquidationAccount, CascadeEntry } from "../../lib/risk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface LiquidationSimulatorProps {
  positions: MarginPosition[];
  balance: string;
  params: MarginParams;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const LiquidationSimulator: React.FC<LiquidationSimulatorProps> = ({
  positions,
  balance,
  params,
}) => {
  const [markDelta, setMarkDelta] = useState("0");
  const [deltaInput, setDeltaInput] = useState("0");

  // Compute slider range based on position values
  const maxAbsDelta = useMemo(() => {
    if (positions.length === 0) return 10000;
    let maxPrice = new Decimal(0);
    for (const p of positions) {
      const m = new Decimal(p.mark_price).abs();
      if (m.gt(maxPrice)) maxPrice = m;
    }
    // Allow slider to go ±50% of max mark price
    return Math.ceil(maxPrice.times("0.5").toNumber());
  }, [positions]);

  // Simulation result
  const simResult = useMemo(
    () => simulateMarkChange(positions, markDelta, balance, params),
    [positions, markDelta, balance, params],
  );

  // Cascade estimation (at current marks, not shifted)
  const cascade = useMemo(() => {
    const account: LiquidationAccount = { balance, positions };
    return estimateLiquidationCascade(positions, account, params);
  }, [positions, balance, params]);

  const handleSliderChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setMarkDelta(val);
    setDeltaInput(val);
  }, []);

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setDeltaInput(val);
    // Only update simulation if valid number
    try {
      new Decimal(val);
      setMarkDelta(val);
    } catch {
      // Keep input but don't update simulation
    }
  }, []);

  const handleInputKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      try {
        new Decimal(deltaInput);
        setMarkDelta(deltaInput);
      } catch {
        // Invalid — ignore
      }
    }
  }, [deltaInput]);

  const handleReset = useCallback(() => {
    setMarkDelta("0");
    setDeltaInput("0");
  }, []);

  const healthColor = {
    healthy: "#10b981",
    warning: "#f59e0b",
    danger: "#f97316",
    liquidation: "#ef4444",
  }[simResult.health];

  return (
    <div
      role="region"
      aria-label="Liquidation simulator"
      style={{
        background: "#111827",
        border: "1px solid #374151",
        borderRadius: 8,
        padding: 16,
        fontFamily: "Inter, system-ui, sans-serif",
        color: "#e5e7eb",
      }}
    >
      <h3 style={{ margin: "0 0 16px", fontSize: 16, fontWeight: 600 }}>
        Liquidation Simulator
      </h3>

      {/* Controls */}
      <div style={{ marginBottom: 16 }}>
        <label
          htmlFor="mark-delta-slider"
          style={{ fontSize: 12, color: "#9ca3af", display: "block", marginBottom: 4 }}
        >
          Global Mark Price Shift
        </label>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <input
            id="mark-delta-slider"
            type="range"
            min={-maxAbsDelta}
            max={maxAbsDelta}
            step="1"
            value={parseFloat(markDelta) || 0}
            onChange={handleSliderChange}
            aria-label="Mark price delta slider"
            aria-valuemin={-maxAbsDelta}
            aria-valuemax={maxAbsDelta}
            aria-valuenow={parseFloat(markDelta) || 0}
            style={{ flex: 1, accentColor: healthColor }}
          />
          <input
            id="mark-delta-input"
            type="text"
            value={deltaInput}
            onChange={handleInputChange}
            onKeyDown={handleInputKeyDown}
            aria-label="Mark price delta input"
            style={{
              width: 100,
              padding: "4px 8px",
              fontSize: 13,
              fontFamily: "monospace",
              background: "#1f2937",
              border: "1px solid #374151",
              borderRadius: 4,
              color: "#e5e7eb",
              textAlign: "right",
            }}
          />
          <button
            onClick={handleReset}
            aria-label="Reset mark delta to zero"
            style={{
              padding: "4px 12px",
              fontSize: 12,
              background: "#374151",
              border: "none",
              borderRadius: 4,
              color: "#e5e7eb",
              cursor: "pointer",
            }}
          >
            Reset
          </button>
        </div>
      </div>

      {/* Simulation Summary */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(130px, 1fr))",
          gap: 12,
          marginBottom: 16,
          padding: 12,
          background: "#0d1117",
          borderRadius: 6,
        }}
      >
        <SimMetric label="Total PnL" value={fmt(simResult.total_pnl)} pnl />
        <SimMetric label="Equity" value={fmt(simResult.equity)} />
        <SimMetric label="Init. Margin" value={fmt(simResult.total_initial_margin)} />
        <SimMetric label="Maint. Margin" value={fmt(simResult.total_maintenance_margin)} />
        <SimMetric label="Margin Ratio" value={simResult.margin_ratio === "Infinity" ? "∞" : fmtRatio(simResult.margin_ratio)} />
        <SimMetric label="Health" value={simResult.health.toUpperCase()} color={healthColor} />
      </div>

      {/* Simulated Positions */}
      {simResult.positions.length > 0 && (
        <div style={{ marginBottom: 16 }}>
          <h4 style={{ margin: "0 0 8px", fontSize: 13, fontWeight: 600, color: "#9ca3af" }}>
            Simulated Positions
          </h4>
          <div style={{ overflowX: "auto" }}>
            <table
              style={{ width: "100%", fontSize: 11, borderCollapse: "collapse", whiteSpace: "nowrap" }}
              aria-label="Simulated positions table"
            >
              <thead>
                <tr style={{ borderBottom: "1px solid #374151" }}>
                  <th style={thStyle}>Symbol</th>
                  <th style={thStyle}>Size</th>
                  <th style={thStyle}>New Mark</th>
                  <th style={thStyle}>PnL</th>
                  <th style={thStyle}>Init. Margin</th>
                  <th style={thStyle}>Maint. Margin</th>
                </tr>
              </thead>
              <tbody>
                {simResult.positions.map((p) => {
                  const pnlNum = parseFloat(p.pnl);
                  return (
                    <tr key={p.symbol + p.size} style={{ borderBottom: "1px solid #1f2937" }}>
                      <td style={tdStyle}>{p.symbol}</td>
                      <td style={tdStyle}>{p.size}</td>
                      <td style={tdStyle}>{fmt(p.mark_price)}</td>
                      <td
                        style={{
                          ...tdStyle,
                          color: pnlNum > 0 ? "#10b981" : pnlNum < 0 ? "#ef4444" : "#9ca3af",
                          fontWeight: 600,
                        }}
                      >
                        {pnlNum > 0 ? "+" : ""}{fmt(p.pnl)}
                      </td>
                      <td style={tdStyle}>{fmt(p.initial_margin)}</td>
                      <td style={tdStyle}>{fmt(p.maintenance_margin)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Liquidation Cascade */}
      {cascade.length > 0 && (
        <div>
          <h4 style={{ margin: "0 0 8px", fontSize: 13, fontWeight: 600, color: "#9ca3af" }}>
            Liquidation Cascade (from current marks)
          </h4>
          <div style={{ overflowX: "auto" }}>
            <table
              style={{ width: "100%", fontSize: 11, borderCollapse: "collapse", whiteSpace: "nowrap" }}
              aria-label="Liquidation cascade table"
            >
              <thead>
                <tr style={{ borderBottom: "1px solid #374151" }}>
                  <th style={thStyle}>Order</th>
                  <th style={thStyle}>Symbol</th>
                  <th style={thStyle}>Size</th>
                  <th style={thStyle}>Liq. Price</th>
                  <th style={thStyle}>Δ from Mark</th>
                  <th style={thStyle}>Δ %</th>
                </tr>
              </thead>
              <tbody>
                {cascade.map((c, index) => (
                  <tr key={c.symbol + c.size} style={{ borderBottom: "1px solid #1f2937" }}>
                    <td style={tdStyle}>{index + 1}</td>
                    <td style={tdStyle}>{c.symbol}</td>
                    <td style={tdStyle}>{c.size}</td>
                    <td style={{ ...tdStyle, color: "#f59e0b" }}>{fmt(c.liquidation_price)}</td>
                    <td style={tdStyle}>{fmt(c.mark_delta_to_liquidation)}</td>
                    <td style={tdStyle}>{c.pct_from_current}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {positions.length === 0 && (
        <div style={{ color: "#6b7280", fontSize: 13 }}>
          No positions to simulate. Open positions to use the simulator.
        </div>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Sub-components & helpers
// ---------------------------------------------------------------------------

const SimMetric: React.FC<{ label: string; value: string; pnl?: boolean; color?: string }> = ({
  label,
  value,
  pnl,
  color,
}) => {
  let textColor = color ?? "#e5e7eb";
  if (pnl && !color) {
    const num = parseFloat(value.replace(/,/g, ""));
    if (num > 0) textColor = "#10b981";
    else if (num < 0) textColor = "#ef4444";
    else textColor = "#9ca3af";
  }

  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      <span style={{ fontSize: 10, color: "#9ca3af", textTransform: "uppercase" }}>{label}</span>
      <span style={{ fontSize: 14, fontWeight: 600, fontFamily: "monospace", color: textColor }}>{value}</span>
    </div>
  );
};

function fmt(val: string): string {
  const num = parseFloat(val);
  if (isNaN(num)) return val;
  return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function fmtRatio(val: string): string {
  const num = parseFloat(val);
  if (isNaN(num)) return val;
  return num.toFixed(4);
}

const thStyle: React.CSSProperties = {
  textAlign: "left",
  padding: "4px 8px",
  color: "#9ca3af",
  fontWeight: 500,
};

const tdStyle: React.CSSProperties = {
  padding: "4px 8px",
  fontFamily: "monospace",
};
