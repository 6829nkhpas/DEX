import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { DexStateStore } from "../../state/store";
import type { BaseEvent } from "../../../../../types/generated-types";
import {
  SubscriptionOrchestrator,
  SubscriptionTransport,
} from "../SubscriptionOrchestrator";
import { AggregatedFeedManager } from "../AggregatedFeedManager";
import type { WsChannel } from "../../ws/types";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createNoopTransport(): SubscriptionTransport {
  return {
    subscribe(_ch: WsChannel, _params: Record<string, string>) { },
    unsubscribe(_ch: WsChannel, _params: Record<string, string>) { },
  };
}

function makeTickerDelta(symbol: string, seq: number): BaseEvent<unknown> {
  return {
    event_id: `ticker-${symbol}-${seq}`,
    event_type: "delta",
    sequence: String(seq),
    timestamp: String(Date.now() * 1_000_000),
    source: "market_data",
    payload: {
      symbol,
      last_price: "50000.00",
      mark_price: "50010.00",
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeOrderbookDelta(symbol: string, seq: number): BaseEvent<unknown> {
  return {
    event_id: `ob-${symbol}-${seq}`,
    event_type: "delta",
    sequence: String(seq),
    timestamp: String(Date.now() * 1_000_000),
    source: "market_data",
    payload: {
      symbol,
      bids: [["50000.00", "1.0"]],
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeSnapshot(symbol: string, seq: number): BaseEvent<unknown> {
  return {
    event_id: `snap-${symbol}-${seq}`,
    event_type: "snapshot",
    sequence: String(seq),
    timestamp: String(Date.now() * 1_000_000),
    source: "market_data",
    payload: {
      symbol,
      bids: [["50000.00", "1.0"]],
      asks: [["50001.00", "1.0"]],
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("AggregatedFeedManager", () => {
  test("normal mode passes all events through", () => {
    const store = new DexStateStore();
    const transport = createNoopTransport();
    const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 5 });
    const agg = new AggregatedFeedManager(store, orch, { threshold: 5 });

    // Only 2 symbols — normal mode
    orch.addListener("BTC/USDT");
    orch.addListener("ETH/USDT");

    assert.ok(!agg.isAggregatedMode());

    // Prime with snapshot
    agg.dispatch(makeSnapshot("BTC/USDT", 1));
    agg.dispatch(makeTickerDelta("BTC/USDT", 2));

    const stats = agg.getStats();
    assert.equal(stats.mode, "normal");
    assert.equal(stats.eventsApplied, 2);
    assert.equal(stats.eventsFiltered, 0);
  });

  test("aggregated mode filters background orderbook deltas", () => {
    const store = new DexStateStore();
    const transport = createNoopTransport();
    const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 2 });
    const agg = new AggregatedFeedManager(store, orch, { threshold: 2, backgroundSampleRate: 1.0 });

    // 3 symbols — crosses threshold
    orch.addListener("BTC/USDT");
    orch.addListener("ETH/USDT");
    orch.addListener("SOL/USDT");
    orch.setFocus("BTC/USDT");

    assert.ok(agg.isAggregatedMode());

    // Prime stores with snapshots for all symbols
    agg.dispatch(makeSnapshot("BTC/USDT", 1));
    agg.dispatch(makeSnapshot("ETH/USDT", 1));

    // Background orderbook delta should be filtered
    agg.dispatch(makeOrderbookDelta("ETH/USDT", 2));

    const stats = agg.getStats();
    assert.equal(stats.eventsFiltered, 1, "Background OB delta should be filtered");
  });

  test("focused symbol always gets full data", () => {
    const store = new DexStateStore();
    const transport = createNoopTransport();
    const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 2 });
    const agg = new AggregatedFeedManager(store, orch, { threshold: 2, backgroundSampleRate: 1.0 });

    orch.addListener("BTC/USDT");
    orch.addListener("ETH/USDT");
    orch.addListener("SOL/USDT");
    orch.setFocus("BTC/USDT");

    // Prime
    agg.dispatch(makeSnapshot("BTC/USDT", 1));

    // Focused symbol OB delta should pass through
    agg.dispatch(makeOrderbookDelta("BTC/USDT", 2));

    const stats = agg.getStats();
    assert.equal(stats.eventsFiltered, 0, "Focused symbol should not be filtered");
    assert.equal(stats.eventsApplied, 2);
  });

  test("snapshots always pass through in aggregated mode", () => {
    const store = new DexStateStore();
    const transport = createNoopTransport();
    const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 2 });
    const agg = new AggregatedFeedManager(store, orch, { threshold: 2 });

    orch.addListener("BTC/USDT");
    orch.addListener("ETH/USDT");
    orch.addListener("SOL/USDT");

    // Even background snapshots should pass through
    agg.dispatch(makeSnapshot("ETH/USDT", 1));

    const stats = agg.getStats();
    assert.equal(stats.eventsApplied, 1);
    assert.equal(stats.eventsFiltered, 0);
  });

  test("reset stats zeroes counters", () => {
    const store = new DexStateStore();
    const transport = createNoopTransport();
    const orch = new SubscriptionOrchestrator(transport);
    const agg = new AggregatedFeedManager(store, orch);

    orch.addListener("BTC/USDT");
    agg.dispatch(makeSnapshot("BTC/USDT", 1));
    agg.dispatch(makeTickerDelta("BTC/USDT", 2));

    assert.equal(agg.getStats().eventsApplied, 2);

    agg.resetStats();
    assert.equal(agg.getStats().eventsApplied, 0);
    assert.equal(agg.getStats().eventsFiltered, 0);
  });
});
