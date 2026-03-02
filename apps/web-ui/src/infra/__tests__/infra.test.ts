// ---------------------------------------------------------------------------
// Tests for CircuitBreaker, RateLimiter, Telemetry, SafeDisplay, TokenManager
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { CircuitBreaker, CircuitBreakerOpenError } from "../circuit-breaker";
import { RateLimiter } from "../rate-limiter";
import { TelemetryClient, resetTelemetryClient } from "../telemetry";
import { escapeHtml, sanitizeSymbol, sanitizeId, safeDecimalDisplay, truncateDisplay } from "../safe-display";
import { TokenManager } from "../token-manager";

// ===========================================================================
// Circuit Breaker
// ===========================================================================

describe("CircuitBreaker — state transitions", () => {
  test("starts in CLOSED state", () => {
    const cb = new CircuitBreaker();
    assert.equal(cb.state, "CLOSED");
    assert.equal(cb.canRequest(), true);
  });

  test("stays CLOSED below failure threshold", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3 });
    cb.recordFailure(500);
    cb.recordFailure(500);
    assert.equal(cb.state, "CLOSED");
    assert.equal(cb.canRequest(), true);
  });

  test("opens after reaching failure threshold", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3 });
    cb.recordFailure(500);
    cb.recordFailure(502);
    cb.recordFailure(503);
    assert.equal(cb.state, "OPEN");
    assert.equal(cb.canRequest(), false);
  });

  test("counts 429 as a failure", () => {
    const cb = new CircuitBreaker({ failureThreshold: 2 });
    cb.recordFailure(429);
    cb.recordFailure(429);
    assert.equal(cb.state, "OPEN");
  });

  test("ignores non-failure status codes", () => {
    const cb = new CircuitBreaker({ failureThreshold: 2 });
    cb.recordFailure(400);
    cb.recordFailure(404);
    assert.equal(cb.state, "CLOSED");
  });

  test("resets on success", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3 });
    cb.recordFailure(500);
    cb.recordFailure(500);
    cb.recordSuccess();
    // Should have reset consecutive, now needs 3 more failures
    cb.recordFailure(500);
    cb.recordFailure(500);
    assert.equal(cb.state, "CLOSED");
  });

  test("transitions OPEN → HALF_OPEN after cooldown", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1, cooldownMs: 50 });
    cb.recordFailure(500);
    assert.equal(cb.state, "OPEN");
    // Wait for cooldown
    const start = Date.now();
    while (Date.now() - start < 60) { /* spin */ }
    assert.equal(cb.state, "HALF_OPEN");
    assert.equal(cb.canRequest(), true);
  });

  test("HALF_OPEN → CLOSED on success", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1, cooldownMs: 0 });
    cb.recordFailure(500);
    // OPEN → HALF_OPEN (cooldown=0)
    assert.equal(cb.state, "HALF_OPEN");
    cb.recordSuccess();
    assert.equal(cb.state, "CLOSED");
  });

  test("HALF_OPEN → OPEN on probe failure", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1, cooldownMs: 100 });
    cb.recordFailure(500);
    // Wait for cooldown to get to HALF_OPEN
    const start = Date.now();
    while (Date.now() - start < 110) { /* spin */ }
    assert.equal(cb.state, "HALF_OPEN");
    cb.recordFailure(500);
    // Immediately after failure, state should be OPEN (before cooldown elapses again)
    assert.equal(cb["_state"], "OPEN");
  });

  test("getSnapshot returns correct data", () => {
    const cb = new CircuitBreaker({ failureThreshold: 2 });
    cb.recordFailure(500);
    cb.recordFailure(500);
    const snap = cb.getSnapshot();
    assert.equal(snap.state, "OPEN");
    assert.equal(snap.consecutiveFailures, 2);
    assert.equal(snap.totalTrips, 1);
  });

  test("state change listener fires on transitions", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1 });
    const states: string[] = [];
    cb.onStateChange((s) => states.push(s));
    cb.recordFailure(500);
    assert.deepEqual(states, ["OPEN"]);
  });

  test("reset restores CLOSED state", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1 });
    cb.recordFailure(500);
    assert.equal(cb.state, "OPEN");
    cb.reset();
    assert.equal(cb.state, "CLOSED");
    assert.equal(cb.canRequest(), true);
  });
});

