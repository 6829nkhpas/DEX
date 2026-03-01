// ---------------------------------------------------------------------------
// Tests — Positions: PnL calculation with decimal.js precision
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { computePnl } from "../Positions";

// ---------------------------------------------------------------------------
// computePnl — core PnL logic
// ---------------------------------------------------------------------------

describe("computePnl", () => {
    test("long position — mark > entry → positive PnL", () => {
        // PnL = (51000 - 50000) * 1.5 = 1500
        const pnl = computePnl("51000", "50000", "1.5");
        assert.equal(pnl, "1500");
    });

    test("long position — mark < entry → negative PnL", () => {
        // PnL = (49000 - 50000) * 2.0 = -2000
        const pnl = computePnl("49000", "50000", "2.0");
        assert.equal(pnl, "-2000");
    });

    test("short position — mark > entry → negative PnL", () => {
        // Short has negative size. PnL = (51000 - 50000) * (-1.0) = -1000
        const pnl = computePnl("51000", "50000", "-1.0");
        assert.equal(pnl, "-1000");
    });

    test("short position — mark < entry → positive PnL", () => {
        // PnL = (49000 - 50000) * (-1.0) = 1000
        const pnl = computePnl("49000", "50000", "-1.0");
        assert.equal(pnl, "1000");
    });

    test("zero PnL when mark equals entry", () => {
        const pnl = computePnl("50000.00", "50000.00", "3.0");
        assert.equal(pnl, "0");
    });

    test("high-precision values — no floating-point drift", () => {
        // PnL = (50000.123456789 - 50000.000000000) * 0.000001
        // = 0.123456789 * 0.000001 = 0.000000123456789
        const pnl = computePnl(
            "50000.123456789",
            "50000.000000000",
            "0.000001",
        );
        assert.equal(pnl, "0.000000123456789");
    });

    test("very large position size", () => {
        // PnL = (51000 - 50000) * 999999 = 999999000
        const pnl = computePnl("51000", "50000", "999999");
        assert.equal(pnl, "999999000");
    });

    test("fractional prices and sizes", () => {
        // PnL = (100.50 - 100.25) * 10.5 = 0.25 * 10.5 = 2.625
        const pnl = computePnl("100.50", "100.25", "10.5");
        assert.equal(pnl, "2.625");
    });

    test("zero size → zero PnL", () => {
        const pnl = computePnl("51000", "50000", "0");
        assert.equal(pnl, "0");
    });

    test("negative mark price edge case", () => {
        // Unlikely but possible in futures. PnL = (-100 - 50) * 1 = -150
        const pnl = computePnl("-100", "50", "1");
        assert.equal(pnl, "-150");
    });
});

// ---------------------------------------------------------------------------
// PnL live update scenario (logic-level)
// ---------------------------------------------------------------------------

describe("PnL live update simulation", () => {
    test("PnL changes as mark price updates", () => {
        const entryPrice = "50000.00";
        const size = "2.0";

        // Tick 1: mark = 50500
        const pnl1 = computePnl("50500.00", entryPrice, size);
        assert.equal(pnl1, "1000");

        // Tick 2: mark = 49800
        const pnl2 = computePnl("49800.00", entryPrice, size);
        assert.equal(pnl2, "-400");

        // Tick 3: mark back to entry
        const pnl3 = computePnl("50000.00", entryPrice, size);
        assert.equal(pnl3, "0");
    });
});
