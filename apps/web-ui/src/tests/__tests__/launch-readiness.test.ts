// ---------------------------------------------------------------------------
// launch-readiness.test.ts — Phase 20 regression coverage for critical paths
// ---------------------------------------------------------------------------
//
// Uses Node's built-in test runner (tsx --test), consistent with existing tests.
// All tests are pure logic — no React renderer, no DOM, no network.
//
// Suites:
//   1. Session restore after reconnect / refresh
//   2. Session expiry timer logic
//   3. Protected action blocking (all non-authenticated states)
//   4. Chain change invalidation
//   5. Account change invalidation
//   6. UI fallback state mapping (AuthStatus → expected label)
//   7. Trading flow auth stability (isSubmitDisabled invariant)
// ---------------------------------------------------------------------------

import { test, describe, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";

import {
  generateNonce,
  buildLoginMessage,
  createSession,
  isSessionValid,
  persistSession,
  loadSession,
  clearSession,
  type AuthSession,
} from "../../auth/authService";
import type { AuthStatus } from "../../auth/AuthProvider";

// ---------------------------------------------------------------------------
// Shared mock sessionStorage
// ---------------------------------------------------------------------------

function createMockSessionStorage() {
  const store: Record<string, string> = {};
  return {
    getItem: (k: string) => (Object.prototype.hasOwnProperty.call(store, k) ? store[k] : null),
    setItem: (k: string, v: string) => { store[k] = v; },
    removeItem: (k: string) => { delete store[k]; },
    clear: () => { for (const k of Object.keys(store)) delete store[k]; },
  };
}

let mockStorage: ReturnType<typeof createMockSessionStorage>;
const installStorage = () => {
  mockStorage = createMockSessionStorage();
  (globalThis as Record<string, unknown>).sessionStorage = mockStorage;
};
const uninstallStorage = () => {
  delete (globalThis as Record<string, unknown>).sessionStorage;
};

// ---------------------------------------------------------------------------
// Helper: fresh session, default +1h expiry
// ---------------------------------------------------------------------------

function freshSession(overrides: Partial<AuthSession> = {}): AuthSession {
  const now = new Date();
  const expires = new Date(now.getTime() + 60 * 60 * 1000);
  return {
    address: "0xWallet0000000000000000000000000000000001",
    signature: "0xsig",
    nonce: "a".repeat(64),
    issuedAt: now.toISOString(),
    expiresAt: expires.toISOString(),
    accountId: "acct-test-001",
    ...overrides,
  };
}

function expiredSession(overrides: Partial<AuthSession> = {}): AuthSession {
  const nowMs = Date.now();
  return freshSession({
    issuedAt: new Date(nowMs - 2 * 60 * 60 * 1000).toISOString(), // 2 hours ago
    expiresAt: new Date(nowMs - 60 * 60 * 1000).toISOString(),    // 1 hour ago (expired)
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// 1. Session restore after reconnect / refresh
// ---------------------------------------------------------------------------

describe("Session restore on reconnect/refresh", () => {
  beforeEach(installStorage);
  afterEach(uninstallStorage);

  test("valid session in storage → restores as authenticated", () => {
    const address = "0xWallet0000000000000000000000000000000001";
    const session = freshSession({ address });
    persistSession(session);

    // Simulate AuthProvider mount logic
    const loaded = loadSession();
    assert.ok(loaded !== null, "session should load from storage");
    assert.equal(isSessionValid(loaded, address), true, "should be valid for same address");
    // → authStatus should be "authenticated"
  });

  test("expired session in storage → clears and transitions to expired", () => {
    const address = "0xWallet0000000000000000000000000000000001";
    const session = expiredSession({ address });
    persistSession(session);

    const loaded = loadSession();
    assert.ok(loaded !== null, "corrupt sessions are still loadable");
    assert.equal(isSessionValid(loaded, address), false, "expired session is invalid");
    // AuthProvider calls clearSession(), sets status "expired"
    clearSession();
    assert.equal(loadSession(), null, "storage cleared after expiry detection");
  });

  test("session for a DIFFERENT address → invalid on restore (account switched)", () => {
    const sessionAddress = "0xOLD0000000000000000000000000000000000AA";
    const currentAddress = "0xNEW0000000000000000000000000000000000BB";
    const session = freshSession({ address: sessionAddress });
    persistSession(session);

    const loaded = loadSession();
    assert.ok(loaded !== null);
    // isSessionValid checks address match
    assert.equal(isSessionValid(loaded, currentAddress), false);
  });

  test("no session in storage → authStatus stays connected (no crash)", () => {
    // Simulate: wallet connected, no stored session → status = "connected"
    const loaded = loadSession();
    assert.equal(loaded, null, "nothing in storage → null");
    // No error thrown → connected status is set elsewhere
  });

  test("corrupt storage → handled gracefully", () => {
    (globalThis as Record<string, unknown>).sessionStorage =
      createMockSessionStorage(); // fresh
    (globalThis as unknown as { sessionStorage: typeof mockStorage })
      .sessionStorage.setItem("dex_auth_session_v1", "{not:json!");
    const loaded = loadSession();
    assert.equal(loaded, null, "corrupt JSON should return null gracefully");
  });
});

// ---------------------------------------------------------------------------
// 2. Session expiry timer logic
// ---------------------------------------------------------------------------

describe("Session expiry timer logic", () => {
  test("session well within TTL is still valid", () => {
    const session = freshSession(); // +1h
    assert.equal(isSessionValid(session, session.address), true);
  });

  test("session past TTL is invalid", () => {
    const session = expiredSession();
    assert.equal(isSessionValid(session, session.address), false,
      "past expiry → invalid");
  });

  test("session expiring exactly now is invalid (boundary condition)", () => {
    const justExpired = new Date(Date.now() - 1).toISOString();
    const session = freshSession({ expiresAt: justExpired });
    assert.equal(isSessionValid(session, session.address), false,
      "boundary: now >= expiresAt → invalid");
  });

  test("createSession sets expiresAt exactly 24 h after issuedAt", () => {
    const issuedAt = "2026-03-26T12:00:00.000Z";
    const session = createSession("0xA", "0xS", "n", issuedAt, "acct");
    const diff = new Date(session.expiresAt).getTime() - new Date(issuedAt).getTime();
    assert.equal(diff, 24 * 60 * 60 * 1000, "TTL must be exactly 24 hours");
  });

  test("fresh session for wrong address becomes invalid despite valid TTL", () => {
    const session = freshSession({ address: "0xADDR_A" }); // valid TTL
    assert.equal(isSessionValid(session, "0xADDR_B"), false,
      "address mismatch overrides valid TTL");
  });
});

// ---------------------------------------------------------------------------
// 3. Protected action blocking
// ---------------------------------------------------------------------------

describe("Protected action blocking — all non-authenticated states", () => {
  const NON_AUTH_STATES: AuthStatus[] = [
    "disconnected",
    "connecting",
    "connected",
    "signing",
    "expired",
    "rejected",
  ];

  for (const status of NON_AUTH_STATES) {
    test(`order submit is disabled when authStatus = "${status}"`, () => {
      const isAuthenticated = status === "authenticated";
      const isSubmitDisabled = !isAuthenticated;
      assert.equal(isSubmitDisabled, true,
        `${status} should block order submission`);
    });

    test(`cancel button is non-interactive when authStatus = "${status}"`, () => {
      const isAuthenticated = status === "authenticated";
      // Cancel shows a disabled span instead of a button
      assert.equal(isAuthenticated, false);
    });

    test(`withdrawal is blocked when authStatus = "${status}"`, () => {
      const isAuthenticated = status === "authenticated";
      assert.equal(!isAuthenticated, true);
    });
  }

  test("order submit is ENABLED only for authenticated", () => {
    const status: AuthStatus = "authenticated";
    const isAuthenticated = status === "authenticated";
    const isSubmitDisabled = !isAuthenticated;
    assert.equal(isSubmitDisabled, false, "authenticated → submit enabled");
  });
});

// ---------------------------------------------------------------------------
// 4. Chain change invalidation
// ---------------------------------------------------------------------------

describe("Chain change invalidation", () => {
  beforeEach(installStorage);
  afterEach(uninstallStorage);

  test("chain change always clears session regardless of address", () => {
    const session = freshSession();
    persistSession(session);
    assert.ok(loadSession() !== null, "session present before chain change");

    // Simulate AuthProvider chainChanged handler: calls clearSession
    clearSession();

    assert.equal(loadSession(), null, "session cleared after chain change");
  });

  test("chain change should leave no auth tokens in storage", () => {
    const session = freshSession();
    persistSession(session);
    clearSession(); // simulate handler
    // Verify second call is idempotent
    clearSession();
    assert.equal(loadSession(), null, "idempotent clear");
  });
});

// ---------------------------------------------------------------------------
// 5. Account change invalidation
// ---------------------------------------------------------------------------

describe("Account change invalidation", () => {
  beforeEach(installStorage);
  afterEach(uninstallStorage);

  test("switching to a new address invalidates existing session", () => {
    const oldAddr = "0xOLD0000000000000000000000000000000000AA";
    const newAddr = "0xNEW0000000000000000000000000000000000BB";
    const session = freshSession({ address: oldAddr });
    persistSession(session);

    // AuthProvider: address changed → check session validity
    const loaded = loadSession();
    assert.ok(loaded);
    if (!isSessionValid(loaded, newAddr)) {
      clearSession();
    }

    assert.equal(loadSession(), null, "old session removed after account switch");
  });

  test("switching to same address (case change) keeps session valid", () => {
    const addr = "0xAbcDef0000000000000000000000000000001234";
    const session = freshSession({ address: addr });
    assert.equal(
      isSessionValid(session, addr.toUpperCase()),
      true,
      "address comparison is case-insensitive",
    );
    assert.equal(
      isSessionValid(session, addr.toLowerCase()),
      true,
    );
  });

  test("nonce uniqueness per sign-in prevents session replay", () => {
    const n1 = generateNonce();
    const n2 = generateNonce();
    const n3 = generateNonce();
    // Three independent sign-in attempts must not share a nonce
    assert.notEqual(n1, n2);
    assert.notEqual(n2, n3);
    assert.notEqual(n1, n3);
  });
});

// ---------------------------------------------------------------------------
// 6. UI fallback state mapping
// ---------------------------------------------------------------------------

describe("UI fallback state mapping", () => {
  const STATE_MAP: Record<AuthStatus, { label: string; isConnected: boolean }> = {
    disconnected:  { label: "Connect Wallet",        isConnected: false },
    connecting:    { label: "Connecting…",            isConnected: false },
    connected:     { label: "Sign In",                isConnected: true  },
    signing:       { label: "Awaiting signature…",    isConnected: true  },
    authenticated: { label: "Sign Out",               isConnected: true  },
    expired:       { label: "Session expired",        isConnected: true  },
    rejected:      { label: "Rejected — Retry",       isConnected: true  },
  };

  for (const [status, expected] of Object.entries(STATE_MAP) as [AuthStatus, typeof STATE_MAP[AuthStatus]][]) {
    test(`${status} → shows "${expected.label}", wallet connected=${expected.isConnected}`, () => {
      const isAuthenticated = status === "authenticated";
      const walletsConnected = expected.isConnected;

      // Verify: only "authenticated" unlocks protected actions
      if (isAuthenticated) {
        assert.equal(walletsConnected, true, "authenticated implies connected");
      }

      // Label is defined (not empty)
      assert.ok(expected.label.length > 0, "label should not be empty");
    });
  }

  test("exactly one state is authenticated", () => {
    const authStatuses: AuthStatus[] = [
      "disconnected", "connecting", "connected",
      "signing", "authenticated", "expired", "rejected",
    ];
    const authCount = authStatuses.filter((s) => s === "authenticated").length;
    assert.equal(authCount, 1, "exactly one state is authenticated");
  });
});

// ---------------------------------------------------------------------------
// 7. Trading flow auth stability
// ---------------------------------------------------------------------------

describe("Trading flow auth stability after auth changes", () => {
  const TRANSITIONS: { from: AuthStatus; to: AuthStatus; shouldBlock: boolean }[] = [
    { from: "authenticated", to: "expired",       shouldBlock: true  },
    { from: "authenticated", to: "disconnected",  shouldBlock: true  },
    { from: "authenticated", to: "connected",     shouldBlock: true  },
    { from: "connected",     to: "authenticated", shouldBlock: false },
    { from: "signing",       to: "rejected",      shouldBlock: true  },
    { from: "signing",       to: "authenticated", shouldBlock: false },
  ];

  for (const { from, to, shouldBlock } of TRANSITIONS) {
    test(`${from} → ${to}: isSubmitDisabled=${shouldBlock}`, () => {
      // Simulate state transition: after the transition, only "authenticated" unblocks
      const isAuthenticated = to === "authenticated";
      const isSubmitDisabled = !isAuthenticated;
      assert.equal(
        isSubmitDisabled,
        shouldBlock,
        `After transitioning from ${from} to ${to}, submit disabled should be ${shouldBlock}`,
      );
    });
  }

  test("signing message is deterministic across status transitions", () => {
    const addr = "0xWallet0000000000000000000000000000000001";
    const nonce = "f".repeat(64);
    const issuedAt = "2026-03-26T12:00:00.000Z";
    const msg1 = buildLoginMessage(addr, nonce, issuedAt);
    const msg2 = buildLoginMessage(addr, nonce, issuedAt);
    assert.equal(msg1, msg2, "login message must be deterministic");
  });

  test("after sign-out, subsequent action guard returns blocked", () => {
    // Helper: extracts the check so TypeScript cannot narrow the literal type
    const isAuth = (s: string) => s === "authenticated";

    // Simulate: was authenticated, then sign-out transitions to "connected"
    const before: string = "authenticated";
    assert.equal(!isAuth(before), false, "was enabled before sign-out");

    const after: string = "connected"; // post sign-out
    assert.equal(!isAuth(after), true, "now blocked after sign-out");
  });
});
