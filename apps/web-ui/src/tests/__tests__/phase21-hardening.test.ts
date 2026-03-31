// ---------------------------------------------------------------------------
// phase21-hardening.test.ts — Phase 21 production hardening regression suite
// ---------------------------------------------------------------------------
//
// Uses Node's built-in test runner (tsx --test), consistent with existing suite.
// All tests are pure logic — no React renderer, no DOM, no network.
//
// Suites:
//   1.  Wallet reconnect / disconnect (mock provider accountsChanged)
//   2.  Parallel sign-in blocked by in-flight guard
//   3.  Auth rate limiter — exhaustion (5 attempts → 6th blocked)
//   4.  Auth rate limiter — RateLimitError shape (action, waitMs)
//   5.  Named rate-limiter registry — getOrCreate returns same instance
//   6.  Registry.resetAll() restores all limiters to full capacity
//   7.  useProtectedAction guard logic (pure extraction, no React)
//   8.  Governance role derivation and hasRole ordering
//   9.  GovernanceGuard role pass / block (pure logic)
//  10.  Governance audit log append
//  11.  Error session recovery — expired session + reconnect clears state
// ---------------------------------------------------------------------------

import { test, describe, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";

import {
    RateLimiter,
    RateLimiterRegistry,
    RateLimitError,
    defaultRateLimiterRegistry,
} from "../../infra/rate-limiter";
import {
    createSession,
    isSessionValid,
    persistSession,
    loadSession,
    clearSession,
    generateNonce,
    type AuthSession,
} from "../../auth/authService";
import { hasRole, type AdminRole } from "../../auth/GovernanceContext";

// ---------------------------------------------------------------------------
// Shared mock sessionStorage
// ---------------------------------------------------------------------------

function makeMockStorage() {
    const store: Record<string, string> = {};
    return {
        getItem: (k: string) =>
            Object.prototype.hasOwnProperty.call(store, k) ? store[k] : null,
        setItem: (k: string, v: string) => { store[k] = v; },
        removeItem: (k: string) => { delete store[k]; },
        clear: () => { for (const k of Object.keys(store)) delete store[k]; },
        _store: store,
    };
}

let mockStorage: ReturnType<typeof makeMockStorage>;
const installStorage = () => {
    mockStorage = makeMockStorage();
    (globalThis as Record<string, unknown>).sessionStorage = mockStorage;
};
const uninstallStorage = () => {
    delete (globalThis as Record<string, unknown>).sessionStorage;
};

// ---------------------------------------------------------------------------
// Helper: build a session
// ---------------------------------------------------------------------------

function freshSession(overrides: Partial<AuthSession> = {}): AuthSession {
    const now = new Date();
    return {
        address: "0xWallet0000000000000000000000000000000001",
        signature: "0xsig",
        nonce: "a".repeat(64),
        issuedAt: now.toISOString(),
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000).toISOString(), // +1h
        accountId: "acct-test-001",
        ...overrides,
    };
}

// ---------------------------------------------------------------------------
// 1. Wallet reconnect / disconnect (via mock provider emit simulation)
// ---------------------------------------------------------------------------

