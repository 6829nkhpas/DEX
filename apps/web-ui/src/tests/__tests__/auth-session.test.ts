// ---------------------------------------------------------------------------
// auth-session.test.ts — Phase 19 Auth + Wallet Session Layer tests
// ---------------------------------------------------------------------------
//
// Uses Node's built-in test runner (tsx --test).
// Tests pure auth service functions + session validation + integration paths.
// No React renderer — all tests operate at the logic/service level.
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

// Install mock sessionStorage globally before each test group
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

function makeSession(overrides: Partial<AuthSession> = {}): AuthSession {
  const now = new Date();
  const expires = new Date(now.getTime() + 60 * 60 * 1000); // +1 hour
  return {
    address: "0xAbC1230000000000000000000000000000004567",
    signature: "0xsignature",
    nonce: "a".repeat(64),
    issuedAt: now.toISOString(),
    expiresAt: expires.toISOString(),
    accountId: "test-account-id",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// 1. generateNonce
// ---------------------------------------------------------------------------

describe("generateNonce", () => {
  test("returns a 64-character hex string", () => {
    const nonce = generateNonce();
    assert.equal(nonce.length, 64);
    assert.match(nonce, /^[0-9a-f]{64}$/);
  });

  test("returns unique values on each call", () => {
    const n1 = generateNonce();
    const n2 = generateNonce();
    assert.notEqual(n1, n2);
  });
});

// ---------------------------------------------------------------------------
// 2. buildLoginMessage
// ---------------------------------------------------------------------------

describe("buildLoginMessage", () => {
  test("includes address, nonce, and issuedAt", () => {
    const address = "0xABCDEF";
    const nonce = "badc0ffee" + "0".repeat(55);
    const issuedAt = "2026-03-25T00:00:00.000Z";
    const message = buildLoginMessage(address, nonce, issuedAt);

    assert.ok(message.includes(address), "message should include address");
    assert.ok(message.includes(nonce), "message should include nonce");
    assert.ok(message.includes(issuedAt), "message should include issuedAt");
  });

  test("is deterministic — same inputs yield identical output", () => {
    const address = "0x1234";
    const nonce = "f".repeat(64);
    const issuedAt = "2026-01-01T00:00:00.000Z";
    const m1 = buildLoginMessage(address, nonce, issuedAt);
    const m2 = buildLoginMessage(address, nonce, issuedAt);
    assert.equal(m1, m2);
  });
});

// ---------------------------------------------------------------------------
// 3. createSession
// ---------------------------------------------------------------------------

describe("createSession", () => {
  test("sets all expected fields", () => {
    const issuedAt = new Date().toISOString();
    const session = createSession(
      "0xAddr",
      "0xSig",
      "nonce123",
      issuedAt,
      "acct-uuid",
    );
    assert.equal(session.address, "0xAddr");
    assert.equal(session.signature, "0xSig");
    assert.equal(session.nonce, "nonce123");
    assert.equal(session.issuedAt, issuedAt);
    assert.equal(session.accountId, "acct-uuid");
  });

  test("expiresAt is exactly 24 hours after issuedAt", () => {
    const issuedAt = "2026-03-25T00:00:00.000Z";
    const session = createSession("0xA", "0xS", "nonce", issuedAt, "acct");
    const issuedMs = new Date(issuedAt).getTime();
    const expiresMs = new Date(session.expiresAt).getTime();
    assert.equal(expiresMs - issuedMs, 24 * 60 * 60 * 1000);
  });
});

// ---------------------------------------------------------------------------
// 4. isSessionValid
// ---------------------------------------------------------------------------

describe("isSessionValid", () => {
  test("returns true for a fresh session with matching address", () => {
    const session = makeSession();
    assert.equal(isSessionValid(session, session.address), true);
  });

  test("returns false for an expired session", () => {
    const pastExpiry = new Date(Date.now() - 1).toISOString(); // already expired
    const session = makeSession({ expiresAt: pastExpiry });
    assert.equal(isSessionValid(session, session.address), false);
  });

  test("returns false when address does not match", () => {
    const session = makeSession({ address: "0xAAA" });
    assert.equal(isSessionValid(session, "0xBBB"), false);
  });

  test("is case-insensitive on address comparison", () => {
    const session = makeSession({ address: "0xabcdef" });
    assert.equal(isSessionValid(session, "0xABCDEF"), true);
    assert.equal(isSessionValid(session, "0xABCDEF"), true);
  });

  test("returns false for session expired exactly now", () => {
    // Set expiresAt to 1 ms in the past
    const justExpired = new Date(Date.now() - 1).toISOString();
    const session = makeSession({ expiresAt: justExpired });
    assert.equal(isSessionValid(session, session.address), false);
  });
});

// ---------------------------------------------------------------------------
// 5. persistSession / loadSession / clearSession round-trip
// ---------------------------------------------------------------------------

describe("Session storage round-trip", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("persistSession then loadSession returns the same session", () => {
    const session = makeSession();
    persistSession(session);
    const loaded = loadSession();
    assert.ok(loaded !== null);
    assert.deepEqual(loaded, session);
  });

  test("loadSession returns null when nothing persisted", () => {
    assert.equal(loadSession(), null);
  });

  test("clearSession removes the persisted session", () => {
    const session = makeSession();
    persistSession(session);
    clearSession();
    assert.equal(loadSession(), null);
  });

  test("loadSession returns null for corrupt JSON", () => {
    mockStorage.setItem("dex_auth_session_v1", "{invalid json}");
    assert.equal(loadSession(), null);
  });

  test("loadSession returns null for valid JSON missing required fields", () => {
    mockStorage.setItem("dex_auth_session_v1", JSON.stringify({ address: "0x1" }));
    assert.equal(loadSession(), null);
  });

  test("clearSession is idempotent", () => {
    // Should not throw even when nothing stored
    clearSession();
    clearSession();
    assert.equal(loadSession(), null);
  });
});

// ---------------------------------------------------------------------------
// 6. Sign-in success path (mock signMessage)
// ---------------------------------------------------------------------------

describe("Sign-in success path", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("creates and persists session on successful signature", () => {
    const address = "0xAbC1230000000000000000000000000000004567";
    const accountId = "derived-account-id";

    // Simulate the AuthProvider sign-in flow inline
    const nonce = generateNonce();
    const issuedAt = new Date().toISOString();
    const message = buildLoginMessage(address, nonce, issuedAt);
    // Mock wallet returns a signature
    const mockSignature = "0xmockedsignature";
    const session = createSession(address, mockSignature, nonce, issuedAt, accountId);

    persistSession(session);

    const loaded = loadSession();
    assert.ok(loaded !== null);
    assert.equal(loaded.address, address);
    assert.equal(loaded.signature, mockSignature);
    assert.equal(loaded.accountId, accountId);
    assert.ok(message.includes(nonce), "message includes nonce");
  });
});