describe("CircuitBreaker — execute wrapper", () => {
  test("passes through successful responses", async () => {
    const cb = new CircuitBreaker();
    const mockResponse = new Response("ok", { status: 200 });
    const result = await cb.execute(() => Promise.resolve(mockResponse));
    assert.equal(result.status, 200);
  });

  test("records failure for 500 responses", async () => {
    const cb = new CircuitBreaker({ failureThreshold: 2 });
    const mockResponse = new Response("err", { status: 500 });
    await cb.execute(() => Promise.resolve(mockResponse));
    assert.equal(cb.getSnapshot().consecutiveFailures, 1);
  });

  test("rejects when breaker is open", async () => {
    const cb = new CircuitBreaker({ failureThreshold: 1 });
    cb.recordFailure(500);
    await assert.rejects(
      () => cb.execute(() => Promise.resolve(new Response("", { status: 200 }))),
      (err: unknown) => err instanceof CircuitBreakerOpenError,
    );
  });
});

// ===========================================================================
// Rate Limiter
// ===========================================================================

describe("RateLimiter — token bucket", () => {
  test("allows requests when tokens available", () => {
    const rl = new RateLimiter({ capacity: 3, refillRate: 1 });
    assert.equal(rl.tryConsume(), true);
    assert.equal(rl.tryConsume(), true);
    assert.equal(rl.tryConsume(), true);
  });

  test("rejects when tokens exhausted", () => {
    const rl = new RateLimiter({ capacity: 2, refillRate: 0 });
    assert.equal(rl.tryConsume(), true);
    assert.equal(rl.tryConsume(), true);
    assert.equal(rl.tryConsume(), false);
  });

  test("canConsume does not consume tokens", () => {
    const rl = new RateLimiter({ capacity: 1, refillRate: 0 });
    assert.equal(rl.canConsume(), true);
    assert.equal(rl.canConsume(), true); // still true, not consumed
    assert.equal(rl.tryConsume(), true);
    assert.equal(rl.canConsume(), false);
  });

  test("estimatedWaitMs returns 0 when tokens available", () => {
    const rl = new RateLimiter({ capacity: 5, refillRate: 1 });
    assert.equal(rl.estimatedWaitMs(), 0);
  });

  test("estimatedWaitMs returns positive when exhausted", () => {
    const rl = new RateLimiter({ capacity: 1, refillRate: 1 });
    rl.tryConsume();
    const wait = rl.estimatedWaitMs();
    assert.ok(wait > 0);
  });

  test("getSnapshot reflects current state", () => {
    const rl = new RateLimiter({ capacity: 5, refillRate: 2 });
    const snap = rl.getSnapshot();
    assert.equal(snap.capacity, 5);
    assert.equal(snap.refillRate, 2);
    assert.ok(snap.tokens <= 5);
  });

  test("reset restores full capacity", () => {
    const rl = new RateLimiter({ capacity: 3, refillRate: 0 });
    rl.tryConsume();
    rl.tryConsume();
    rl.tryConsume();
    assert.equal(rl.tryConsume(), false);
    rl.reset();
    assert.equal(rl.tryConsume(), true);
  });
});

// ===========================================================================
// Telemetry
// ===========================================================================

describe("TelemetryClient — sampling and emission", () => {
  test("emits events when enabled with 100% sample rate", () => {
    const tel = new TelemetryClient({
      enabled: true,
      sampleRate: 1.0,
      devMode: true,
      flushIntervalMs: 0,
    });
    tel.emit("connection_lifecycle", { action: "connect" });
    const stats = tel.getStats();
    assert.equal(stats.totalEmitted, 1);
    assert.equal(stats.totalSampled, 1);
    assert.equal(stats.totalDropped, 0);
    tel.dispose();
  });

  test("drops all events at 0% sample rate", () => {
    const tel = new TelemetryClient({
      enabled: true,
      sampleRate: 0,
      devMode: true,
      flushIntervalMs: 0,
    });
    for (let i = 0; i < 100; i++) {
      tel.emit("gap_detected", { seq: i });
    }
    const stats = tel.getStats();
    assert.equal(stats.totalEmitted, 100);
    assert.equal(stats.totalSampled, 0);
    assert.equal(stats.totalDropped, 100);
    tel.dispose();
  });

  test("forceEmit bypasses sampling", () => {
    const tel = new TelemetryClient({
      enabled: true,
      sampleRate: 0,
      devMode: true,
      flushIntervalMs: 0,
    });
    tel.forceEmit("circuit_breaker_trip", { state: "OPEN" });
    const stats = tel.getStats();
    assert.equal(stats.totalSampled, 1);
    tel.dispose();
  });

  test("disabled telemetry drops everything", () => {
    const tel = new TelemetryClient({
      enabled: false,
      sampleRate: 1.0,
      flushIntervalMs: 0,
    });
    tel.emit("buffer_overflow", {});
    assert.equal(tel.getStats().totalEmitted, 0);
    tel.dispose();
  });

  test("setSampleRate changes rate at runtime", () => {
    const tel = new TelemetryClient({
      enabled: true,
      sampleRate: 0,
      devMode: true,
      flushIntervalMs: 0,
    });
    tel.emit("cpu_warning", {});
    assert.equal(tel.getStats().totalDropped, 1);

    tel.setSampleRate(1.0);
    tel.emit("cpu_warning", {});
    assert.equal(tel.getStats().totalSampled, 1);
    tel.dispose();
  });
});