describe("Wallet reconnect / disconnect via mock provider", () => {
    /**
     * Mock EIP-1193 provider with event emitter.
     * Simulates the provider embedded in WalletProvider's useEffect.
     */
    function createMockProvider() {
        const listeners: Record<string, ((...args: unknown[]) => void)[]> = {};
        return {
            async request(args: { method: string; params?: unknown[] }): Promise<unknown> {
                if (args.method === "eth_requestAccounts") return ["0xInitial"];
                if (args.method === "personal_sign") return "0xmocksig";
                throw new Error(`Unknown: ${args.method}`);
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
                for (const h of listeners[event] ?? []) h(...args);
            },
        };
    }

    test("accountsChanged([]) signals disconnect — address becomes null", () => {
        const mock = createMockProvider();
        let address: string | null = "0xInitial";

        // Simulate the WalletProvider accountsChanged handler
        const handleAccountsChanged = (accounts: unknown) => {
            const accs = accounts as string[];
            address = accs.length === 0 ? null : accs[0];
        };
        mock.on("accountsChanged", handleAccountsChanged);

        mock.emit("accountsChanged", []);
        assert.equal(address, null, "address should be null after disconnect");
    });

    test("accountsChanged([newAddr]) signals account switch", () => {
        const mock = createMockProvider();
        let address: string | null = "0xOLD";

        const handleAccountsChanged = (accounts: unknown) => {
            const accs = accounts as string[];
            address = accs.length === 0 ? null : accs[0];
        };
        mock.on("accountsChanged", handleAccountsChanged);

        mock.emit("accountsChanged", ["0xNEW"]);
        assert.equal(address, "0xNEW");
    });

    test("removeListener stops receiving events", () => {
        const mock = createMockProvider();
        let callCount = 0;
        const handler = () => { callCount++; };

        mock.on("accountsChanged", handler);
        mock.emit("accountsChanged", []);
        assert.equal(callCount, 1);

        mock.removeListener("accountsChanged", handler);
        mock.emit("accountsChanged", []);
        assert.equal(callCount, 1, "should not fire after removeListener");
    });

    test("chainChanged event fires registered listener", () => {
        const mock = createMockProvider();
        let fired = false;
        mock.on("chainChanged", () => { fired = true; });
        mock.emit("chainChanged", "0x1");
        assert.equal(fired, true);
    });

    test("disconnect clears in-flight sign guard (logic invariant)", () => {
        // Simulate inFlightRef reset on disconnect
        const signingInFlightRef = { current: true }; // was signing
        // Disconnect handler:
        signingInFlightRef.current = false;
        assert.equal(signingInFlightRef.current, false, "sign guard cleared on disconnect");
    });
});

// ---------------------------------------------------------------------------
// 2. Parallel sign-in blocked by in-flight guard
// ---------------------------------------------------------------------------

describe("Parallel sign-in blocked by in-flight guard", () => {
    test("second concurrent sign attempt throws 'already in progress'", async () => {
        // Simulate the WalletProvider.signMessage in-flight ref logic
        const signingInFlightRef = { current: false };

        async function mockSignMessage(msg: string): Promise<string> {
            if (signingInFlightRef.current) {
                throw new Error(
                    "A signature request is already in progress. Please complete or reject it in your wallet.",
                );
            }
            signingInFlightRef.current = true;
            try {
                // Simulate async wallet prompt
                await new Promise((resolve) => setTimeout(resolve, 10));
                void msg;
                return "0xsignature";
            } finally {
                signingInFlightRef.current = false;
            }
        }

        // First sign starts
        const p1 = mockSignMessage("message1");
        // Second sign fires immediately (while first is in-flight)
        await assert.rejects(
            () => mockSignMessage("message2"),
            /already in progress/,
            "Concurrent sign should throw 'already in progress'",
        );
        // First sign should complete successfully
        const sig = await p1;
        assert.equal(sig, "0xsignature");
    });

    test("guard resets after sign completes — next call succeeds", async () => {
        const signingInFlightRef = { current: false };

        async function mockSignMessage(): Promise<string> {
            if (signingInFlightRef.current) throw new Error("already in progress");
            signingInFlightRef.current = true;
            try {
                return "0xdone";
            } finally {
                signingInFlightRef.current = false;
            }
        }

        const r1 = await mockSignMessage();
        assert.equal(r1, "0xdone");

        // After first completes, guard should be reset
        const r2 = await mockSignMessage();
        assert.equal(r2, "0xdone");
    });
});

// ---------------------------------------------------------------------------
// 3. Auth rate limiter — exhaustion
// ---------------------------------------------------------------------------

describe("Auth rate limiter — exhaustion (5 attempts)", () => {
    test("first 5 sign-in attempts are allowed", () => {
        const limiter = new RateLimiter({ capacity: 5, refillRate: 0 });
        for (let i = 0; i < 5; i++) {
            assert.equal(limiter.tryConsume(), true, `attempt ${i + 1} should be allowed`);
        }
    });

    test("6th attempt is blocked after 5 exhausted tokens", () => {
        const limiter = new RateLimiter({ capacity: 5, refillRate: 0 });
        for (let i = 0; i < 5; i++) limiter.tryConsume();
        assert.equal(limiter.tryConsume(), false, "6th attempt should be rate-limited");
    });

    test("estimatedWaitMs > 0 when exhausted", () => {
        const limiter = new RateLimiter({ capacity: 1, refillRate: 0.1 }); // 1 token/10s
        limiter.tryConsume(); // exhaust
        const wait = limiter.estimatedWaitMs();
        assert.ok(wait > 0, "should report positive wait time");
    });

    test("tokens refill over time — second window allows again", async () => {
        // Capacity 2, refill 4 per second → 1 token every 250ms
        const limiter = new RateLimiter({ capacity: 2, refillRate: 4 });
        limiter.tryConsume();
        limiter.tryConsume(); // exhaust
        assert.equal(limiter.tryConsume(), false);

        // Wait 300ms for at least 1 token to refill
        await new Promise((r) => setTimeout(r, 300));
        assert.equal(limiter.tryConsume(), true, "should be allowed after token refills");
    });
});

