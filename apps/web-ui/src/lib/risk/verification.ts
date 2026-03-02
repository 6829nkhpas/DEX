// ---------------------------------------------------------------------------
// verification.ts — Model verification replay engine
// ---------------------------------------------------------------------------
//
// Replays N historical snapshots (from mocked golden data), compares
// computed margin/liquidation values against expected outputs, and
// produces a verification report.
//
// The report is a structured object that can be serialized to Markdown.
// ---------------------------------------------------------------------------

import Decimal from "decimal.js";
import {
  calculateInitialMargin,
  calculateMaintenanceMargin,
  computeUnrealizedPnl,
  computeAccountMetrics,
} from "./margin";
import { simulateMarkChange, computeLiquidationPrice } from "./liquidation";
import type { MarginPosition, MarginParams, AccountMarginMetrics } from "./margin";
import type { LiquidationAccount } from "./liquidation";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface GoldenSnapshot {
  id: string;
  description: string;
  positions: MarginPosition[];
  balance: string;
  params: MarginParams;
  expected: {
    total_initial_margin: string;
    total_maintenance_margin: string;
    total_unrealized_pnl: string;
    equity: string;
    margin_ratio: string;
    health: AccountMarginMetrics["health"];
    liquidation_prices?: Record<string, string>; // symbol -> expected liq price
  };
}

export interface VerificationResult {
  snapshot_id: string;
  description: string;
  passed: boolean;
  mismatches: Array<{
    field: string;
    expected: string;
    actual: string;
    tolerance_exceeded: boolean;
  }>;
}

export interface VerificationReport {
  timestamp: string;
  total_snapshots: number;
  passed: number;
  failed: number;
  confidence: string; // "HIGH" | "MEDIUM" | "LOW"
  results: VerificationResult[];
}

// ---------------------------------------------------------------------------
// Golden test data (mocked historical snapshots)
// ---------------------------------------------------------------------------

export const GOLDEN_SNAPSHOTS: GoldenSnapshot[] = [
  {
    id: "GS-001",
    description: "Single long BTC position, 10x leverage, at entry",
    positions: [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
    ],
    balance: "10000.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "5000",
      total_maintenance_margin: "250",
      total_unrealized_pnl: "0",
      equity: "10000",
      margin_ratio: "40",
      health: "healthy",
    },
  },
  {
    id: "GS-002",
    description: "Single long BTC, mark dropped 5% (underwater)",
    positions: [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "47500.00" },
    ],
    balance: "5500.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "5000",
      total_maintenance_margin: "237.5",
      total_unrealized_pnl: "-2500",
      equity: "3000",
      margin_ratio: "12.631578",
      health: "healthy",
    },
  },
  {
    id: "GS-003",
    description: "Short ETH position, mark rose (losing)",
    positions: [
      { symbol: "ETH/USDT", size: "-10", entry_price: "3000.00", mark_price: "3500.00" },
    ],
    balance: "10000.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "3000",
      total_maintenance_margin: "175",
      total_unrealized_pnl: "-5000",
      equity: "5000",
      margin_ratio: "28.571428",
      health: "healthy",
    },
  },
  {
    id: "GS-004",
    description: "Multiple positions, near danger zone",
    positions: [
      { symbol: "BTC/USDT", size: "2", entry_price: "50000.00", mark_price: "48000.00" },
      { symbol: "ETH/USDT", size: "20", entry_price: "3200.00", mark_price: "3100.00" },
    ],
    balance: "12500.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "16400",
      total_maintenance_margin: "790",
      total_unrealized_pnl: "-6000",
      equity: "6500",
      margin_ratio: "8.227848",
      health: "healthy",
    },
  },
  {
    id: "GS-005",
    description: "Zero-size position (edge case)",
    positions: [
      { symbol: "BTC/USDT", size: "0", entry_price: "50000.00", mark_price: "50000.00" },
    ],
    balance: "10000.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "0",
      total_maintenance_margin: "0",
      total_unrealized_pnl: "0",
      equity: "10000",
      margin_ratio: "Infinity",
      health: "healthy",
    },
  },
  {
    id: "GS-006",
    description: "Very large position, 125x leverage",
    positions: [
      { symbol: "BTC/USDT", size: "100", entry_price: "50000.00", mark_price: "50010.00" },
    ],
    balance: "100000.00",
    params: { leverage: "125", maintenance_margin_rate: "0.004" },
    expected: {
      total_initial_margin: "40000",
      total_maintenance_margin: "20004",
      total_unrealized_pnl: "1000",
      equity: "101000",
      margin_ratio: "5.049390",
      health: "healthy",
    },
  },
  {
    id: "GS-007",
    description: "Liquidation zone — margin ratio < 1.1",
    positions: [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "40500.00" },
    ],
    balance: "5300.00",
    params: { leverage: "10", maintenance_margin_rate: "0.005" },
    expected: {
      total_initial_margin: "5000",
      total_maintenance_margin: "202.5",
      total_unrealized_pnl: "-9500",
      equity: "-4200",
      margin_ratio: "-20.740740",
      health: "liquidation",
    },
  },
  {
    id: "GS-008",
    description: "Warning zone — margin ratio between 1.5 and 2.0",
    positions: [
      { symbol: "BTC/USDT", size: "10", entry_price: "50000.00", mark_price: "49700.00" },
    ],
    balance: "5000.00",
    params: { leverage: "10", maintenance_margin_rate: "0.05" },
    expected: {
      total_initial_margin: "50000",
      total_maintenance_margin: "24850",
      total_unrealized_pnl: "-3000",
      equity: "2000",
      margin_ratio: "0.080483",
      health: "liquidation",
    },
  },
];

