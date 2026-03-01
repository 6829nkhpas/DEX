import { test, describe } from "node:test";
import assert from "node:assert/strict";
import {
  SubscriptionOrchestrator,
  SubscriptionTransport,
} from "../SubscriptionOrchestrator";
import type { WsChannel } from "../../ws/types";

// ---------------------------------------------------------------------------
// Mock transport — records subscribe/unsubscribe calls
// ---------------------------------------------------------------------------

interface TransportCall {
  action: "subscribe" | "unsubscribe";
  channel: WsChannel;
  symbol: string;
}

function createMockTransport(): SubscriptionTransport & { calls: TransportCall[] } {
  const calls: TransportCall[] = [];
  return {
    calls,
    subscribe(channel: WsChannel, params: Record<string, string>) {
      calls.push({ action: "subscribe", channel, symbol: params.symbol });
    },
    unsubscribe(channel: WsChannel, params: Record<string, string>) {
      calls.push({ action: "unsubscribe", channel, symbol: params.symbol });
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SubscriptionOrchestrator", () => {
  // -----------------------------------------------------------------------
  // Duplicate subscriptions
  // -----------------------------------------------------------------------

  describe("duplicate subscriptions", () => {
    test("first addListener subscribes to transport", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");

      assert.ok(orch.isSubscribed("BTC/USDT"));
      assert.equal(orch.getListenerCount("BTC/USDT"), 1);
      // Should have subscribed to at least market_data
      const subCalls = transport.calls.filter((c) => c.action === "subscribe");
      assert.ok(subCalls.length > 0, "Should have transport subscribe calls");
    });

    test("second addListener coalesces — no duplicate transport subscribe", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      const callsAfterFirst = transport.calls.length;

      orch.addListener("BTC/USDT");
      assert.equal(orch.getListenerCount("BTC/USDT"), 2);
      // No additional transport calls
      assert.equal(transport.calls.length, callsAfterFirst);
    });

    test("third addListener still coalesces", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.addListener("BTC/USDT");
      orch.addListener("BTC/USDT");

      assert.equal(orch.getListenerCount("BTC/USDT"), 3);
    });

    test("removing one listener does NOT unsubscribe when others remain", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.addListener("BTC/USDT");

      const callsBefore = transport.calls.length;
      orch.removeListener("BTC/USDT");

      assert.equal(orch.getListenerCount("BTC/USDT"), 1);
      assert.ok(orch.isSubscribed("BTC/USDT"));
      // No unsubscribe calls
      const unsubCalls = transport.calls.slice(callsBefore).filter((c) => c.action === "unsubscribe");
      assert.equal(unsubCalls.length, 0);
    });
  });

  // -----------------------------------------------------------------------
  // Focus switching
  // -----------------------------------------------------------------------

  describe("focus switching", () => {
    test("setting focus promotes symbol to full tier", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.addListener("ETH/USDT");

      orch.setFocus("ETH/USDT");

      assert.equal(orch.getFocusedSymbol(), "ETH/USDT");
      assert.equal(orch.getTier("ETH/USDT"), "full");
    });

    test("switching focus demotes old symbol to ticker-only", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");
      assert.equal(orch.getTier("BTC/USDT"), "full");

      orch.addListener("ETH/USDT");
      orch.setFocus("ETH/USDT");

      assert.equal(orch.getTier("BTC/USDT"), "ticker-only");
      assert.equal(orch.getTier("ETH/USDT"), "full");
    });

    test("focus switch triggers unsubscribe for trades on demoted symbol", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");

      orch.addListener("ETH/USDT");

      // Clear call log
      transport.calls.length = 0;
      orch.setFocus("ETH/USDT");

      // BTC/USDT should have trades unsubscribed (demoted to ticker-only)
      const btcUnsubs = transport.calls.filter(
        (c) => c.action === "unsubscribe" && c.symbol === "BTC/USDT" && c.channel === "trades"
      );
      assert.equal(btcUnsubs.length, 1, "Should unsubscribe trades for demoted symbol");

      // ETH/USDT should have trades subscribed (promoted to full)
      const ethSubs = transport.calls.filter(
        (c) => c.action === "subscribe" && c.symbol === "ETH/USDT" && c.channel === "trades"
      );
      assert.equal(ethSubs.length, 1, "Should subscribe trades for promoted symbol");
    });

    test("setting same focus twice is a no-op", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");

      transport.calls.length = 0;
      orch.setFocus("BTC/USDT");

      assert.equal(transport.calls.length, 0, "No transport calls for redundant focus set");
    });
  });

  // -----------------------------------------------------------------------
  // Symbol removal
  // -----------------------------------------------------------------------

  describe("symbol removal", () => {
    test("removing last listener unsubscribes from all channels", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");

      transport.calls.length = 0;
      orch.removeListener("BTC/USDT");

      assert.ok(!orch.isSubscribed("BTC/USDT"));
      assert.equal(orch.getListenerCount("BTC/USDT"), 0);

      const unsubCalls = transport.calls.filter((c) => c.action === "unsubscribe");
      assert.ok(unsubCalls.length >= 1, "Should have unsubscribe calls");
    });

    test("removing focused symbol clears focus", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");
      orch.removeListener("BTC/USDT");

      assert.equal(orch.getFocusedSymbol(), null);
    });

    test("removing non-existent symbol is a no-op", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      // Should not throw
      orch.removeListener("DOES_NOT_EXIST");
      assert.equal(transport.calls.length, 0);
    });

    test("re-adding after removal creates fresh subscription", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.removeListener("BTC/USDT");
      assert.ok(!orch.isSubscribed("BTC/USDT"));

      transport.calls.length = 0;
      orch.addListener("BTC/USDT");
      assert.ok(orch.isSubscribed("BTC/USDT"));
      assert.equal(orch.getListenerCount("BTC/USDT"), 1);

      const subCalls = transport.calls.filter((c) => c.action === "subscribe");
      assert.ok(subCalls.length > 0, "Should subscribe again after re-add");
    });
  });

  // -----------------------------------------------------------------------
  // Aggregation mode
  // -----------------------------------------------------------------------

  describe("aggregation mode", () => {
    test("activates when symbol count exceeds threshold", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 3 });

      orch.addListener("BTC/USDT");
      orch.addListener("ETH/USDT");
      orch.addListener("SOL/USDT");
      assert.ok(!orch.isAggregatedMode());

      orch.addListener("AVAX/USDT");
      assert.ok(orch.isAggregatedMode());
    });

    test("deactivates when symbols drop below threshold", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport, { aggregationThreshold: 3 });

      orch.addListener("BTC/USDT");
      orch.addListener("ETH/USDT");
      orch.addListener("SOL/USDT");
      orch.addListener("AVAX/USDT");
      assert.ok(orch.isAggregatedMode());

      orch.removeListener("AVAX/USDT");
      assert.ok(!orch.isAggregatedMode());
    });
  });

  // -----------------------------------------------------------------------
  // Snapshot & dispose
  // -----------------------------------------------------------------------

  describe("snapshot and dispose", () => {
    test("getSnapshot returns correct state", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.addListener("BTC/USDT");
      orch.addListener("ETH/USDT");
      orch.setFocus("BTC/USDT");

      const snap = orch.getSnapshot();
      assert.equal(snap.focusedSymbol, "BTC/USDT");
      assert.equal(snap.totalListeners, 3);
      assert.equal(snap.aggregatedMode, false);
      assert.equal(snap.subscriptions.size, 2);
    });

    test("dispose unsubscribes everything", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.setFocus("BTC/USDT");
      orch.addListener("ETH/USDT");

      transport.calls.length = 0;
      orch.dispose();

      assert.equal(orch.getFocusedSymbol(), null);
      assert.equal(orch.getSubscribedSymbols().length, 0);

      const unsubCalls = transport.calls.filter((c) => c.action === "unsubscribe");
      assert.ok(unsubCalls.length >= 2, "Should unsubscribe all channels for all symbols");
    });
  });

  // -----------------------------------------------------------------------
  // getSubscribedSymbols
  // -----------------------------------------------------------------------

  describe("getSubscribedSymbols", () => {
    test("returns list of all subscribed symbols", () => {
      const transport = createMockTransport();
      const orch = new SubscriptionOrchestrator(transport);

      orch.addListener("BTC/USDT");
      orch.addListener("ETH/USDT");
      orch.addListener("SOL/USDT");

      const symbols = orch.getSubscribedSymbols();
      assert.equal(symbols.length, 3);
      assert.ok(symbols.includes("BTC/USDT"));
      assert.ok(symbols.includes("ETH/USDT"));
      assert.ok(symbols.includes("SOL/USDT"));
    });
  });
});
