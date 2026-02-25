import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { DexStateStore } from "../store";
import type { BaseEvent } from "../../../../../types/generated-types";
import type { OrderbookSnapshotPayload, OrderbookDeltaPayload } from "../types";

// Helper to make events
function makeSnapshot(seq: number): BaseEvent<OrderbookSnapshotPayload> {
    return {
        event_id: `snap-${seq}`,
        event_type: "snapshot",
        source: "market_data",
        sequence: String(seq),
        timestamp: "1000",
        payload: {
            symbol: "BTC_USD",
            bids: [["50000.00", "1.0"]],
            asks: [["50010.00", "1.0"]]
        },
        metadata: { version: "1", correlation_id: "", causation_id: "" }
    } as BaseEvent<OrderbookSnapshotPayload>;
}

function makeDelta(seq: number): BaseEvent<OrderbookDeltaPayload> {
    return {
        event_id: `delta-${seq}`,
        event_type: "delta",
        source: "market_data",
        sequence: String(seq),
        timestamp: "1000",
        payload: {
            symbol: "BTC_USD",
            bids: [[String(50000 + seq), "0.5"]]
        },
        metadata: { version: "1", correlation_id: "", causation_id: "" }
    } as BaseEvent<OrderbookDeltaPayload>;
}

describe("DexStateStore - Snapshot Atomicity & Buffering", () => {
    test("in-order delta flow", () => {
        const store = new DexStateStore();
        store.dispatch(makeSnapshot(100));
        assert.equal(store.getState().metrics.events_ignored, 0);

        store.dispatch(makeDelta(101));
        assert.equal(store.getState().metrics.gaps_detected, 0);

        const ob = store.getOrderbook("BTC_USD");
        assert.equal(ob?.lastSeq, "101");
        assert.equal(ob?.bids.length, 2);
    });

    test("Pre-snapshot buffered deltas replay over snapshot safely", () => {
        const store = new DexStateStore();
        // Send delta 101, gap detected because seq=0
        store.dispatch(makeDelta(101));
        assert.equal(store.getState().metrics.gaps_detected, 0); // Not gap, meta.lastSeq === 0 so it just buffers
        assert.equal(store.getState().metrics.buffer_size_by_stream.get("market_data::BTC_USD"), 1);

        // Snapshot arrives with seq=100
        store.dispatch(makeSnapshot(100));

        // The snapshot should naturally flush 101
        const ob = store.getOrderbook("BTC_USD");
        assert.equal(ob?.lastSeq, "101"); // 100 replaced, then 101 applied
        assert.equal(store.getState().metrics.buffer_size_by_stream.get("market_data::BTC_USD"), 0);
    });

    test("Sequence gap buffering -> triggers snapshot_since", (t) => {
        const store = new DexStateStore();
        store.dispatch(makeSnapshot(100));

        let snapshotRequested = false;
        store.onRequestSnapshot((channel, params, sinceSeq) => {
            snapshotRequested = true;
            assert.equal(channel, "market_data");
            assert.equal(params.symbol, "BTC_USD");
            assert.equal(sinceSeq, 100);
        });

        // Skip 101, send 102
        store.dispatch(makeDelta(102));
        assert.equal(store.getState().metrics.gaps_detected, 1);
        assert.equal(snapshotRequested, true);
        assert.equal(store.getState().metrics.buffer_size_by_stream.get("market_data::BTC_USD"), 1);

        // Now send 101, which fills the gap and flushes 102
        store.dispatch(makeDelta(101));
        const ob = store.getOrderbook("BTC_USD");
        assert.equal(ob?.lastSeq, "102");
        assert.equal(store.getState().metrics.buffer_size_by_stream.get("market_data::BTC_USD"), 0);
    });

    test("Duplicates are ignored", () => {
        const store = new DexStateStore();
        store.dispatch(makeSnapshot(100));
        store.dispatch(makeDelta(101));

        // Duplicate <= lastSeq
        store.dispatch(makeDelta(100));
        assert.equal(store.getState().metrics.events_ignored, 1);

        // Exact ID duplicate
        store.dispatch(makeDelta(101));
        assert.equal(store.getState().metrics.events_ignored, 2);
    });

    test("Buffer cap overflow clears and requests complete snapshot", () => {
        const store = new DexStateStore();
        let requests: { sinceSeq: number }[] = [];
        store.onRequestSnapshot((channel, params, sinceSeq) => {
            requests.push({ sinceSeq });
        });

        // Initialize state so it has lastSeq = 100
        store.dispatch(makeSnapshot(100));

        // Overflow buffer up to MAX_BUFFER_SIZE + 1 items
        // We trigger gaps so they all get buffered
        // Sequence should be 1000, 1001... leaving gap at 101
        for (let i = 0; i <= 10000; i++) {
            store.dispatch(makeDelta(1000 + i));
        }

        // It should request partial snapshots for gap buffering, but then on overflow it asks for `sinceSeq: 0`.
        assert.equal(store.getState().metrics.buffer_size_by_stream.get("market_data::BTC_USD"), 0);
        const lastReq = requests[requests.length - 1];
        assert.equal(lastReq.sinceSeq, 0); // The overflow triggered total reset request
    });
});
