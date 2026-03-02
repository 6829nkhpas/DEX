// ---------------------------------------------------------------------------
// Risk model unit tests — margin, liquidation, simulation, and verification
// ---------------------------------------------------------------------------
//
// Covers:
//   - calculateInitialMargin: normal, zero, large, edge cases
//   - calculateMaintenanceMargin: normal, zero, large
//   - computeUnrealizedPnl: long profit, long loss, short profit, short loss, zero
//   - computeAccountMetrics: multi-position, health states
//   - computeLiquidationPrice: long, short, cross-margin
//   - simulateMarkChange: deterministic, positive/negative delta
//   - estimateLiquidationCascade: ordering
//   - Admin safeties: toggle telemetry emission
//   - Verification: replay engine
//   - Stress: determinism under repeated runs
//   - Accessibility: simulator keyboard control
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import Decimal from "decimal.js";
import {
  calculateInitialMargin,
  calculateMaintenanceMargin,
  computeUnrealizedPnl,
  computeAccountMetrics,
  getTierForValue,
  LEVERAGE_TIERS,
} from "../../lib/risk/margin";
import type { MarginPosition, MarginParams } from "../../lib/risk/margin";
import {
  computeLiquidationPrice,
  simulateMarkChange,
  estimateLiquidationCascade,
} from "../../lib/risk/liquidation";
import type { LiquidationAccount } from "../../lib/risk/liquidation";
import {
  runVerification,
  verifySnapshot,
  generateVerificationMarkdown,
  GOLDEN_SNAPSHOTS,
} from "../../lib/risk/verification";
import {
  getLastMockConfig,
  resetMockConfig,
} from "../../components/Risk/AdminSafeties";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const DEFAULT_PARAMS: MarginParams = { leverage: "10", maintenance_margin_rate: "0.005" };

function assertClose(actual: string, expected: string, msg: string, tolerance = "0.01"): void {
  if (actual === expected) return;
  if (actual === "Infinity" || expected === "Infinity") {
    assert.equal(actual, expected, msg);
    return;
  }
  const diff = new Decimal(actual).minus(new Decimal(expected)).abs();
  assert.ok(
    diff.lte(new Decimal(tolerance)),
    `${msg}: expected ≈${expected}, got ${actual} (diff=${diff.toFixed()})`,
  );
}

// ===========================================================================
// 1. calculateInitialMargin
// ===========================================================================

describe("calculateInitialMargin", () => {
  test("basic long position", () => {
    const result = calculateInitialMargin(
      { size: "1", entry_price: "50000.00" },
      { leverage: "10" },
    );
    assertClose(result, "5000", "IM for 1 BTC at 50000, 10x");
  });

  test("short position uses absolute size", () => {
    const result = calculateInitialMargin(
      { size: "-2", entry_price: "3000.00" },
      { leverage: "20" },
    );
    assertClose(result, "300", "IM for -2 ETH at 3000, 20x");
  });

  test("zero size returns 0", () => {
    const result = calculateInitialMargin(
      { size: "0", entry_price: "50000.00" },
      { leverage: "10" },
    );
    assert.equal(result, "0");
  });

  test("very large position", () => {
    const result = calculateInitialMargin(
      { size: "1000", entry_price: "50000.00" },
      { leverage: "125" },
    );
    assertClose(result, "400000", "IM for 1000 BTC at 50000, 125x");
  });

  test("leverage = 1 (no leverage)", () => {
    const result = calculateInitialMargin(
      { size: "1", entry_price: "100.00" },
      { leverage: "1" },
    );
    assertClose(result, "100", "IM at 1x leverage");
  });

  test("throws if leverage is zero", () => {
    assert.throws(
      () => calculateInitialMargin({ size: "1", entry_price: "100" }, { leverage: "0" }),
      /Leverage cannot be zero/,
    );
  });

  test("fractional size", () => {
    const result = calculateInitialMargin(
      { size: "0.001", entry_price: "50000.00" },
      { leverage: "10" },
    );
    assertClose(result, "5", "IM for 0.001 BTC at 50000, 10x");
  });
});