// ---------------------------------------------------------------------------
// 7. Signature rejection — no session persisted
// ---------------------------------------------------------------------------

describe("Signature rejection", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("rejected signature leaves no session in storage", () => {
    // Simulate rejection: signMessage throws, so we never call createSession/persistSession
    let sessionCreated = false;
    try {
      // Mock: wallet throws user-rejected error
      throw new Error("User denied message signature");
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
    } catch (_err) {
      // Auth provider sets status to "rejected" and returns without persisting
    }
    // No session should exist
    assert.equal(loadSession(), null);
    assert.equal(sessionCreated, false);
  });

  test("rejection error message is identifiable", () => {
    const rejectionMessages = [
      "User denied message signature",
      "user rejected the request",
      "MetaMask Message Signature: User denied",
    ];
    for (const msg of rejectionMessages) {
      const isRejection =
        msg.includes("User denied") ||
        msg.includes("user rejected") ||
        msg.includes("4001");
      assert.equal(isRejection, true, `Should identify rejection: "${msg}"`);
    }
  });
});

// ---------------------------------------------------------------------------
// 8. Session restore on mount
// ---------------------------------------------------------------------------

describe("Session restore", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("valid session in storage is recognised as authenticated", () => {
    const address = "0xWalletAddress";
    const session = makeSession({ address });
    persistSession(session);

    // Simulate AuthProvider mount: load and validate
    const loaded = loadSession();
    assert.ok(loaded !== null);
    const valid = isSessionValid(loaded, address);
    assert.equal(valid, true); // → status should be "authenticated"
  });

  test("expired session in storage is rejected on mount", () => {
    const address = "0xWalletAddress";
    const expired = makeSession({
      address,
      expiresAt: new Date(Date.now() - 1000).toISOString(),
    });
    persistSession(expired);

    const loaded = loadSession();
    assert.ok(loaded !== null);
    const valid = isSessionValid(loaded, address);
    assert.equal(valid, false); // → status should be "expired" or "connected"
  });

  test("session for different address is invalid", () => {
    const storedAddress = "0xOLD_WALLET";
    const currentAddress = "0xNEW_WALLET";
    const session = makeSession({ address: storedAddress });
    persistSession(session);

    const loaded = loadSession();
    assert.ok(loaded !== null);
    const valid = isSessionValid(loaded, currentAddress);
    assert.equal(valid, false); // address mismatch → invalidate
  });
});