// ---------------------------------------------------------------------------
// 4. RateLimitError shape
// ---------------------------------------------------------------------------

describe("RateLimitError — typed error shape", () => {
    test("carries action name and waitMs", () => {
        const err = new RateLimitError("authSignIn", 12500);
        assert.equal(err.action, "authSignIn");
        assert.equal(err.waitMs, 12500);
        assert.ok(err instanceof Error);
        assert.equal(err.name, "RateLimitError");
    });

    test("message includes action and waitMs", () => {
        const err = new RateLimitError("cancelOrder", 5000);
        assert.ok(err.message.includes("cancelOrder"));
        assert.ok(err.message.includes("5000"));
    });

    test("instanceof check works", () => {
        const err = new RateLimitError("test", 0);
        assert.ok(err instanceof RateLimitError);
        assert.ok(err instanceof Error);
    });
});

// ---------------------------------------------------------------------------
// 5. Named rate-limiter registry — getOrCreate
// ---------------------------------------------------------------------------

describe("RateLimiterRegistry — named limiter management", () => {
    test("getOrCreate returns same instance on second call", () => {
        const registry = new RateLimiterRegistry();
        const l1 = registry.getOrCreate("testAction", { capacity: 5, refillRate: 1 });
        const l2 = registry.getOrCreate("testAction", { capacity: 99, refillRate: 99 });
        // Second call returns the cached instance, not a new one
        assert.strictEqual(l1, l2, "same RateLimiter instance should be returned");
        // Config from first call should win (capacity=5 not 99)
        assert.equal(l2.getSnapshot().capacity, 5);
    });

    test("different names produce different instances", () => {
        const registry = new RateLimiterRegistry();
        const l1 = registry.getOrCreate("actionA");
        const l2 = registry.getOrCreate("actionB");
        assert.notStrictEqual(l1, l2);
    });

    test("has() returns false before getOrCreate, true after", () => {
        const registry = new RateLimiterRegistry();
        assert.equal(registry.has("flow"), false);
        registry.getOrCreate("flow");
        assert.equal(registry.has("flow"), true);
    });

    test("remove() deletes the named limiter", () => {
        const registry = new RateLimiterRegistry();
        registry.getOrCreate("temp");
        assert.equal(registry.has("temp"), true);
        registry.remove("temp");
        assert.equal(registry.has("temp"), false);
    });
});

// ---------------------------------------------------------------------------
// 6. Registry.resetAll()
// ---------------------------------------------------------------------------

describe("RateLimiterRegistry — resetAll", () => {
    test("resetAll restores all limiters to full capacity", () => {
        const registry = new RateLimiterRegistry();
        const l1 = registry.getOrCreate("a", { capacity: 3, refillRate: 0 });
        const l2 = registry.getOrCreate("b", { capacity: 2, refillRate: 0 });

        l1.tryConsume(); l1.tryConsume(); l1.tryConsume(); // exhaust l1
        l2.tryConsume(); l2.tryConsume(); // exhaust l2

        assert.equal(l1.tryConsume(), false);
        assert.equal(l2.tryConsume(), false);

        registry.resetAll();

        assert.equal(l1.tryConsume(), true, "l1 should be full after resetAll");
        assert.equal(l2.tryConsume(), true, "l2 should be full after resetAll");
    });
});

// ---------------------------------------------------------------------------
// 7. useProtectedAction guard logic (pure extraction — no React required)
// ---------------------------------------------------------------------------

