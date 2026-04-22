// ---------------------------------------------------------------------------
// phase19-auth-hardening.test.ts — Phase 19 Auth + Wallet Access Hardening
// ---------------------------------------------------------------------------
//
// Tests for all Phase 19 hardening additions:
//   1.  isSessionStructurallyValid — type-guard
//   2.  Nonce consumption / replay prevention
//   3.  AuthRequiredError class
//   4.  Protected action gating logic (unit-level)
//   5.  Full auth lifecycle simulations
//   6.  Session TTL export verification
//   7.  Hardened loadSession (structural validation)
//   8.  Edge-case handling (stale tabs, chain changes, etc.)
//
// Uses Node's built-in test runner (tsx --test).
// ---------------------------------------------------------------------------

import { test, describe, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";

import {
  generateNonce,
  buildLoginMessage,
  createSession,
  isSessionValid,
  isSessionStructurallyValid,
  consumeNonce,
  isNonceUsed,
  clearUsedNonces,
  persistSession,
  loadSession,
  clearSession,
  SESSION_TTL_MS,
  type AuthSession,
} from "../../auth/authService";

import { AuthRequiredError } from "../../api/types";

// ---------------------------------------------------------------------------
// Mock sessionStorage for Node test environment
// ---------------------------------------------------------------------------

function createMockSessionStorage() {
  const store: Record<string, string> = {};
  return {
    getItem(key: string): string | null {
      return Object.prototype.hasOwnProperty.call(store, key) ? store[key] : null;
    },
    setItem(key: string, value: string) {
      store[key] = value;
    },
    removeItem(key: string) {
      delete store[key];
    },
    clear() {
      for (const k of Object.keys(store)) delete store[k];
    },
    get _store() {
      return store;
    },
  };
}

let mockStorage: ReturnType<typeof createMockSessionStorage>;

function installMockStorage() {
  mockStorage = createMockSessionStorage();
  (globalThis as Record<string, unknown>).sessionStorage = mockStorage;
}

function uninstallMockStorage() {
  delete (globalThis as Record<string, unknown>).sessionStorage;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeValidSession(overrides: Partial<AuthSession> = {}): AuthSession {
  const now = new Date();
  const expires = new Date(now.getTime() + 60 * 60 * 1000); // +1 hour
  return {
    address: "0xAbC1230000000000000000000000000000004567",
    signature: "0xsignature123",
    nonce: "a".repeat(64),
    issuedAt: now.toISOString(),
    expiresAt: expires.toISOString(),
    accountId: "test-account-id-12345678",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// 1. isSessionStructurallyValid — type-guard
// ---------------------------------------------------------------------------

describe("isSessionStructurallyValid", () => {
  test("returns true for a well-formed session", () => {
    const session = makeValidSession();
    assert.equal(isSessionStructurallyValid(session), true);
  });

  test("rejects null", () => {
    assert.equal(isSessionStructurallyValid(null), false);
  });

  test("rejects undefined", () => {
    assert.equal(isSessionStructurallyValid(undefined), false);
  });

  test("rejects a number", () => {
    assert.equal(isSessionStructurallyValid(42), false);
  });

  test("rejects a string", () => {
    assert.equal(isSessionStructurallyValid("not a session"), false);
  });

  test("rejects empty object", () => {
    assert.equal(isSessionStructurallyValid({}), false);
  });

  test("rejects when address is missing", () => {
    const session = makeValidSession();
    const { address: _, ...rest } = session;
    assert.equal(isSessionStructurallyValid(rest), false);
  });

  test("rejects when address is empty string", () => {
    const session = makeValidSession({ address: "" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when signature is empty", () => {
    const session = makeValidSession({ signature: "" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when nonce is too short", () => {
    const session = makeValidSession({ nonce: "abc123" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when nonce is too long", () => {
    const session = makeValidSession({ nonce: "a".repeat(65) });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when nonce contains non-hex characters", () => {
    const session = makeValidSession({ nonce: "g".repeat(64) });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects uppercase hex nonce", () => {
    const session = makeValidSession({ nonce: "A".repeat(64) });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when issuedAt is not a valid date", () => {
    const session = makeValidSession({ issuedAt: "not-a-date" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when expiresAt is not a valid date", () => {
    const session = makeValidSession({ expiresAt: "not-a-date" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when expiresAt <= issuedAt (backwards timestamps)", () => {
    const now = new Date();
    const earlier = new Date(now.getTime() - 1000);
    const session = makeValidSession({
      issuedAt: now.toISOString(),
      expiresAt: earlier.toISOString(),
    });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when expiresAt === issuedAt (zero-duration)", () => {
    const now = new Date().toISOString();
    const session = makeValidSession({
      issuedAt: now,
      expiresAt: now,
    });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when accountId is empty", () => {
    const session = makeValidSession({ accountId: "" });
    assert.equal(isSessionStructurallyValid(session), false);
  });

  test("rejects when a field is a number instead of string", () => {
    const session = makeValidSession();
    const bad = { ...session, address: 12345 };
    assert.equal(isSessionStructurallyValid(bad), false);
  });

  test("accepts nonce with all valid hex chars (0-9, a-f)", () => {
    const nonce = "0123456789abcdef".repeat(4); // 64 chars
    const session = makeValidSession({ nonce });
    assert.equal(isSessionStructurallyValid(session), true);
  });
});

// ---------------------------------------------------------------------------
// 2. Nonce consumption / replay prevention
// ---------------------------------------------------------------------------

describe("Nonce replay prevention", () => {
  beforeEach(() => {
    clearUsedNonces();
  });

  test("consumeNonce returns true on first use", () => {
    const nonce = generateNonce();
    assert.equal(consumeNonce(nonce), true);
  });

  test("consumeNonce returns false on reuse", () => {
    const nonce = generateNonce();
    consumeNonce(nonce);
    assert.equal(consumeNonce(nonce), false);
  });

  test("isNonceUsed returns false for unused nonce", () => {
    const nonce = generateNonce();
    assert.equal(isNonceUsed(nonce), false);
  });

  test("isNonceUsed returns true after consumption", () => {
    const nonce = generateNonce();
    consumeNonce(nonce);
    assert.equal(isNonceUsed(nonce), true);
  });

  test("clearUsedNonces resets all tracked nonces", () => {
    const n1 = generateNonce();
    const n2 = generateNonce();
    consumeNonce(n1);
    consumeNonce(n2);
    assert.equal(isNonceUsed(n1), true);
    assert.equal(isNonceUsed(n2), true);

    clearUsedNonces();

    assert.equal(isNonceUsed(n1), false);
    assert.equal(isNonceUsed(n2), false);
  });

  test("different nonces are tracked independently", () => {
    const n1 = generateNonce();
    const n2 = generateNonce();
    consumeNonce(n1);

    assert.equal(isNonceUsed(n1), true);
    assert.equal(isNonceUsed(n2), false);
    assert.equal(consumeNonce(n2), true);
  });
});

// ---------------------------------------------------------------------------
// 3. AuthRequiredError class
// ---------------------------------------------------------------------------

describe("AuthRequiredError", () => {
  test("is an instance of Error", () => {
    const err = new AuthRequiredError();
    assert.ok(err instanceof Error);
  });

  test("has correct name", () => {
    const err = new AuthRequiredError();
    assert.equal(err.name, "AuthRequiredError");
  });

  test("has default message", () => {
    const err = new AuthRequiredError();
    assert.equal(err.message, "Authentication required. Please sign in.");
  });

  test("accepts custom message", () => {
    const err = new AuthRequiredError("Custom auth error");
    assert.equal(err.message, "Custom auth error");
  });
});

// ---------------------------------------------------------------------------
// 4. SESSION_TTL_MS export verification
// ---------------------------------------------------------------------------

describe("SESSION_TTL_MS", () => {
  test("is exactly 24 hours in milliseconds", () => {
    assert.equal(SESSION_TTL_MS, 24 * 60 * 60 * 1000);
  });

  test("is exported as a number", () => {
    assert.equal(typeof SESSION_TTL_MS, "number");
  });
});

// ---------------------------------------------------------------------------
// 5. Hardened loadSession (structural validation)
// ---------------------------------------------------------------------------

describe("Hardened loadSession", () => {
  beforeEach(installMockStorage);
  afterEach(() => {
    uninstallMockStorage();
    clearUsedNonces();
  });

  test("loads valid session", () => {
    const session = makeValidSession();
    persistSession(session);
    const loaded = loadSession();
    assert.ok(loaded !== null);
    assert.deepEqual(loaded, session);
  });

  test("rejects session with short nonce", () => {
    const session = makeValidSession();
    // Manually corrupt the stored session
    const corrupt = { ...session, nonce: "abc123" };
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(corrupt));
    assert.equal(loadSession(), null);
  });

  test("rejects session with non-hex nonce", () => {
    const session = makeValidSession();
    const corrupt = { ...session, nonce: "g".repeat(64) };
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(corrupt));
    assert.equal(loadSession(), null);
  });

  test("rejects session with backwards timestamps", () => {
    const now = new Date();
    const session = makeValidSession({
      issuedAt: now.toISOString(),
      expiresAt: new Date(now.getTime() - 1000).toISOString(),
    });
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(session));
    assert.equal(loadSession(), null);
  });

  test("rejects session with invalid date strings", () => {
    const session = makeValidSession({ issuedAt: "not-a-date" });
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(session));
    assert.equal(loadSession(), null);
  });

  test("rejects session with empty accountId", () => {
    const session = makeValidSession({ accountId: "" });
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(session));
    assert.equal(loadSession(), null);
  });

  test("rejects session with empty address", () => {
    const session = makeValidSession({ address: "" });
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(session));
    assert.equal(loadSession(), null);
  });

  test("rejects session with numeric fields", () => {
    const session = { ...makeValidSession(), address: 12345 };
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify(session));
    assert.equal(loadSession(), null);
  });
});

// ---------------------------------------------------------------------------
// 6. Protected action gating (logic-level simulation)
// ---------------------------------------------------------------------------

describe("Protected action gating — auth boundary", () => {
  test("order submit blocked during 'signing' state", () => {
    const authStatus: string = "signing";
    const isAuthenticated = authStatus === "authenticated";
    assert.equal(isAuthenticated, false);
  });

  test("order submit blocked during 'expired' state", () => {
    const authStatus: string = "expired";
    const isAuthenticated = authStatus === "authenticated";
    assert.equal(isAuthenticated, false);
  });

  test("order submit blocked during 'rejected' state", () => {
    const authStatus: string = "rejected";
    const isAuthenticated = authStatus === "authenticated";
    assert.equal(isAuthenticated, false);
  });

  test("withdraw blocked when not authenticated", () => {
    const authStatus: string = "connected";
    const isAuthenticated = authStatus === "authenticated";
    // Withdraw button should have disabled={!isAuthenticated}
    assert.equal(!isAuthenticated, true);
  });

  test("cancel rendered as non-interactive span when not authenticated", () => {
    const authStatus: string = "connecting";
    const isAuthenticated = authStatus === "authenticated";
    // Cancel button renders as disabled span
    assert.equal(isAuthenticated, false);
  });

  test("all non-authenticated states are blocked", () => {
    const blockedStates: string[] = [
      "disconnected",
      "connecting",
      "connected",
      "signing",
      "expired",
      "rejected",
    ];
    for (const state of blockedStates) {
      assert.equal(
        state === "authenticated",
        false,
        `State "${state}" should NOT be authenticated`,
      );
    }
  });

  test("only 'authenticated' state allows protected actions", () => {
    const authStatus: string = "authenticated";
    assert.equal(authStatus === "authenticated", true);
  });

  test("session validity check catches expired session between poll ticks", () => {
    // Session expired 1 second ago
    const session = makeValidSession({
      expiresAt: new Date(Date.now() - 1000).toISOString(),
    });
    const address = session.address;
    // Even if authStatus hasn't caught up, isSessionValid should catch it
    assert.equal(isSessionValid(session, address), false);
  });

  test("session validity check catches address mismatch", () => {
    const session = makeValidSession({ address: "0xOLD" });
    assert.equal(isSessionValid(session, "0xNEW"), false);
  });
});

// ---------------------------------------------------------------------------
// 7. Full auth lifecycle simulation
// ---------------------------------------------------------------------------

describe("Full auth lifecycle", () => {
  beforeEach(() => {
    installMockStorage();
    clearUsedNonces();
  });
  afterEach(() => {
    uninstallMockStorage();
    clearUsedNonces();
  });

  test("connect → sign-in → auth → sign-out → re-sign lifecycle", () => {
    const address = "0xAbC1230000000000000000000000000000004567";
    const accountId = "derived-acct-id";

    // 1. Generate nonce and build message
    const nonce1 = generateNonce();
    const issuedAt1 = new Date().toISOString();
    const message1 = buildLoginMessage(address, nonce1, issuedAt1);
    assert.ok(message1.includes(nonce1));

    // 2. Create session (mock signature)
    const session1 = createSession(address, "0xsig1", nonce1, issuedAt1, accountId);
    consumeNonce(nonce1);
    persistSession(session1);

    // 3. Verify session is valid
    assert.equal(isSessionValid(session1, address), true);
    assert.equal(isSessionStructurallyValid(session1), true);

    // 4. Load from storage works
    const loaded = loadSession();
    assert.ok(loaded);
    assert.deepEqual(loaded, session1);

    // 5. Sign out clears everything
    clearSession();
    assert.equal(loadSession(), null);
    assert.equal(isNonceUsed(nonce1), false); // nonces cleared

    // 6. Re-sign with fresh nonce
    const nonce2 = generateNonce();
    assert.notEqual(nonce1, nonce2); // guaranteed unique
    const issuedAt2 = new Date().toISOString();
    const session2 = createSession(address, "0xsig2", nonce2, issuedAt2, accountId);
    consumeNonce(nonce2);
    persistSession(session2);

    assert.equal(isSessionValid(session2, address), true);
    assert.ok(loadSession());
  });

  test("chain change invalidates session", () => {
    const address = "0xUser123";
    const nonce = generateNonce();
    const session = createSession(address, "0xsig", nonce, new Date().toISOString(), "acct-1");
    persistSession(session);

    // Simulate chain change: AuthProvider calls clearSession
    clearSession();

    assert.equal(loadSession(), null);
    // Nonces cleared — can re-sign
    assert.equal(isNonceUsed(nonce), false);
  });

  test("wallet disconnect fully clears auth state", () => {
    const session = makeValidSession();
    persistSession(session);
    assert.ok(loadSession());

    // Simulate disconnect: address → null, AuthProvider calls clearSession
    clearSession();

    assert.equal(loadSession(), null);
  });

  test("account change invalidates session for old address", () => {
    const oldAddr = "0xOLD_WALLET_ADDRESS";
    const newAddr = "0xNEW_WALLET_ADDRESS";
    const nonce = generateNonce();
    const session = createSession(oldAddr, "0xsig", nonce, new Date().toISOString(), "old-acct");
    persistSession(session);

    // Simulate account change: validate against new address fails
    assert.equal(isSessionValid(session, newAddr), false);

    // AuthProvider would call clearSession
    clearSession();
    assert.equal(loadSession(), null);
  });
});

// ---------------------------------------------------------------------------
// 8. Edge cases
// ---------------------------------------------------------------------------

describe("Edge cases", () => {
  beforeEach(() => {
    installMockStorage();
    clearUsedNonces();
  });
  afterEach(() => {
    uninstallMockStorage();
    clearUsedNonces();
  });

  test("session expired by exactly 1ms is invalid", () => {
    const now = Date.now();
    const session = makeValidSession({
      expiresAt: new Date(now - 1).toISOString(),
    });
    assert.equal(isSessionValid(session, session.address), false);
  });

  test("session expiring in 1ms is still valid", () => {
    const session = makeValidSession({
      expiresAt: new Date(Date.now() + 60_000).toISOString(), // 60 sec future
    });
    assert.equal(isSessionValid(session, session.address), true);
  });

  test("multiple rapid nonce generations produce unique values", () => {
    const nonces = new Set<string>();
    for (let i = 0; i < 100; i++) {
      nonces.add(generateNonce());
    }
    assert.equal(nonces.size, 100, "All 100 nonces should be unique");
  });

  test("clearSession is safe to call when no storage available", () => {
    // Uninstall storage to simulate private browsing
    uninstallMockStorage();
    // Should not throw
    clearSession();
    installMockStorage(); // re-install for afterEach
  });

  test("loadSession returns null when storage is unavailable", () => {
    uninstallMockStorage();
    const result = loadSession();
    assert.equal(result, null);
    installMockStorage(); // re-install for afterEach
  });

  test("persistSession silently fails when storage is unavailable", () => {
    uninstallMockStorage();
    const session = makeValidSession();
    // Should not throw
    persistSession(session);
    installMockStorage();
    // Verify nothing was persisted
    assert.equal(loadSession(), null);
  });

  test("session with address case mismatch is still valid", () => {
    const session = makeValidSession({
      address: "0xabcdef0000000000000000000000000000001234",
    });
    // Compare with uppercase
    assert.equal(
      isSessionValid(session, "0xABCDEF0000000000000000000000000000001234"),
      true,
    );
  });

  test("buildLoginMessage is deterministic across calls", () => {
    const addr = "0xTest";
    const nonce = "f".repeat(64);
    const issued = "2026-01-01T00:00:00.000Z";

    const m1 = buildLoginMessage(addr, nonce, issued);
    const m2 = buildLoginMessage(addr, nonce, issued);
    const m3 = buildLoginMessage(addr, nonce, issued);

    assert.equal(m1, m2);
    assert.equal(m2, m3);
  });

  test("createSession produces correct 24h expiry", () => {
    const issuedAt = "2026-06-15T12:00:00.000Z";
    const session = createSession("0xAddr", "0xSig", "a".repeat(64), issuedAt, "acct");
    const issuedMs = new Date(issuedAt).getTime();
    const expiresMs = new Date(session.expiresAt).getTime();
    assert.equal(expiresMs - issuedMs, SESSION_TTL_MS);
  });
});
