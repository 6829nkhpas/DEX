// ---------------------------------------------------------------------------
// Unit tests — OrderEntry validation & payload composition
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import {
    isPositiveDecimal,
    isValidDecimal,
    validateOrder,
    buildCreateOrderRequest,
} from "../OrderEntry";

// ---------------------------------------------------------------------------
// isPositiveDecimal
// ---------------------------------------------------------------------------

describe("isPositiveDecimal", () => {
    test("valid positive decimals", () => {
        assert.ok(isPositiveDecimal("1"));
        assert.ok(isPositiveDecimal("0.001"));
        assert.ok(isPositiveDecimal("50000.00"));
        assert.ok(isPositiveDecimal("99999999.12345678"));
    });

    test("zero is NOT positive", () => {
        assert.equal(isPositiveDecimal("0"), false);
        assert.equal(isPositiveDecimal("0.0"), false);
        assert.equal(isPositiveDecimal("0.00"), false);
    });

    test("negative values", () => {
        assert.equal(isPositiveDecimal("-1"), false);
        assert.equal(isPositiveDecimal("-0.01"), false);
    });

    test("invalid strings", () => {
        assert.equal(isPositiveDecimal(""), false);
        assert.equal(isPositiveDecimal("abc"), false);
        assert.equal(isPositiveDecimal("12.34.56"), false);
        assert.equal(isPositiveDecimal("  "), false);
    });

    test("edge: very large numbers", () => {
        assert.ok(isPositiveDecimal("999999999999999999.999999999999999999"));
    });
});

// ---------------------------------------------------------------------------
// isValidDecimal
// ---------------------------------------------------------------------------

describe("isValidDecimal", () => {
    test("accepts zero", () => {
        assert.ok(isValidDecimal("0"));
        assert.ok(isValidDecimal("0.0"));
    });

    test("accepts positive and negative", () => {
        assert.ok(isValidDecimal("1.5"));
        assert.ok(isValidDecimal("-1.5"));
    });

    test("rejects junk", () => {
        assert.equal(isValidDecimal(""), false);
        assert.equal(isValidDecimal("xyz"), false);
    });
});

// ---------------------------------------------------------------------------
// validateOrder
// ---------------------------------------------------------------------------

describe("validateOrder", () => {
    const validFields = {
        side: "BUY",
        order_type: "LIMIT",
        price: "50000.00",
        quantity: "1.0",
        tif: "GTC",
        gtdDate: "",
    };

    test("valid LIMIT order — no errors", () => {
        const errors = validateOrder(validFields);
        assert.deepEqual(errors, {});
    });

    test("missing side", () => {
        const errors = validateOrder({ ...validFields, side: "" });
        assert.ok(errors.side);
    });

    test("missing order_type", () => {
        const errors = validateOrder({ ...validFields, order_type: "" });
        assert.ok(errors.order_type);
    });

    test("missing quantity", () => {
        const errors = validateOrder({ ...validFields, quantity: "" });
        assert.ok(errors.quantity);
    });

    test("non-positive quantity", () => {
        const errors = validateOrder({ ...validFields, quantity: "0" });
        assert.ok(errors.quantity);
    });

    test("invalid quantity (non-decimal)", () => {
        const errors = validateOrder({ ...validFields, quantity: "abc" });
        assert.ok(errors.quantity);
    });

    test("missing price on LIMIT order", () => {
        const errors = validateOrder({ ...validFields, price: "" });
        assert.ok(errors.price);
    });

    test("non-positive price on LIMIT order", () => {
        const errors = validateOrder({ ...validFields, price: "-1" });
        assert.ok(errors.price);
    });

    test("MARKET order — price not required", () => {
        const errors = validateOrder({
            ...validFields,
            order_type: "MARKET",
            price: "",
        });
        assert.equal(errors.price, undefined);
    });

    test("missing TIF", () => {
        const errors = validateOrder({ ...validFields, tif: "" });
        assert.ok(errors.time_in_force);
    });

    test("GTD without date", () => {
        const errors = validateOrder({ ...validFields, tif: "GTD", gtdDate: "" });
        assert.ok(errors.gtd_date);
    });

    test("GTD with date — no error", () => {
        const errors = validateOrder({
            ...validFields,
            tif: "GTD",
            gtdDate: "2026-12-31T23:59",
        });
        assert.equal(errors.gtd_date, undefined);
    });

    test("multiple errors at once", () => {
        const errors = validateOrder({
            side: "",
            order_type: "",
            price: "",
            quantity: "",
            tif: "",
            gtdDate: "",
        });
        assert.ok(errors.side);
        assert.ok(errors.order_type);
        assert.ok(errors.quantity);
        assert.ok(errors.time_in_force);
    });
});