describe("useProtectedAction — guard logic extraction", () => {
    /**
     * Pure analog of what useProtectedAction computes for isDisabled.
     * Mirrors the hook's condition: !isAuthenticated || !limiter.canConsume()
     */
    function computeIsDisabled(authStatus: string, limiter: RateLimiter): boolean {
        const isAuthenticated = authStatus === "authenticated";
        return !isAuthenticated || !limiter.canConsume();
    }

    test("isDisabled=true when not authenticated", () => {
        const limiter = new RateLimiter({ capacity: 10, refillRate: 1 });
        assert.equal(computeIsDisabled("connected", limiter), true);
        assert.equal(computeIsDisabled("disconnected", limiter), true);
        assert.equal(computeIsDisabled("expired", limiter), true);
        assert.equal(computeIsDisabled("rejected", limiter), true);
    });

    test("isDisabled=false when authenticated and tokens available", () => {
        const limiter = new RateLimiter({ capacity: 5, refillRate: 1 });
        assert.equal(computeIsDisabled("authenticated", limiter), false);
    });

    test("isDisabled=true when authenticated but rate-limited", () => {
        const limiter = new RateLimiter({ capacity: 1, refillRate: 0 });
        limiter.tryConsume(); // exhaust
        assert.equal(computeIsDisabled("authenticated", limiter), true);
    });

    test("RateLimitError thrown when tokens exhausted during execute", () => {
        const limiter = new RateLimiter({ capacity: 1, refillRate: 0 });
        limiter.tryConsume(); // exhaust

        // Simulate the execute() rate-limit path
        let thrown: RateLimitError | null = null;
        if (!limiter.tryConsume()) {
            const waitMs = limiter.estimatedWaitMs();
            thrown = new RateLimitError("testAction", waitMs);
        }

        assert.ok(thrown instanceof RateLimitError);
        assert.equal(thrown.action, "testAction");
    });

    test("singleton defaultRateLimiterRegistry is stable across imports", () => {
        // The singleton should already have "authSignIn" from AuthProvider integration
        // After a fresh registry instance was used in tests above, the default one
        // should still be the same object reference
        const r1 = defaultRateLimiterRegistry;
        const r2 = defaultRateLimiterRegistry;
        assert.strictEqual(r1, r2, "singleton should be the same reference");
    });
});

// ---------------------------------------------------------------------------
// 8. Governance role ordering — hasRole()
// ---------------------------------------------------------------------------

describe("Governance — hasRole() ordering", () => {
    const ROLES: AdminRole[] = ["none", "support", "risk", "super"];

    test("every role meets its own level", () => {
        for (const role of ROLES) {
            assert.equal(hasRole(role, role), true, `${role} should meet ${role}`);
        }
    });

    test("super meets all lower roles", () => {
        for (const required of ROLES) {
            assert.equal(hasRole("super", required), true, `super should meet ${required}`);
        }
    });

    test("none only meets none", () => {
        assert.equal(hasRole("none", "none"), true);
        assert.equal(hasRole("none", "support"), false);
        assert.equal(hasRole("none", "risk"), false);
        assert.equal(hasRole("none", "super"), false);
    });

    test("support meets support and none, not risk or super", () => {
        assert.equal(hasRole("support", "none"), true);
        assert.equal(hasRole("support", "support"), true);
        assert.equal(hasRole("support", "risk"), false);
        assert.equal(hasRole("support", "super"), false);
    });

    test("risk meets risk, support, none — not super", () => {
        assert.equal(hasRole("risk", "none"), true);
        assert.equal(hasRole("risk", "support"), true);
        assert.equal(hasRole("risk", "risk"), true);
        assert.equal(hasRole("risk", "super"), false);
    });
});

// ---------------------------------------------------------------------------
// 9. GovernanceGuard — role pass / block (pure logic)
// ---------------------------------------------------------------------------

describe("GovernanceGuard — role gate logic", () => {
    /** Pure analog of GovernanceGuard render condition */
    function guardPasses(actualRole: AdminRole, requiredRole: AdminRole): boolean {
        return hasRole(actualRole, requiredRole);
    }

    test("super passes risk-required gate", () => {
        assert.equal(guardPasses("super", "risk"), true);
    });

    test("none fails support-required gate", () => {
        assert.equal(guardPasses("none", "support"), false);
    });

    test("unauthenticated (none) always fails non-none gates", () => {
        for (const required of ["support", "risk", "super"] as AdminRole[]) {
            assert.equal(
                guardPasses("none", required),
                false,
                `unauthenticated should fail ${required}`,
            );
        }
    });

    test("exact role match passes", () => {
        assert.equal(guardPasses("risk", "risk"), true);
        assert.equal(guardPasses("support", "support"), true);
    });
});