// ===========================================================================
// Safe Display
// ===========================================================================

describe("SafeDisplay — encoding helpers", () => {
  test("escapeHtml escapes dangerous characters", () => {
    assert.equal(escapeHtml("<script>alert('xss')</script>"), "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;");
    assert.equal(escapeHtml('a"b'), "a&quot;b");
    assert.equal(escapeHtml("a&b"), "a&amp;b");
  });

  test("sanitizeSymbol strips dangerous chars", () => {
    assert.equal(sanitizeSymbol("BTC/USDT"), "BTC/USDT");
    assert.equal(sanitizeSymbol("BTC<script>/USDT"), "BTCscript/USDT");
    assert.equal(sanitizeSymbol("ETH-PERP_01"), "ETH-PERP_01");
  });

  test("sanitizeId strips non-ID chars", () => {
    assert.equal(sanitizeId("abc-123_def"), "abc-123_def");
    assert.equal(sanitizeId("abc<>123"), "abc123");
  });

  test("safeDecimalDisplay returns value for valid decimals", () => {
    assert.equal(safeDecimalDisplay("50000.00"), "50000.00");
    assert.equal(safeDecimalDisplay("-1.5"), "-1.5");
    assert.equal(safeDecimalDisplay("123"), "123");
  });

  test("safeDecimalDisplay returns '0' for invalid strings", () => {
    assert.equal(safeDecimalDisplay("not-a-number"), "0");
    assert.equal(safeDecimalDisplay("<script>"), "0");
    assert.equal(safeDecimalDisplay(""), "0");
  });

  test("truncateDisplay truncates long strings", () => {
    assert.equal(truncateDisplay("Hello", 10), "Hello");
    assert.equal(truncateDisplay("Hello, World!", 5), "Hello…");
  });
});

// ===========================================================================
// Token Manager
// ===========================================================================

describe("TokenManager — mutex refresh", () => {
  test("returns current token when not expired", async () => {
    const tm = new TokenManager("tok-1", Date.now() + 60_000, {
      refreshFn: async () => ({ accessToken: "tok-2", expiresAt: Date.now() + 60_000 }),
    });
    const token = await tm.getToken();
    assert.equal(token, "tok-1");
  });

  test("refreshes when token is expired", async () => {
    let refreshCount = 0;
    const tm = new TokenManager("tok-1", Date.now() - 1000, {
      refreshFn: async () => {
        refreshCount++;
        return { accessToken: "tok-fresh", expiresAt: Date.now() + 60_000 };
      },
    });
    const token = await tm.getToken();
    assert.equal(token, "tok-fresh");
    assert.equal(refreshCount, 1);
  });

  test("mutex prevents parallel refreshes", async () => {
    let refreshCount = 0;
    const tm = new TokenManager("tok-1", Date.now() - 1000, {
      refreshFn: async () => {
        refreshCount++;
        await new Promise((r) => setTimeout(r, 50));
        return { accessToken: "tok-mutex", expiresAt: Date.now() + 60_000 };
      },
    });

    // Fire 3 concurrent getToken calls
    const [t1, t2, t3] = await Promise.all([
      tm.getToken(),
      tm.getToken(),
      tm.getToken(),
    ]);

    assert.equal(t1, "tok-mutex");
    assert.equal(t2, "tok-mutex");
    assert.equal(t3, "tok-mutex");
    // Only 1 refresh should have happened
    assert.equal(refreshCount, 1);
  });

  test("isExpired returns correct status", () => {
    const tmFresh = new TokenManager("tok", Date.now() + 120_000, {
      refreshFn: async () => ({ accessToken: "x", expiresAt: 0 }),
    });
    assert.equal(tmFresh.isExpired(), false);

    const tmExpired = new TokenManager("tok", Date.now() - 1000, {
      refreshFn: async () => ({ accessToken: "x", expiresAt: 0 }),
    });
    assert.equal(tmExpired.isExpired(), true);
  });

  test("forceRefresh always calls refresh", async () => {
    let calls = 0;
    const tm = new TokenManager("tok", Date.now() + 60_000, {
      refreshFn: async () => {
        calls++;
        return { accessToken: `tok-${calls}`, expiresAt: Date.now() + 60_000 };
      },
    });

    // Token is valid, no refresh needed
    await tm.getToken();
    assert.equal(calls, 0);

    // Force refresh
    const fresh = await tm.forceRefresh();
    assert.equal(calls, 1);
    assert.equal(fresh, "tok-1");
  });
});