// ---------------------------------------------------------------------------
// buildCreateOrderRequest
// ---------------------------------------------------------------------------

describe("buildCreateOrderRequest", () => {
    test("LIMIT GTC order payload", () => {
        const req = buildCreateOrderRequest({
            accountId: "acct-1",
            symbol: "BTC/USDT",
            side: "BUY",
            order_type: "LIMIT",
            price: "50000.00",
            quantity: "1.5",
            tif: "GTC",
            gtdDate: "",
        });

        assert.equal(req.account_id, "acct-1");
        assert.equal(req.symbol, "BTC/USDT");
        assert.equal(req.side, "BUY");
        assert.equal(req.order_type, "LIMIT");
        assert.equal(req.price, "50000.00");
        assert.equal(req.quantity, "1.5");
        assert.deepEqual(req.time_in_force, { type: "GTC" });
    });

    test("MARKET order uses price '0'", () => {
        const req = buildCreateOrderRequest({
            accountId: "acct-1",
            symbol: "ETH/USDT",
            side: "SELL",
            order_type: "MARKET",
            price: "", // ignored for MARKET
            quantity: "2.0",
            tif: "IOC",
            gtdDate: "",
        });

        assert.equal(req.order_type, "MARKET");
        assert.equal(req.price, "0");
        assert.deepEqual(req.time_in_force, { type: "IOC" });
    });

    test("GTD includes Unix nanos timestamp", () => {
        const dateStr = "2026-12-31T23:59:00";
        const req = buildCreateOrderRequest({
            accountId: "acct-1",
            symbol: "BTC/USDT",
            side: "BUY",
            order_type: "LIMIT",
            price: "50000.00",
            quantity: "1.0",
            tif: "GTD",
            gtdDate: dateStr,
        });

        assert.equal(req.time_in_force.type, "GTD");
        // Must be a GTD with a string value (Unix nanos)
        if (req.time_in_force.type === "GTD") {
            const nanos = BigInt(req.time_in_force.value);
            assert.ok(nanos > 0n);
            // Should roughly match the date in ms * 1e6
            const expectedMs = new Date(dateStr).getTime();
            const expectedNanos = BigInt(expectedMs) * 1_000_000n;
            assert.equal(nanos, expectedNanos);
        }
    });

    test("FOK time-in-force", () => {
        const req = buildCreateOrderRequest({
            accountId: "acct-1",
            symbol: "SOL/USDT",
            side: "BUY",
            order_type: "LIMIT",
            price: "100.00",
            quantity: "10",
            tif: "FOK",
            gtdDate: "",
        });
        assert.deepEqual(req.time_in_force, { type: "FOK" });
    });
});

// ---------------------------------------------------------------------------
// ApiError & 429 handling (unit-level)
// ---------------------------------------------------------------------------

describe("ApiError shape", () => {
    // Import the class to verify its constructor
    test("ApiError stores status and body", async () => {
        const { ApiError } = await import("../../../api/types");
        const err = new ApiError(429, { error: "RATE_LIMITED", message: "Too many requests" });
        assert.equal(err.status, 429);
        assert.equal(err.body?.error, "RATE_LIMITED");
        assert.equal(err.body?.message, "Too many requests");
        assert.equal(err.name, "ApiError");
        assert.ok(err.message.includes("Too many requests"));
    });

    test("ApiError with null body", async () => {
        const { ApiError } = await import("../../../api/types");
        const err = new ApiError(500, null);
        assert.equal(err.status, 500);
        assert.equal(err.body, null);
        assert.ok(err.message.includes("500"));
    });
});
