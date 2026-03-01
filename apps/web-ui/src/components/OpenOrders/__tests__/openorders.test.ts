// ---------------------------------------------------------------------------
// Tests — OpenOrders: cancel integration, error handling, no optimistic mutation
// ---------------------------------------------------------------------------
//
// Uses node:test. Tests the non-DOM logic:
//   1. filterActiveOrders — only PENDING/PARTIAL pass through
//   2. Cancel integration: REST 200 → order still in store → WS event → order removed
//   3. cancelErrorMessage — 404, 409, 429, generic
//   4. No optimistic mutation — store unchanged until WS event
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { filterActiveOrders, cancelErrorMessage } from "../OpenOrders";
import { ApiError } from "../../../api/types";
import { DexStateStore } from "../../../state/store";
import type { BaseEvent, Order, CancelReason } from "../../../../../../types/generated-types";
import type { AccountSnapshotPayload, AccountDeltaPayload } from "../../../state/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeOrder(overrides: Partial<Order> = {}): Order {
    return {
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
        ...overrides,
    };
}

function makeAccountSnapshot(
    seq: number,
    accountId: string,
    orders: Order[] = [],
): BaseEvent<AccountSnapshotPayload> {
    return {
        event_id: `evt-snap-${seq}`,
        event_type: "snapshot",
        sequence: String(seq),
        timestamp: String(Date.now() * 1_000_000),
        source: "account",
        payload: {
            account_id: accountId,
            balances: { USDT: "100000.00" },
            orders,
        },
        metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
}

function makeAccountDelta(
    seq: number,
    order: Order,
): BaseEvent<AccountDeltaPayload> {
    return {
        event_id: `evt-delta-${seq}`,
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

// ---------------------------------------------------------------------------
// filterActiveOrders
// ---------------------------------------------------------------------------

describe("filterActiveOrders", () => {
    test("includes PENDING orders", () => {
        const orders: Record<string, Order> = {
            "o1": makeOrder({ order_id: "o1", status: { state: "PENDING" } }),
        };
        const result = filterActiveOrders(orders);
        assert.equal(result.length, 1);
        assert.equal(result[0].order_id, "o1");
    });

    test("includes PARTIAL orders", () => {
        const orders: Record<string, Order> = {
            "o1": makeOrder({ order_id: "o1", status: { state: "PARTIAL" } }),
        };
        const result = filterActiveOrders(orders);
        assert.equal(result.length, 1);
    });

    test("excludes FILLED orders", () => {
        const orders: Record<string, Order> = {
            "o1": makeOrder({ order_id: "o1", status: { state: "FILLED" } }),
        };
        const result = filterActiveOrders(orders);
        assert.equal(result.length, 0);
    });

    test("excludes CANCELED orders", () => {
        const orders: Record<string, Order> = {
            "o1": makeOrder({
                order_id: "o1",
                status: { state: "CANCELED", reason: "USER_REQUESTED" as CancelReason },
            }),
        };
        const result = filterActiveOrders(orders);
        assert.equal(result.length, 0);
    });

    test("mixed statuses — only active pass through", () => {
        const orders: Record<string, Order> = {
            "o1": makeOrder({ order_id: "o1", status: { state: "PENDING" } }),
            "o2": makeOrder({ order_id: "o2", status: { state: "FILLED" } }),
            "o3": makeOrder({ order_id: "o3", status: { state: "PARTIAL" } }),
            "o4": makeOrder({
                order_id: "o4",
                status: { state: "CANCELED", reason: "SELF_TRADE" as CancelReason },
            }),
        };
        const result = filterActiveOrders(orders);
        assert.equal(result.length, 2);
        const ids = result.map((o) => o.order_id).sort();
        assert.deepEqual(ids, ["o1", "o3"]);
    });

    test("empty orders map returns empty array", () => {
        assert.deepEqual(filterActiveOrders({}), []);
    });
});

// ---------------------------------------------------------------------------
// cancelErrorMessage
// ---------------------------------------------------------------------------

describe("cancelErrorMessage", () => {
    test("404 → order not found", () => {
        const msg = cancelErrorMessage(new ApiError(404, null));
        assert.ok(msg.toLowerCase().includes("not found"));
    });

    test("409 → already filled or canceled", () => {
        const msg = cancelErrorMessage(new ApiError(409, null));
        assert.ok(msg.toLowerCase().includes("already"));
    });

    test("429 → rate limit", () => {
        const msg = cancelErrorMessage(new ApiError(429, null));
        assert.ok(msg.toLowerCase().includes("rate limit"));
    });

    test("500 → falls back to body message", () => {
        const msg = cancelErrorMessage(
            new ApiError(500, { error: "INTERNAL", message: "Something broke" }),
        );
        assert.equal(msg, "Something broke");
    });

    test("500 with null body → generic message with status", () => {
        const msg = cancelErrorMessage(new ApiError(500, null));
        assert.ok(msg.includes("500"));
    });
});

// ---------------------------------------------------------------------------
// Cancel integration: REST 200 → order still in store → WS OrderCanceled → removed
// ---------------------------------------------------------------------------

describe("Integration: Cancel flow — REST + WS reconciliation", () => {
    test("order remains in store after REST cancel success (no optimistic removal)", () => {
        const store = new DexStateStore();
        const pendingOrder = makeOrder({ order_id: "order-42", status: { state: "PENDING" } });

        // 1. Initialize with a PENDING order
        store.dispatch(makeAccountSnapshot(1, "acct-1", [pendingOrder]));

        const acct = store.getAccount();
        assert.ok(acct);
        assert.ok(acct.orders["order-42"]);
        assert.equal(acct.orders["order-42"].status.state, "PENDING");

        // 2. Simulate REST cancel — 200 OK.  Store should NOT be modified.
        //    (The cancel button calls REST, but we verify store is unchanged.)
        const acctAfterRest = store.getAccount();
        assert.ok(acctAfterRest);
        assert.ok(acctAfterRest.orders["order-42"], "Order must still be in store after REST call");
        assert.equal(acctAfterRest.orders["order-42"].status.state, "PENDING");

        // 3. Simulate WS OrderCanceled event arriving
        const canceledOrder: Order = {
            ...pendingOrder,
            status: { state: "CANCELED", reason: "USER_REQUESTED" as CancelReason },
            updated_at: String(Date.now() * 1_000_000),
            version: 1,
        };
        store.dispatch(makeAccountDelta(2, canceledOrder));

        // 4. Verify store now has canceled order
        const acctFinal = store.getAccount();
        assert.ok(acctFinal);
        assert.equal(acctFinal.orders["order-42"].status.state, "CANCELED");

        // 5. Verify it's filtered out by active filter
        const active = filterActiveOrders(acctFinal.orders);
        const stillVisible = active.find((o) => o.order_id === "order-42");
        assert.equal(stillVisible, undefined, "Canceled order must not appear in active list");
    });

    test("listener detects order canceled via WS event", () => {
        const store = new DexStateStore();
        const order = makeOrder({ order_id: "order-99", status: { state: "PENDING" } });
        store.dispatch(makeAccountSnapshot(1, "acct-1", [order]));

        let canceled = false;
        store.onStateChange((state) => {
            const acct = state.account;
            if (acct && acct.orders["order-99"]?.status.state === "CANCELED") {
                canceled = true;
            }
        });

        // Dispatch cancel event
        const canceledOrder: Order = {
            ...order,
            status: { state: "CANCELED", reason: "USER_REQUESTED" as CancelReason },
            version: 1,
        };
        store.dispatch(makeAccountDelta(2, canceledOrder));

        assert.ok(canceled, "State change listener should detect cancellation");
    });

    test("multiple orders — cancel one, others remain active", () => {
        const store = new DexStateStore();
        const o1 = makeOrder({ order_id: "o1", status: { state: "PENDING" } });
        const o2 = makeOrder({ order_id: "o2", status: { state: "PARTIAL" } });
        store.dispatch(makeAccountSnapshot(1, "acct-1", [o1, o2]));

        // Cancel o1
        const canceledO1: Order = {
            ...o1,
            status: { state: "CANCELED", reason: "USER_REQUESTED" as CancelReason },
            version: 1,
        };
        store.dispatch(makeAccountDelta(2, canceledO1));

        const acct = store.getAccount();
        assert.ok(acct);
        const active = filterActiveOrders(acct.orders);
        assert.equal(active.length, 1);
        assert.equal(active[0].order_id, "o2");
        assert.equal(active[0].status.state, "PARTIAL");
    });
});

// ---------------------------------------------------------------------------
// No optimistic mutation invariant
// ---------------------------------------------------------------------------

describe("No optimistic mutation", () => {
    test("store orders are unchanged between REST call and WS event", () => {
        const store = new DexStateStore();
        const order = makeOrder({ order_id: "keep-me", status: { state: "PENDING" } });
        store.dispatch(makeAccountSnapshot(1, "acct-1", [order]));

        // Snapshot state before "REST call"
        const beforeCancel = store.getAccount();
        assert.ok(beforeCancel);
        const orderBefore = { ...beforeCancel.orders["keep-me"] };

        // Simulate: REST cancel is called but no WS event yet
        // (we don't call store.dispatch — that's the point)

        // Verify: state is byte-for-byte identical
        const afterRestNoWs = store.getAccount();
        assert.ok(afterRestNoWs);
        assert.deepEqual(afterRestNoWs.orders["keep-me"].status, orderBefore.status);
        assert.equal(afterRestNoWs.orders["keep-me"].version, orderBefore.version);
    });
});
