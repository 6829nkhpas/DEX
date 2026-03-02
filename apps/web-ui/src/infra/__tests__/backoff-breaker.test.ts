// ---------------------------------------------------------------------------
// Tests for WS adaptive backoff and circuit-breaker integration
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { CircuitBreaker, CircuitBreakerOpenError } from "../circuit-breaker";
import { RateLimiter } from "../rate-limiter";

// ===========================================================================
// Adaptive WS Backoff
// ===========================================================================

describe("Adaptive WS backoff with jitter", () => {
  /** Reimplementation of ws-client backoffDelay for testing */
  function backoffDelay(attempt: number, jitterSeed?: number): number {
    const base = Math.min(500 * Math.pow(2, attempt), 16_000);
    const jitter = base * 0.2 * ((jitterSeed ?? Math.random()) * 2 - 1);
    return Math.max(0, base + jitter);
  }

  test("attempt 0 gives ~500ms base", () => {
    // With no jitter (seed=0.5 → jitter=0)
    const delay = backoffDelay(0, 0.5);
    assert.equal(delay, 500);
  });

  test("attempt 1 gives ~1000ms base", () => {
    const delay = backoffDelay(1, 0.5);
    assert.equal(delay, 1000);
  });

  test("attempt 5 gives ~16000ms (cap)", () => {
    const delay = backoffDelay(5, 0.5);
    assert.equal(delay, 16000);
  });

  test("high attempts are capped at 16000ms", () => {
    const delay = backoffDelay(100, 0.5);
    assert.equal(delay, 16000);
  });

  test("jitter produces delay within ±20% range", () => {
    for (let i = 0; i < 50; i++) {
      const delay = backoffDelay(2); // base=2000
      assert.ok(delay >= 2000 * 0.8, `delay ${delay} < minimum`);
      assert.ok(delay <= 2000 * 1.2, `delay ${delay} > maximum`);
    }
  });

  test("max attempts tracking", () => {
    const maxAttempts = 10;
    let reconnectAttempt = 0;
    const attempts: number[] = [];

    // Simulate reconnection loop
    while (reconnectAttempt < maxAttempts) {
      const delay = backoffDelay(reconnectAttempt, 0.5);
      attempts.push(delay);
      reconnectAttempt++;
    }

    assert.equal(attempts.length, 10);
    // First attempts should be smaller
    assert.ok(attempts[0] < attempts[3]);
    // Later attempts cap at 16000
    assert.equal(attempts[attempts.length - 1], 16000);
  });
});

// ===========================================================================
// Circuit Breaker + Rate Limiter integration (REST call wrapper)
// ===========================================================================

describe("REST call protection — circuit breaker + rate limiter", () => {
  test("rate limiter blocks burst after exhaustion", () => {
    const rl = new RateLimiter({ capacity: 3, refillRate: 0 });
    const cb = new CircuitBreaker({ failureThreshold: 5 });

    // Simulate 5 rapid calls
    const results: boolean[] = [];
    for (let i = 0; i < 5; i++) {
      if (rl.tryConsume() && cb.canRequest()) {
        results.push(true);
      } else {
        results.push(false);
      }
    }

    // First 3 succeed, last 2 rate-limited
    assert.deepEqual(results, [true, true, true, false, false]);
  });

  test("circuit breaker blocks after failures even with tokens", () => {
    const rl = new RateLimiter({ capacity: 100, refillRate: 100 });
    const cb = new CircuitBreaker({ failureThreshold: 3 });

    // 3 failures to trip the breaker
    cb.recordFailure(500);
    cb.recordFailure(500);
    cb.recordFailure(500);

    // Even though rate limiter has tokens, breaker blocks
    assert.equal(rl.tryConsume(), true);
    assert.equal(cb.canRequest(), false);
  });

  test("both protections compose correctly", () => {
    const rl = new RateLimiter({ capacity: 5, refillRate: 0 });
    const cb = new CircuitBreaker({ failureThreshold: 2 });

    // Make calls, some fail
    const attempt = (): "success" | "rate_limited" | "breaker_open" => {
      if (!rl.tryConsume()) return "rate_limited";
      if (!cb.canRequest()) return "breaker_open";
      return "success";
    };

    assert.equal(attempt(), "success");
    cb.recordFailure(500);
    assert.equal(attempt(), "success");
    cb.recordFailure(500);
    // Breaker now open
    assert.equal(attempt(), "breaker_open");
  });
});

// ===========================================================================
// Breaker state exposure for UI
// ===========================================================================

describe("Breaker state exposure for UI", () => {
  test("getSnapshot provides all data needed for UI display", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3, cooldownMs: 5000 });
    cb.recordFailure(500);
    cb.recordFailure(429);

    const snap = cb.getSnapshot();
    assert.equal(snap.state, "CLOSED");
    assert.equal(snap.consecutiveFailures, 2);
    assert.ok(snap.lastFailureTime > 0);
    assert.equal(snap.totalTrips, 0);
  });

  test("state change listener allows reactive UI updates", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1 });
    const stateChanges: string[] = [];
    cb.onStateChange((state) => stateChanges.push(state));

    cb.recordFailure(500); // CLOSED → OPEN
    cb.reset();           // OPEN → CLOSED

    assert.deepEqual(stateChanges, ["OPEN", "CLOSED"]);
  });

  test("unsubscribe stops notifications", () => {
    const cb = new CircuitBreaker({ failureThreshold: 1 });
    const states: string[] = [];
    const unsub = cb.onStateChange((s) => states.push(s));

    cb.recordFailure(500);
    assert.equal(states.length, 1); // "OPEN"

    unsub();
    cb.reset(); // should not trigger the removed listener
    assert.equal(states.length, 1); // still 1
  });
});
