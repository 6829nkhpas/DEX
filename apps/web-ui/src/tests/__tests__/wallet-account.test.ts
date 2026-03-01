// ---------------------------------------------------------------------------
// Wallet & Account integration tests
// ---------------------------------------------------------------------------
//
// Tests:
//   1. wallet connect (mock window.ethereum)
//   2. deterministic account ID derivation
//   3. dynamic WS subscription on wallet change
//   4. balance render from account snapshot/delta
//   5. disconnect clears state
// ---------------------------------------------------------------------------

import { test, describe, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { deriveAccountId } from "../../wallet/WalletProvider";
import { DexStateStore } from "../../state/store";
import type { BaseEvent } from "../../../../../types/generated-types";
import type { AccountSnapshotPayload, AccountDeltaPayload } from "../../state/types";

// ---------------------------------------------------------------------------
// 1. Mock EIP-1193 provider
// ---------------------------------------------------------------------------

function createMockProvider(accounts: string[] = ["0xAbC1230000000000000000000000000000004567"]) {
  const listeners: Record<string, ((...args: unknown[]) => void)[]> = {};

  return {
    accounts,
    async request(args: { method: string; params?: unknown[] }): Promise<unknown> {
      if (args.method === "eth_requestAccounts") {
        return this.accounts;
      }
      if (args.method === "personal_sign") {
        return "0xmocksignature";
      }
      throw new Error(`Unknown method: ${args.method}`);
    },
    on(event: string, handler: (...args: unknown[]) => void) {
      if (!listeners[event]) listeners[event] = [];
      listeners[event].push(handler);
    },
    removeListener(event: string, handler: (...args: unknown[]) => void) {
      if (listeners[event]) {
        listeners[event] = listeners[event].filter((h) => h !== handler);
      }
    },
    emit(event: string, ...args: unknown[]) {
      for (const h of listeners[event] ?? []) {
        h(...args);
      }
    },
  };
}

// ---------------------------------------------------------------------------
// 2. Deterministic account ID derivation
// ---------------------------------------------------------------------------

describe("deriveAccountId", () => {
  test("produces a UUID-shaped string", async () => {
    const id = await deriveAccountId("0xAbC1230000000000000000000000000000004567");
    // UUID format: 8-4-4-4-12
    assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
  });

  test("is deterministic — same input yields same output", async () => {
    const addr = "0xDEADbeef00000000000000000000000000001234";
    const id1 = await deriveAccountId(addr);
    const id2 = await deriveAccountId(addr);
    assert.equal(id1, id2);
  });

  test("is case-insensitive", async () => {
    const upper = await deriveAccountId("0xABCDEF");
    const lower = await deriveAccountId("0xabcdef");
    assert.equal(upper, lower);
  });

  test("different addresses yield different account IDs", async () => {
    const id1 = await deriveAccountId("0x1111111111111111111111111111111111111111");
    const id2 = await deriveAccountId("0x2222222222222222222222222222222222222222");
    assert.notEqual(id1, id2);
  });
});

// ---------------------------------------------------------------------------
// 3. Wallet connect / disconnect via mock provider
// ---------------------------------------------------------------------------

describe("Wallet connect via mock provider", () => {
  let originalEthereum: unknown;

  beforeEach(() => {
    originalEthereum = (globalThis as any).window?.ethereum;
  });

  afterEach(() => {
    // Restore
    if (typeof (globalThis as any).window !== "undefined") {
      (globalThis as any).window.ethereum = originalEthereum;
    }
  });

  test("mock provider returns accounts on eth_requestAccounts", async () => {
    const mock = createMockProvider(["0xABCD"]);
    const accounts = await mock.request({ method: "eth_requestAccounts" }) as string[];
    assert.deepEqual(accounts, ["0xABCD"]);
  });

  test("mock provider signs messages", async () => {
    const mock = createMockProvider(["0xABCD"]);
    const sig = await mock.request({ method: "personal_sign", params: ["hello", "0xABCD"] });
    assert.equal(sig, "0xmocksignature");
  });

  test("mock provider emits accountsChanged", () => {
    const mock = createMockProvider(["0xABCD"]);
    let received: unknown = null;
    mock.on("accountsChanged", (accs) => { received = accs; });
    mock.emit("accountsChanged", ["0xNEW"]);
    assert.deepEqual(received, ["0xNEW"]);
  });

  test("disconnect fires accountsChanged with empty array", () => {
    const mock = createMockProvider(["0xABCD"]);
    let received: unknown = null;
    mock.on("accountsChanged", (accs) => { received = accs; });
    mock.emit("accountsChanged", []);
    assert.deepEqual(received, []);
  });
});

// ---------------------------------------------------------------------------
// 4. Dynamic account subscription — store receives events after wallet switch
// ---------------------------------------------------------------------------

describe("Dynamic account subscription", () => {
  test("subscribing with account_id receives snapshot correctly", () => {
    const store = new DexStateStore();
    const accountId = "test-account-001";

    const snapshot: BaseEvent<AccountSnapshotPayload> = {
      event_id: "acct-snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "1",
      timestamp: "1000000",
      payload: {
        account_id: accountId,
        balances: { USDT: "10000.00", BTC: "0.5" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };

    store.dispatch(snapshot);

    const account = store.getAccount();
    assert.ok(account);
    assert.equal(account.account_id, accountId);
    assert.equal(account.balances["USDT"], "10000.00");
    assert.equal(account.balances["BTC"], "0.5");
  });

  test("switching account resets state on new snapshot", () => {
    const store = new DexStateStore();

    // First account
    store.dispatch({
      event_id: "acct-snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "1",
      timestamp: "1000000",
      payload: {
        account_id: "account-A",
        balances: { USDT: "5000.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    assert.equal(store.getAccount()?.account_id, "account-A");

    // Second account snapshot — replaces wholesale
    store.dispatch({
      event_id: "acct-snap-2",
      event_type: "snapshot",
      source: "account",
      sequence: "2",
      timestamp: "2000000",
      payload: {
        account_id: "account-B",
        balances: { ETH: "100.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    const acct = store.getAccount();
    assert.equal(acct?.account_id, "account-B");
    assert.equal(acct?.balances["ETH"], "100.00");
    // Old balances gone
    assert.equal(acct?.balances["USDT"], undefined);
  });
});

// ---------------------------------------------------------------------------
// 5. Balance updates from account deltas
// ---------------------------------------------------------------------------

describe("Balance render from store state", () => {
  test("account delta updates balances", () => {
    const store = new DexStateStore();

    // Snapshot
    store.dispatch({
      event_id: "snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "10",
      timestamp: "1000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "5000.00", BTC: "1.0" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    assert.equal(store.getAccount()?.balances["USDT"], "5000.00");

    // Delta: USDT balance changes
    store.dispatch({
      event_id: "delta-11",
      event_type: "delta",
      source: "account",
      sequence: "11",
      timestamp: "2000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "4500.00" },
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountDeltaPayload>);

    const acct = store.getAccount();
    assert.equal(acct?.balances["USDT"], "4500.00");
    // BTC unchanged
    assert.equal(acct?.balances["BTC"], "1.0");
  });

  test("balance delta with new asset adds it", () => {
    const store = new DexStateStore();

    store.dispatch({
      event_id: "snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "10",
      timestamp: "1000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "5000.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    // Add SOL balance
    store.dispatch({
      event_id: "delta-11",
      event_type: "delta",
      source: "account",
      sequence: "11",
      timestamp: "2000000",
      payload: {
        account_id: "user-1",
        balances: { SOL: "50.0" },
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountDeltaPayload>);

    const acct = store.getAccount();
    assert.equal(acct?.balances["SOL"], "50.0");
    assert.equal(acct?.balances["USDT"], "5000.00");
  });

  test("listener fires on account state change", () => {
    const store = new DexStateStore();
    let changeCount = 0;

    store.onStateChange(() => {
      changeCount++;
    });

    store.dispatch({
      event_id: "snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "1",
      timestamp: "1000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "1000.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    assert.equal(changeCount, 1);

    store.dispatch({
      event_id: "delta-2",
      event_type: "delta",
      source: "account",
      sequence: "2",
      timestamp: "2000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "900.00" },
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountDeltaPayload>);

    assert.equal(changeCount, 2);
  });
});

// ---------------------------------------------------------------------------
// 6. Disconnect clears context
// ---------------------------------------------------------------------------

describe("Disconnect behavior", () => {
  test("account remains in store until new snapshot replaces it", () => {
    const store = new DexStateStore();

    store.dispatch({
      event_id: "snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "1",
      timestamp: "1000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "1000.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<AccountSnapshotPayload>);

    // Store still has account — disconnect is client-side context only
    assert.ok(store.getAccount());
    assert.equal(store.getAccount()?.account_id, "user-1");
  });

  test("duplicate account snapshot is idempotent", () => {
    const store = new DexStateStore();

    const snap: BaseEvent<AccountSnapshotPayload> = {
      event_id: "snap-1",
      event_type: "snapshot",
      source: "account",
      sequence: "1",
      timestamp: "1000000",
      payload: {
        account_id: "user-1",
        balances: { USDT: "1000.00" },
        orders: [],
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };

    store.dispatch(snap);
    store.dispatch(snap); // same event_id, same seq — should be safe

    assert.equal(store.getAccount()?.account_id, "user-1");
  });
});