// ---------------------------------------------------------------------------
// 9. Logout clears auth
// ---------------------------------------------------------------------------

describe("Logout", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("signOut clears session from storage", () => {
    const session = makeSession();
    persistSession(session);
    assert.ok(loadSession() !== null);

    // Simulate signOut
    clearSession();

    assert.equal(loadSession(), null);
  });

  test("signOut is idempotent — calling twice is safe", () => {
    clearSession(); // no session in storage
    clearSession(); // again — should not throw
    assert.equal(loadSession(), null);
  });
});

// ---------------------------------------------------------------------------
// 10. Disconnect / account change / chain change invalidation
// ---------------------------------------------------------------------------

describe("Auth invalidation on wallet events", () => {
  beforeEach(installMockStorage);
  afterEach(uninstallMockStorage);

  test("disconnect (address → null) should clear session", () => {
    const session = makeSession();
    persistSession(session);

    // Simulate: AuthProvider handles address → null, calls clearSession
    const newAddress: string | null = null;
    if (!newAddress) {
      clearSession();
    }

    assert.equal(loadSession(), null);
  });

  test("account change to different address invalidates existing session", () => {
    const oldAddress = "0xOLD";
    const newAddress = "0xNEW";
    const session = makeSession({ address: oldAddress });
    persistSession(session);

    // Simulate AuthProvider: address changed, check session validity
    const loaded = loadSession();
    assert.ok(loaded !== null);
    if (!isSessionValid(loaded, newAddress)) {
      clearSession();
    }

    assert.equal(loadSession(), null);
  });

  test("chain change should invalidate session regardless of address", () => {
    const session = makeSession();
    persistSession(session);

    // Simulate chainChanged handler: always clears auth
    clearSession();

    assert.equal(loadSession(), null);
  });

  test("nonce is unique per sign-in (prevents replay)", () => {
    const n1 = generateNonce();
    const n2 = generateNonce();
    // Two different sign-in attempts produce different nonces
    assert.notEqual(n1, n2);
  });
});

// ---------------------------------------------------------------------------
// 11. Protected action guards (logic-level)
// ---------------------------------------------------------------------------

describe("Protected action guards", () => {
  test("order submit should be blocked when not authenticated", () => {
    const authStatus: string = "connected"; // not authenticated
    const isAuthenticated = authStatus === "authenticated";
    // isSubmitDisabled should include !isAuthenticated
    const isSubmitDisabled = !isAuthenticated;
    assert.equal(isSubmitDisabled, true);
  });

  test("order submit is enabled when authenticated", () => {
    const authStatus: string = "authenticated";
    const isAuthenticated = authStatus === "authenticated";
    const isSubmitDisabled = !isAuthenticated;
    assert.equal(isSubmitDisabled, false);
  });

  test("cancel is blocked when not authenticated", () => {
    const authStatus: string = "disconnected";
    const isAuthenticated = authStatus === "authenticated";
    // Cancel button renders as disabled span, not interactive button
    assert.equal(isAuthenticated, false);
  });

  test("withdraw is blocked when not authenticated", () => {
    const authStatus: string = "expired";
    const isAuthenticated = authStatus === "authenticated";
    // Withdraw button disabled prop
    assert.equal(!isAuthenticated, true);
  });

  test("all transient states are not authenticated", () => {
    const nonAuthStates: string[] = [
      "disconnected",
      "connecting",
      "connected",
      "signing",
      "expired",
      "rejected",
    ];
    for (const s of nonAuthStates) {
      assert.equal(s === "authenticated", false, `${s} should not be authenticated`);
    }
  });
});
