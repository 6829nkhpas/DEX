// ---------------------------------------------------------------------------
// AccountSummary — aggregated account margin metrics
// ---------------------------------------------------------------------------
//
// Displays equity, total IM/MM, free margin, margin ratio, and health status.
// Values update live when store tickers change.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import { computeAccountMetrics } from "../../lib/risk";
import type { MarginPosition, MarginParams, AccountMarginMetrics } from "../../lib/risk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AccountSummaryProps {
  positions: MarginPosition[];
  balance: string;
  params: MarginParams;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const HEALTH_COLORS: Record<AccountMarginMetrics["health"], string> = {
  healthy: "#10b981",
  warning: "#f59e0b",
  danger: "#f97316",
  liquidation: "#ef4444",
};

const HEALTH_LABELS: Record<AccountMarginMetrics["health"], string> = {
  healthy: "Healthy",
  warning: "Warning",
  danger: "Danger",
  liquidation: "Liquidation Risk",
};

function formatDecimal(val: string, dp: number = 2): string {
  if (val === "Infinity") return "∞";
  const num = parseFloat(val);
  if (isNaN(num)) return val;
  return num.toLocaleString(undefined, { minimumFractionDigits: dp, maximumFractionDigits: dp });
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AccountSummary: React.FC<AccountSummaryProps> = ({ positions, balance, params }) => {
  const metrics = useMemo(
    () => computeAccountMetrics(positions, balance, params),
    [positions, balance, params],
  );

  const healthColor = HEALTH_COLORS[metrics.health];

  return (
    <div
      role="region"
      aria-label="Account margin summary"
      style={{
        background: "#111827",
        border: "1px solid #374151",
        borderRadius: 8,
        padding: 16,
        fontFamily: "Inter, system-ui, sans-serif",
        color: "#e5e7eb",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
        <h3 style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>Account Summary</h3>
        <span
          aria-label={`Health status: ${HEALTH_LABELS[metrics.health]}`}
          style={{
            marginLeft: 12,
            fontSize: 11,
            fontWeight: 700,
            padding: "2px 8px",
            borderRadius: 4,
            background: healthColor + "22",
            color: healthColor,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
          }}
        >
          {HEALTH_LABELS[metrics.health]}
        </span>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))",
          gap: 12,
        }}
      >
        <MetricCard label="Equity" value={formatDecimal(metrics.equity)} />
        <MetricCard label="Unrealised PnL" value={formatDecimal(metrics.total_unrealized_pnl)} pnl />
        <MetricCard label="Initial Margin" value={formatDecimal(metrics.total_initial_margin)} />
        <MetricCard label="Maint. Margin" value={formatDecimal(metrics.total_maintenance_margin)} />
        <MetricCard label="Free Margin" value={formatDecimal(metrics.free_margin)} pnl />
        <MetricCard label="Margin Ratio" value={metrics.margin_ratio === "Infinity" ? "∞" : formatDecimal(metrics.margin_ratio, 4)} />
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

const MetricCard: React.FC<{ label: string; value: string; pnl?: boolean }> = ({ label, value, pnl }) => {
  let color = "#e5e7eb";
  if (pnl) {
    const num = parseFloat(value.replace(/,/g, ""));
    if (num > 0) color = "#10b981";
    else if (num < 0) color = "#ef4444";
    else color = "#9ca3af";
  }

  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      <span style={{ fontSize: 11, color: "#9ca3af", textTransform: "uppercase", letterSpacing: "0.04em" }}>
        {label}
      </span>
      <span style={{ fontSize: 16, fontWeight: 600, fontFamily: "monospace", color }}>{value}</span>
    </div>
  );
};