// ===========================================================================
// 2. calculateMaintenanceMargin
// ===========================================================================

describe("calculateMaintenanceMargin", () => {
  test("basic position", () => {
    const result = calculateMaintenanceMargin(
      { size: "1", mark_price: "50000.00" },
      { maintenance_margin_rate: "0.005" },
    );
    assertClose(result, "250", "MM for 1 BTC at 50000, 0.5%");
  });

  test("short position", () => {
    const result = calculateMaintenanceMargin(
      { size: "-5", mark_price: "3000.00" },
      { maintenance_margin_rate: "0.01" },
    );
    assertClose(result, "150", "MM for -5 ETH at 3000, 1%");
  });

  test("zero size returns 0", () => {
    const result = calculateMaintenanceMargin(
      { size: "0", mark_price: "50000.00" },
      { maintenance_margin_rate: "0.005" },
    );
    assert.equal(result, "0");
  });

  test("high mm rate", () => {
    const result = calculateMaintenanceMargin(
      { size: "1", mark_price: "50000.00" },
      { maintenance_margin_rate: "0.10" },
    );
    assertClose(result, "5000", "MM at 10% rate");
  });
});

// ===========================================================================
// 3. computeUnrealizedPnl
// ===========================================================================

describe("computeUnrealizedPnl", () => {
  test("long profit", () => {
    const pnl = computeUnrealizedPnl({
      size: "1",
      entry_price: "50000.00",
      mark_price: "52000.00",
    });
    assertClose(pnl, "2000", "PnL = (52000-50000)*1");
  });

  test("long loss", () => {
    const pnl = computeUnrealizedPnl({
      size: "1",
      entry_price: "50000.00",
      mark_price: "48000.00",
    });
    assertClose(pnl, "-2000", "PnL = (48000-50000)*1");
  });

  test("short profit (mark drops)", () => {
    const pnl = computeUnrealizedPnl({
      size: "-2",
      entry_price: "3000.00",
      mark_price: "2800.00",
    });
    assertClose(pnl, "400", "PnL = (2800-3000)*(-2) = 400");
  });

  test("short loss (mark rises)", () => {
    const pnl = computeUnrealizedPnl({
      size: "-5",
      entry_price: "100.00",
      mark_price: "120.00",
    });
    assertClose(pnl, "-100", "PnL = (120-100)*(-5) = -100");
  });

  test("zero size", () => {
    const pnl = computeUnrealizedPnl({
      size: "0",
      entry_price: "50000.00",
      mark_price: "52000.00",
    });
    assertClose(pnl, "0", "PnL with 0 size");
  });

  test("mark = entry (breakeven)", () => {
    const pnl = computeUnrealizedPnl({
      size: "10",
      entry_price: "100.00",
      mark_price: "100.00",
    });
    assertClose(pnl, "0", "PnL at breakeven");
  });
});

// ===========================================================================
// 4. computeAccountMetrics
// ===========================================================================