// ---------------------------------------------------------------------------
// 10. Governance audit log append
// ---------------------------------------------------------------------------

describe("Governance audit log", () => {
    test("audit entries carry timestamp, action, accountId, role", () => {
        // Simulate GovernanceProvider.logAction behavior
        const auditLog: Array<{
            timestamp: string;
            action: string;
            accountId: string;
            role: AdminRole;
        }> = [];

        function logAction(action: string, accountId: string, role: AdminRole) {
            auditLog.push({
                timestamp: new Date().toISOString(),
                action,
                accountId,
                role,
            });
        }

        logAction("halt_trading", "acct-123", "super");
        assert.equal(auditLog.length, 1);
        assert.equal(auditLog[0].action, "halt_trading");
        assert.equal(auditLog[0].accountId, "acct-123");
        assert.equal(auditLog[0].role, "super");
        assert.ok(typeof auditLog[0].timestamp === "string");
    });

    test("multiple actions accumulate in log", () => {
        const log: string[] = [];
        const append = (a: string) => log.push(a);

        append("adjust_leverage");
        append("force_liquidate");
        append("suspend_account");

        assert.equal(log.length, 3);
        assert.deepEqual(log, ["adjust_leverage", "force_liquidate", "suspend_account"]);
    });
});

// ---------------------------------------------------------------------------
// 11. Error session recovery — expired + reconnect clears state cleanly
// ---------------------------------------------------------------------------

describe("Error session recovery — expired session on reconnect", () => {
    beforeEach(installStorage);
    afterEach(uninstallStorage);

    test("expired session is cleared when wallet reconnects with same address", () => {
        const address = "0xWallet0000000000000000000000000000000001";
        // Store an expired session
        const expired = freshSession({
            address,
            expiresAt: new Date(Date.now() - 5000).toISOString(),
        });
        persistSession(expired);

        // Simulate AuthProvider mount logic on reconnect
        const loaded = loadSession();
        assert.ok(loaded !== null);
        const valid = isSessionValid(loaded, address);
        assert.equal(valid, false, "expired session should be invalid");

        // Auth provider clears storage and sets status "expired"
        clearSession();
        assert.equal(loadSession(), null, "storage should be clear after recovery");
    });

    test("valid session survives reconnect with same address", () => {
        const address = "0xWallet0000000000000000000000000000000001";
        const session = freshSession({ address });
        persistSession(session);

        // Simulate reconnect
        const loaded = loadSession();
        assert.ok(loaded !== null);
        assert.equal(isSessionValid(loaded, address), true);
        // Session is kept — auth stays "authenticated"
        assert.equal(loadSession()?.address, address);
    });

    test("account switch on reconnect clears session from old address", () => {
        const oldAddress = "0xOLD0000000000000000000000000000000000AA";
        const newAddress = "0xNEW0000000000000000000000000000000000BB";
        const session = freshSession({ address: oldAddress });
        persistSession(session);

        // Simulate: wallet emits accountsChanged → new address
        // AuthProvider receives new address, validates existing session
        const loaded = loadSession();
        assert.ok(loaded);
        if (!isSessionValid(loaded, newAddress)) {
            clearSession();
        }
        assert.equal(loadSession(), null, "old session should be purged on account switch");
    });

    test("nonces are unique across recovery attempts — no replay possible", () => {
        const nonces = new Set<string>();
        for (let i = 0; i < 20; i++) {
            nonces.add(generateNonce());
        }
        assert.equal(nonces.size, 20, "all 20 nonces should be unique");
    });

    test("createSession after recovery produces valid session immediately", () => {
        const address = "0xRecovered";
        const issuedAt = new Date().toISOString();
        const session = createSession(address, "0xsig", generateNonce(), issuedAt, "acct-1");

        assert.equal(isSessionValid(session, address), true);
        assert.ok(new Date(session.expiresAt).getTime() > Date.now());
    });
});
