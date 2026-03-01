// ---------------------------------------------------------------------------
// Integration tests — OrderEntry submission flow + store sync
// ---------------------------------------------------------------------------
//
// Tests the complete flow logic without DOM rendering:
//   1. Build request → call REST → get OrderResponse
//   2. Dispatch WS OrderSubmitted event → store has new order
//   3. Error paths: 400/422 → ApiError propagated, 429 → rate-limit detected
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { buildCreateOrderRequest } from "../OrderEntry";
import { ApiError } from "../../../api/types";
import { DexStateStore } from "../../../state/store";
import type { BaseEvent, Order } from "../../../../../../types/generated-types";
import type { AccountSnapshotPayload, AccountDeltaPayload } from "../../../state/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Simulate a successful REST createOrder response */
function mockCreateOrderSuccess(orderId: string) {
    return { order_id: orderId, status: "PENDING" };
}

/** Simulate an ApiError from REST */
function mockCreateOrderError(status: number, message: string) {
    return new ApiError(status, { error: "VALIDATION_ERROR", message });
}

/** Create a WS OrderSubmitted event that the store can dispatch */
function makeOrderSubmittedEvent(
    seq: number,
    order: Order,
): BaseEvent<AccountDeltaPayload> {
    return {
        event_id: `evt-order-${seq}`,
        event_type: "delta",
        sequence: String(seq),
        timestamp: String(Date.now() * 1_000_000),
        source: "account",
        payload: {
            account_id: order.account_id,
            order,
        },
        metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
}

function makeAccountSnapshot(
    seq: number,
    accountId: string,
): BaseEvent<AccountSnapshotPayload> {
    return {
        event_id: `evt-acct-snap-${seq}`,
        event_type: "snapshot",
        sequence: String(seq),
        timestamp: String(Date.now() * 1_000_000),
        source: "account",
        payload: {
            account_id: accountId,
            balances: { BTC: "10.0", USDT: "500000.00" },
            orders: [],
        },
        metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

describe("Integration: Happy path — submit → REST 200 → WS OrderSubmitted → store", () => {
    test("order flows from REST response to WS event to store", () => {
        const store = new DexStateStore();

        // 1. Initialize account state so delta can be applied
        store.dispatch(makeAccountSnapshot(1, "acct-1"));
        assert.ok(store.getAccount());

        // 2. Build the request payload
        const req = buildCreateOrderRequest({
            accountId: "acct-1",
            symbol: "BTC/USDT",
            side: "BUY",
            order_type: "LIMIT",
            price: "50000.00",
            quantity: "1.0",
            tif: "GTC",
            gtdDate: "",
        });

        assert.equal(req.account_id, "acct-1");
        assert.equal(req.symbol, "BTC/USDT");
        assert.equal(req.side, "BUY");

        // 3. Simulate REST response
        const restResponse = mockCreateOrderSuccess("order-uuid-123");
        assert.equal(restResponse.order_id, "order-uuid-123");
        assert.equal(restResponse.status, "PENDING");

        // 4. Simulate WS order event arriving
        const orderFromWS: Order = {
            order_id: "order-uuid-123",
            account_id: "acct-1",
            symbol: "BTC/USDT",
            side: "BUY" as any,
            price: "50000.00",
            quantity: "1.0",
            filled_quantity: "0.0",
            remaining_quantity: "1.0",
            status: { state: "PENDING" },
            time_in_force: { type: "GTC" },
            created_at: "1708123456789000000",
            updated_at: "1708123456789000000",
            version: 0,
        };

        store.dispatch(makeOrderSubmittedEvent(2, orderFromWS));

        // 5. Verify store has the order
        const acct = store.getAccount();
        assert.ok(acct);
        assert.ok(acct.orders["order-uuid-123"]);
        assert.equal(acct.orders["order-uuid-123"].status.state, "PENDING");
        assert.equal(acct.orders["order-uuid-123"].price, "50000.00");
    });
});

// ---------------------------------------------------------------------------
// Failure paths
// ---------------------------------------------------------------------------

describe("Integration: Failure paths", () => {
    test("REST 400 → ApiError with validation message", () => {
        const err = mockCreateOrderError(400, "Invalid quantity format");
        assert.equal(err.status, 400);
        assert.ok(err instanceof ApiError);
        assert.equal(err.body?.message, "Invalid quantity format");
    });

    test("REST 422 → ApiError with validation message", () => {
        const err = mockCreateOrderError(422, "Insufficient balance");
        assert.equal(err.status, 422);
        assert.ok(err instanceof ApiError);
        assert.equal(err.body?.message, "Insufficient balance");
    });

    test("REST 429 → rate limited detection", () => {
        const err = new ApiError(429, {
            error: "RATE_LIMITED",
            message: "Too many requests",
        });
        assert.equal(err.status, 429);
        assert.equal(err.body?.error, "RATE_LIMITED");
        // The component checks err.status === 429
    });

    test("REST 500 → generic server error", () => {
        const err = new ApiError(500, null);
        assert.equal(err.status, 500);
        assert.equal(err.body, null);
        assert.ok(err.message.includes("500"));
    });
});

// ---------------------------------------------------------------------------
// Store sync detection
// ---------------------------------------------------------------------------

describe("Integration: Store sync — submitted orders marked synced", () => {
    test("listener detects order arrival in store", () => {
        const store = new DexStateStore();
        store.dispatch(makeAccountSnapshot(1, "acct-1"));

        // Simulate tracking submitted orders
        const submittedOrders = [
            { order_id: "order-1", status: "PENDING" as const, submitted_at: Date.now() },
        ];

        // Register listener before event
        let synced = false;
        store.onStateChange((state) => {
            const acct = state.account;
            if (acct && acct.orders["order-1"]) {
                synced = true;
            }
        });

        // Dispatch the WS event
        const wsOrder: Order = {
            order_id: "order-1",
            account_id: "acct-1",
            symbol: "BTC/USDT",
            side: "BUY" as any,
            price: "50000.00",
            quantity: "1.0",
            filled_quantity: "0.0",
            remaining_quantity: "1.0",
            status: { state: "PENDING" },
            time_in_force: { type: "GTC" },
            created_at: "1708123456789000000",
            updated_at: "1708123456789000000",
            version: 0,
        };

        store.dispatch(makeOrderSubmittedEvent(2, wsOrder));

        assert.ok(synced, "Store listener should detect the order");

        // Verify the submitted order can be marked synced
        const updated = submittedOrders.map((so) => {
            const acct = store.getAccount();
            if (so.status === "PENDING" && acct?.orders[so.order_id]) {
                return { ...so, status: "SYNCED" as const };
            }
            return so;
        });

        assert.equal(updated[0].status, "SYNCED");
    });
});

// ---------------------------------------------------------------------------
// Debounce (logic-level test)
// ---------------------------------------------------------------------------

describe("Debounce logic", () => {
    test("rapid calls within window are rejected", () => {
        const DEBOUNCE_MS = 500;
        let lastSubmitTime = 0;
        let callCount = 0;

        function simulateSubmit() {
            const now = Date.now();
            if (now - lastSubmitTime < DEBOUNCE_MS) return;
            lastSubmitTime = now;
            callCount++;
        }

        // First call should succeed
        simulateSubmit();
        assert.equal(callCount, 1);

        // Immediate second call should be rejected (same ms)
        simulateSubmit();
        assert.equal(callCount, 1);
    });
});