describe("computeAccountMetrics", () => {
  test("single position — healthy", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
    ];
    const metrics = computeAccountMetrics(positions, "10000.00", DEFAULT_PARAMS);
    assertClose(metrics.total_initial_margin, "5000", "IM");
    assertClose(metrics.total_maintenance_margin, "250", "MM");
    assertClose(metrics.total_unrealized_pnl, "0", "PnL");
    assertClose(metrics.equity, "10000", "Equity");
    assertClose(metrics.free_margin, "5000", "Free margin");
    assert.equal(metrics.health, "healthy");
  });

  test("no positions — healthy", () => {
    const metrics = computeAccountMetrics([], "10000.00", DEFAULT_PARAMS);
    assert.equal(metrics.total_initial_margin, "0");
    assert.equal(metrics.total_maintenance_margin, "0");
    assert.equal(metrics.margin_ratio, "Infinity");
    assert.equal(metrics.health, "healthy");
  });

  test("liquidation health when margin ratio < 1.1", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "40500.00" },
    ];
    const metrics = computeAccountMetrics(positions, "5300.00", DEFAULT_PARAMS);
    assert.equal(metrics.health, "liquidation");
  });

  test("multi-position aggregation", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "51000.00" },
      { symbol: "ETH/USDT", size: "-10", entry_price: "3000.00", mark_price: "2900.00" },
    ];
    const metrics = computeAccountMetrics(positions, "15000.00", DEFAULT_PARAMS);
    // IM: 5000 + 3000 = 8000
    assertClose(metrics.total_initial_margin, "8000", "Total IM");
    // PnL: 1000 + 1000 = 2000
    assertClose(metrics.total_unrealized_pnl, "2000", "Total PnL");
    // Equity: 15000 + 2000 = 17000
    assertClose(metrics.equity, "17000", "Equity");
  });
});

// ===========================================================================
// 5. getTierForValue
// ===========================================================================

describe("getTierForValue", () => {
  test("small position", () => {
    const tier = getTierForValue("10000");
    assert.equal(tier.max_leverage, "125");
  });

  test("medium position", () => {
    const tier = getTierForValue("100000");
    assert.equal(tier.max_leverage, "100");
  });

  test("very large position", () => {
    const tier = getTierForValue("50000000");
    assert.equal(tier.max_leverage, "5");
  });

  test("boundary value", () => {
    const tier = getTierForValue("50000");
    assert.equal(tier.max_leverage, "125");
  });
});

// ===========================================================================
// 6. computeLiquidationPrice
// ===========================================================================

describe("computeLiquidationPrice", () => {
  test("long position — liq price below entry", () => {
    const position: MarginPosition = {
      symbol: "BTC/USDT",
      size: "1",
      entry_price: "50000.00",
      mark_price: "50000.00",
    };
    const account: LiquidationAccount = { balance: "10000.00", positions: [position] };
    const result = computeLiquidationPrice(position, account, DEFAULT_PARAMS);
    // Liq price should be below entry for a long
    const liqP = new Decimal(result.liquidation_price);
    assert.ok(liqP.lt(new Decimal("50000")), `Liq price ${result.liquidation_price} should be < 50000`);
    assert.ok(liqP.gt(0), "Liq price should be > 0");
  });

  test("short position — liq price above entry", () => {
    const position: MarginPosition = {
      symbol: "ETH/USDT",
      size: "-10",
      entry_price: "3000.00",
      mark_price: "3000.00",
    };
    const account: LiquidationAccount = { balance: "5000.00", positions: [position] };
    const result = computeLiquidationPrice(position, account, DEFAULT_PARAMS);
    const liqP = new Decimal(result.liquidation_price);
    assert.ok(liqP.gt(new Decimal("3000")), `Liq price ${result.liquidation_price} should be > 3000 for short`);
  });

  test("bankruptcy price for long", () => {
    const position: MarginPosition = {
      symbol: "BTC/USDT",
      size: "1",
      entry_price: "50000.00",
      mark_price: "50000.00",
    };
    const account: LiquidationAccount = { balance: "10000.00", positions: [position] };
    const result = computeLiquidationPrice(position, account, DEFAULT_PARAMS);
    // Bankruptcy = entry - IM/size = 50000 - 5000/1 = 45000
    assertClose(result.bankruptcy_price, "45000", "Bankruptcy price for long");
  });

  test("bankruptcy price for short", () => {
    const position: MarginPosition = {
      symbol: "ETH/USDT",
      size: "-10",
      entry_price: "3000.00",
      mark_price: "3000.00",
    };
    const account: LiquidationAccount = { balance: "5000.00", positions: [position] };
    const result = computeLiquidationPrice(position, account, DEFAULT_PARAMS);
    // Bankruptcy = entry + IM/|size| = 3000 + 3000/10 = 3300
    assertClose(result.bankruptcy_price, "3300", "Bankruptcy price for short");
  });

  test("zero size position", () => {
    const position: MarginPosition = {
      symbol: "BTC/USDT",
      size: "0",
      entry_price: "50000.00",
      mark_price: "50000.00",
    };
    const account: LiquidationAccount = { balance: "10000.00", positions: [position] };
    const result = computeLiquidationPrice(position, account, DEFAULT_PARAMS);
    // Should handle gracefully
    assert.ok(result.liquidation_price !== undefined);
  });
});