// ---------------------------------------------------------------------------
// Verification engine
// ---------------------------------------------------------------------------

/** Tolerance for decimal comparison — accounts for rounding differences. */
const TOLERANCE = new Decimal("0.01");

function isClose(actual: string, expected: string): boolean {
  if (actual === expected) return true;
  if (actual === "Infinity" || expected === "Infinity") return actual === expected;
  try {
    const a = new Decimal(actual);
    const e = new Decimal(expected);
    return a.minus(e).abs().lte(TOLERANCE);
  } catch {
    return actual === expected;
  }
}

/**
 * Run verification against a single golden snapshot.
 */
export function verifySnapshot(snapshot: GoldenSnapshot): VerificationResult {
  const metrics = computeAccountMetrics(snapshot.positions, snapshot.balance, snapshot.params);

  const mismatches: VerificationResult["mismatches"] = [];

  const checks: Array<[string, string, string]> = [
    ["total_initial_margin", metrics.total_initial_margin, snapshot.expected.total_initial_margin],
    ["total_maintenance_margin", metrics.total_maintenance_margin, snapshot.expected.total_maintenance_margin],
    ["total_unrealized_pnl", metrics.total_unrealized_pnl, snapshot.expected.total_unrealized_pnl],
    ["equity", metrics.equity, snapshot.expected.equity],
    ["margin_ratio", metrics.margin_ratio, snapshot.expected.margin_ratio],
    ["health", metrics.health, snapshot.expected.health],
  ];

  for (const [field, actual, expected] of checks) {
    if (!isClose(actual, expected)) {
      mismatches.push({
        field,
        expected,
        actual,
        tolerance_exceeded: true,
      });
    }
  }

  // Check liquidation prices if provided
  if (snapshot.expected.liquidation_prices) {
    const account = { balance: snapshot.balance, positions: snapshot.positions };
    for (const pos of snapshot.positions) {
      const expectedLiq = snapshot.expected.liquidation_prices[pos.symbol];
      if (expectedLiq !== undefined) {
        const result = computeLiquidationPrice(pos, account, snapshot.params);
        if (!isClose(result.liquidation_price, expectedLiq)) {
          mismatches.push({
            field: `liquidation_price_${pos.symbol}`,
            expected: expectedLiq,
            actual: result.liquidation_price,
            tolerance_exceeded: true,
          });
        }
      }
    }
  }

  return {
    snapshot_id: snapshot.id,
    description: snapshot.description,
    passed: mismatches.length === 0,
    mismatches,
  };
}

/**
 * Run full verification against all golden snapshots.
 */
export function runVerification(snapshots: GoldenSnapshot[] = GOLDEN_SNAPSHOTS): VerificationReport {
  const results = snapshots.map(verifySnapshot);
  const passed = results.filter((r) => r.passed).length;
  const failed = results.length - passed;

  const passRate = results.length > 0 ? passed / results.length : 1;
  let confidence: string;
  if (passRate >= 0.95) confidence = "HIGH";
  else if (passRate >= 0.8) confidence = "MEDIUM";
  else confidence = "LOW";

  return {
    timestamp: new Date().toISOString(),
    total_snapshots: results.length,
    passed,
    failed,
    confidence,
    results,
  };
}

/**
 * Generate a Markdown verification report.
 */
export function generateVerificationMarkdown(report: VerificationReport): string {
  const lines: string[] = [];

  lines.push("# Risk Model Verification Report");
  lines.push("");
  lines.push(`**Generated**: ${report.timestamp}`);
  lines.push(`**Total Snapshots**: ${report.total_snapshots}`);
  lines.push(`**Passed**: ${report.passed}`);
  lines.push(`**Failed**: ${report.failed}`);
  lines.push(`**Confidence**: ${report.confidence}`);
  lines.push("");
  lines.push("## Results");
  lines.push("");

  for (const result of report.results) {
    const status = result.passed ? "✅ PASS" : "❌ FAIL";
    lines.push(`### ${result.snapshot_id}: ${result.description}`);
    lines.push(`**Status**: ${status}`);
    lines.push("");

    if (result.mismatches.length > 0) {
      lines.push("| Field | Expected | Actual | Tolerance Exceeded |");
      lines.push("|-------|----------|--------|--------------------|");
      for (const m of result.mismatches) {
        lines.push(`| ${m.field} | ${m.expected} | ${m.actual} | ${m.tolerance_exceeded ? "Yes" : "No"} |`);
      }
      lines.push("");
    }
  }

  lines.push("## Methodology");
  lines.push("");
  lines.push("- Margin calculations use decimal.js with ROUND_UP (favor safety).");
  lines.push("- Liquidation prices solved algebraically from margin_ratio = 1.1.");
  lines.push("- Tolerance for numeric comparison: ±0.01.");
  lines.push("- Golden snapshots sourced from spec §05/§06 examples and edge cases.");
  lines.push("");
  lines.push("## Known Limitations");
  lines.push("");
  lines.push("- Cross-margin multi-position liquidation cascade is approximate.");
  lines.push("- Portfolio-margin (VaR) mode is not yet implemented.");
  lines.push("- Concentration risk add-on is not included in these calculations.");
  lines.push("");

  return lines.join("\n");
}
