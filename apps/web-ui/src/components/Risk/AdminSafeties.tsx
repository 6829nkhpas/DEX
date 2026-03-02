// ---------------------------------------------------------------------------
// AdminSafeties — stub admin panel with safety toggles
// ---------------------------------------------------------------------------
//
// Toggles (UI-only stubs):
//   - emergency_liquidation_pause
//   - reduce_leverage_limit
//   - increase_margin_buffer
//
// Each toggle emits a telemetry event and writes to a local mock config.
// ---------------------------------------------------------------------------

import React, { useState, useCallback } from "react";
import { getTelemetryClient } from "../../infra/telemetry";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AdminSafetiesConfig {
  emergency_liquidation_pause: boolean;
  reduce_leverage_limit: boolean;
  increase_margin_buffer: boolean;
}

const DEFAULT_CONFIG: AdminSafetiesConfig = {
  emergency_liquidation_pause: false,
  reduce_leverage_limit: false,
  increase_margin_buffer: false,
};

// ---------------------------------------------------------------------------
// Mock config endpoint
// ---------------------------------------------------------------------------

/** Last written config — accessible for tests. */
let _lastMockConfig: AdminSafetiesConfig = { ...DEFAULT_CONFIG };

export function getLastMockConfig(): AdminSafetiesConfig {
  return { ..._lastMockConfig };
}

export function resetMockConfig(): void {
  _lastMockConfig = { ...DEFAULT_CONFIG };
}

async function writeMockConfig(config: AdminSafetiesConfig): Promise<void> {
  _lastMockConfig = { ...config };
  // In production this would POST to /admin/config or ops/telemetry-mock
  // For now, also try a fire-and-forget POST to the mock endpoint
  try {
    await fetch("/admin/risk-config", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(config),
    }).catch(() => { /* ignore network errors in stub */ });
  } catch {
    // Non-blocking
  }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface AdminSafetiesProps {
  /** Optional callback when config changes */
  onConfigChange?: (config: AdminSafetiesConfig) => void;
}

export const AdminSafeties: React.FC<AdminSafetiesProps> = ({ onConfigChange }) => {
  const [config, setConfig] = useState<AdminSafetiesConfig>({ ...DEFAULT_CONFIG });

  const handleToggle = useCallback(
    (key: keyof AdminSafetiesConfig) => {
      setConfig((prev) => {
        const next = { ...prev, [key]: !prev[key] };

        // Emit telemetry
        try {
          const telemetry = getTelemetryClient();
          telemetry.emit("circuit_breaker_trip" as any, {
            action: "admin_safety_toggle",
            key,
            value: next[key],
            config: next,
          });
        } catch {
          // Telemetry is fire-and-forget
        }

        // Write to mock config endpoint
        writeMockConfig(next);

        onConfigChange?.(next);
        return next;
      });
    },
    [onConfigChange],
  );

  return (
    <div
      role="region"
      aria-label="Admin safety controls"
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
        Admin Safeties
        <span style={{ marginLeft: 8, fontSize: 11, color: "#6b7280", fontWeight: 400 }}>
          (stubs)
        </span>
      </h3>

      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <SafetyToggle
          id="emergency_liquidation_pause"
          label="Emergency Liquidation Pause"
          description="Halt all liquidations system-wide. Use during extreme market events."
          checked={config.emergency_liquidation_pause}
          onToggle={() => handleToggle("emergency_liquidation_pause")}
          danger
        />
        <SafetyToggle
          id="reduce_leverage_limit"
          label="Reduce Leverage Limit"
          description="Temporarily cap maximum leverage to 10x across all symbols."
          checked={config.reduce_leverage_limit}
          onToggle={() => handleToggle("reduce_leverage_limit")}
        />
        <SafetyToggle
          id="increase_margin_buffer"
          label="Increase Margin Buffer"
          description="Add 20% extra margin buffer to all maintenance margin calculations."
          checked={config.increase_margin_buffer}
          onToggle={() => handleToggle("increase_margin_buffer")}
        />
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Sub-component: Toggle row
// ---------------------------------------------------------------------------

const SafetyToggle: React.FC<{
  id: string;
  label: string;
  description: string;
  checked: boolean;
  onToggle: () => void;
  danger?: boolean;
}> = ({ id, label, description, checked, onToggle, danger }) => {
  const activeColor = danger ? "#ef4444" : "#f59e0b";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "8px 12px",
        background: checked ? (danger ? "#7f1d1d22" : "#78350f22") : "transparent",
        borderRadius: 6,
        border: `1px solid ${checked ? activeColor + "44" : "#374151"}`,
      }}
    >
      <button
        role="switch"
        id={id}
        aria-checked={checked}
        aria-label={label}
        onClick={onToggle}
        style={{
          width: 44,
          height: 24,
          borderRadius: 12,
          border: "none",
          background: checked ? activeColor : "#4b5563",
          position: "relative",
          cursor: "pointer",
          transition: "background 0.2s",
          flexShrink: 0,
        }}
      >
        <span
          style={{
            position: "absolute",
            top: 2,
            left: checked ? 22 : 2,
            width: 20,
            height: 20,
            borderRadius: "50%",
            background: "#fff",
            transition: "left 0.2s",
          }}
        />
      </button>
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>{label}</div>
        <div style={{ fontSize: 11, color: "#9ca3af" }}>{description}</div>
      </div>
      {checked && (
        <span
          style={{
            fontSize: 10,
            fontWeight: 700,
            color: activeColor,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
          }}
        >
          Active
        </span>
      )}
    </div>
  );
};