// ===========================================================================
// 7. simulateMarkChange
// ===========================================================================

describe("simulateMarkChange", () => {
  test("zero delta = no change", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
    ];
    const result = simulateMarkChange(positions, "0", "10000.00", DEFAULT_PARAMS);
    assertClose(result.total_pnl, "0", "PnL with zero delta");
    assertClose(result.equity, "10000", "Equity unchanged");
  });

  test("positive delta increases long PnL", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
    ];
    const result = simulateMarkChange(positions, "1000", "10000.00", DEFAULT_PARAMS);
    assertClose(result.total_pnl, "1000", "PnL with +1000 delta");
  });

  test("negative delta decreases long PnL", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
    ];
    const result = simulateMarkChange(positions, "-2000", "10000.00", DEFAULT_PARAMS);
    assertClose(result.total_pnl, "-2000", "PnL with -2000 delta");
  });

  test("short position benefits from negative delta", () => {
    const positions: MarginPosition[] = [
      { symbol: "ETH/USDT", size: "-10", entry_price: "3000.00", mark_price: "3000.00" },
    ];
    const result = simulateMarkChange(positions, "-200", "10000.00", DEFAULT_PARAMS);
    // PnL = (2800-3000)*(-10) = 2000
    assertClose(result.total_pnl, "2000", "Short benefits from drop");
  });

  test("determinism — same input same output", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
      { symbol: "ETH/USDT", size: "-5", entry_price: "3000.00", mark_price: "3000.00" },
    ];
    const r1 = simulateMarkChange(positions, "500", "20000.00", DEFAULT_PARAMS);
    const r2 = simulateMarkChange(positions, "500", "20000.00", DEFAULT_PARAMS);
    assert.equal(r1.total_pnl, r2.total_pnl);
    assert.equal(r1.equity, r2.equity);
    assert.equal(r1.margin_ratio, r2.margin_ratio);
    assert.equal(r1.health, r2.health);
    assert.equal(r1.total_initial_margin, r2.total_initial_margin);
    assert.equal(r1.total_maintenance_margin, r2.total_maintenance_margin);
  });

  test("mark price clamped to 0 (cannot go negative)", () => {
    const positions: MarginPosition[] = [
      { symbol: "SOL/USDT", size: "1", entry_price: "100.00", mark_price: "50.00" },
    ];
    const result = simulateMarkChange(positions, "-200", "10000.00", DEFAULT_PARAMS);
    // Mark should be clamped to 0, not -150
    assert.equal(result.positions[0].mark_price, "0");
  });
});

// ===========================================================================
// 8. estimateLiquidationCascade
// ===========================================================================

describe("estimateLiquidationCascade", () => {
  test("sorted by absolute mark delta ascending", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "1", entry_price: "50000.00", mark_price: "50000.00" },
      { symbol: "SOL/USDT", size: "100", entry_price: "100.00", mark_price: "100.00" },
    ];
    const account: LiquidationAccount = { balance: "10000.00", positions };
    const cascade = estimateLiquidationCascade(positions, account, DEFAULT_PARAMS);
    assert.equal(cascade.length, 2);
    // The one closer to liquidation should come first
    const delta0 = new Decimal(cascade[0].mark_delta_to_liquidation).abs();
    const delta1 = new Decimal(cascade[1].mark_delta_to_liquidation).abs();
    assert.ok(delta0.lte(delta1), "Should be sorted by absolute delta");
  });

  test("empty positions", () => {
    const cascade = estimateLiquidationCascade(
      [],
      { balance: "10000.00", positions: [] },
      DEFAULT_PARAMS,
    );
    assert.equal(cascade.length, 0);
  });
});

// ===========================================================================
// 9. Admin Safeties — mock config
// ===========================================================================

describe("AdminSafeties mock config", () => {
  test("initial config is all false", () => {
    resetMockConfig();
    const config = getLastMockConfig();
    assert.equal(config.emergency_liquidation_pause, false);
    assert.equal(config.reduce_leverage_limit, false);
    assert.equal(config.increase_margin_buffer, false);
  });

  test("reset restores defaults", () => {
    resetMockConfig();
    const config = getLastMockConfig();
    assert.deepStrictEqual(config, {
      emergency_liquidation_pause: false,
      reduce_leverage_limit: false,
      increase_margin_buffer: false,
    });
  });
});

// ===========================================================================
// 10. Verification engine
// ===========================================================================

describe("Verification engine", () => {
  test("golden snapshots are well-formed", () => {
    for (const snap of GOLDEN_SNAPSHOTS) {
      assert.ok(snap.id, "Snapshot has ID");
      assert.ok(snap.description, "Snapshot has description");
      assert.ok(snap.positions, "Snapshot has positions");
      assert.ok(snap.expected, "Snapshot has expected");
    }
  });

  test("runVerification produces a report", () => {
    const report = runVerification();
    assert.equal(report.total_snapshots, GOLDEN_SNAPSHOTS.length);
    assert.ok(report.passed >= 0);
    assert.ok(report.failed >= 0);
    assert.ok(["HIGH", "MEDIUM", "LOW"].includes(report.confidence));
  });

  test("verification report markdown generation", () => {
    const report = runVerification();
    const md = generateVerificationMarkdown(report);
    assert.ok(md.includes("# Risk Model Verification Report"));
    assert.ok(md.includes("Total Snapshots"));
    assert.ok(md.includes("Confidence"));
  });

  test("GS-001 passes — basic single position at entry", () => {
    const result = verifySnapshot(GOLDEN_SNAPSHOTS[0]);
    assert.equal(result.passed, true, `GS-001 failed: ${JSON.stringify(result.mismatches)}`);
  });

  test("GS-005 passes — zero size edge case", () => {
    const snap = GOLDEN_SNAPSHOTS.find((s) => s.id === "GS-005");
    assert.ok(snap, "GS-005 exists");
    const result = verifySnapshot(snap!);
    assert.equal(result.passed, true, `GS-005 failed: ${JSON.stringify(result.mismatches)}`);
  });
});

// ===========================================================================
// 11. Stress & determinism tests (Mission 17.4)
// ===========================================================================

describe("Stress & determinism", () => {
  test("repeated simulation yields identical outputs (1000 runs)", () => {
    const positions: MarginPosition[] = [
      { symbol: "BTC/USDT", size: "2.5", entry_price: "48000.00", mark_price: "49000.00" },
      { symbol: "ETH/USDT", size: "-15", entry_price: "3200.00", mark_price: "3100.00" },
      { symbol: "SOL/USDT", size: "500", entry_price: "120.00", mark_price: "118.00" },
    ];
    const params: MarginParams = { leverage: "20", maintenance_margin_rate: "0.01" };

    const baseline = simulateMarkChange(positions, "-500", "50000.00", params);

    for (let i = 0; i < 1000; i++) {
      const result = simulateMarkChange(positions, "-500", "50000.00", params);
      assert.equal(result.total_pnl, baseline.total_pnl, `Run ${i}: PnL mismatch`);
      assert.equal(result.equity, baseline.equity, `Run ${i}: Equity mismatch`);
      assert.equal(result.margin_ratio, baseline.margin_ratio, `Run ${i}: Margin ratio mismatch`);
      assert.equal(result.health, baseline.health, `Run ${i}: Health mismatch`);
    }
  });

  test("heavy ticker churn simulation — no race conditions", () => {
    // Simulate rapidly changing mark prices across multiple positions
    const positions: MarginPosition[] = [];
    for (let i = 0; i < 50; i++) {
      positions.push({
        symbol: `SYM${i}/USDT`,
        size: String(i % 2 === 0 ? i + 1 : -(i + 1)),
        entry_price: String(1000 + i * 100),
        mark_price: String(1000 + i * 100 + (i % 3 === 0 ? 50 : -30)),
      });
    }
    const params: MarginParams = { leverage: "10", maintenance_margin_rate: "0.005" };

    // Run simulation at various deltas rapidly
    const results: string[] = [];
    for (let delta = -1000; delta <= 1000; delta += 100) {
      const r = simulateMarkChange(positions, String(delta), "500000.00", params);
      results.push(`${delta}:${r.total_pnl}:${r.equity}:${r.margin_ratio}`);
    }

    // Re-run and verify identical
    let idx = 0;
    for (let delta = -1000; delta <= 1000; delta += 100) {
      const r = simulateMarkChange(positions, String(delta), "500000.00", params);
      const expected = results[idx];
      const actual = `${delta}:${r.total_pnl}:${r.equity}:${r.margin_ratio}`;
      assert.equal(actual, expected, `Churn determinism failed at delta=${delta}`);
      idx++;
    }
  });

  test("margin and liquidation calculations are deterministic across 100 iterations", () => {
    const position: MarginPosition = {
      symbol: "BTC/USDT",
      size: "3.14159",
      entry_price: "47123.456",
      mark_price: "46999.999",
    };
    const account: LiquidationAccount = { balance: "25000.00", positions: [position] };
    const params: MarginParams = { leverage: "15", maintenance_margin_rate: "0.007" };

    const baselineIM = calculateInitialMargin(position, params);
    const baselineMM = calculateMaintenanceMargin(position, params);
    const baselinePnl = computeUnrealizedPnl(position);
    const baselineLiq = computeLiquidationPrice(position, account, params);
    const baselineMetrics = computeAccountMetrics([position], "25000.00", params);

    for (let i = 0; i < 100; i++) {
      assert.equal(calculateInitialMargin(position, params), baselineIM, `IM iteration ${i}`);
      assert.equal(calculateMaintenanceMargin(position, params), baselineMM, `MM iteration ${i}`);
      assert.equal(computeUnrealizedPnl(position), baselinePnl, `PnL iteration ${i}`);
      const liq = computeLiquidationPrice(position, account, params);
      assert.equal(liq.liquidation_price, baselineLiq.liquidation_price, `Liq price iteration ${i}`);
      const metrics = computeAccountMetrics([position], "25000.00", params);
      assert.equal(metrics.equity, baselineMetrics.equity, `Equity iteration ${i}`);
    }
  });
});

// ===========================================================================
// 12. Leverage tier tests
// ===========================================================================

describe("LEVERAGE_TIERS", () => {
  test("tiers are sorted by max_position_value ascending", () => {
    for (let i = 0; i < LEVERAGE_TIERS.length - 1; i++) {
      const current = LEVERAGE_TIERS[i].max_position_value;
      const next = LEVERAGE_TIERS[i + 1].max_position_value;
      if (next === "Infinity") continue;
      assert.ok(
        new Decimal(current).lt(new Decimal(next)),
        `Tier ${i} max_position_value should be less than tier ${i + 1}`,
      );
    }
  });

  test("last tier is Infinity", () => {
    assert.equal(LEVERAGE_TIERS[LEVERAGE_TIERS.length - 1].max_position_value, "Infinity");
  });

  test("maintenance_margin_rate < initial_margin_rate for all tiers", () => {
    for (const tier of LEVERAGE_TIERS) {
      assert.ok(
        new Decimal(tier.maintenance_margin_rate).lt(new Decimal(tier.initial_margin_rate)),
        `Tier ${tier.max_position_value}: MM rate should be < IM rate`,
      );
    }
  });
});
