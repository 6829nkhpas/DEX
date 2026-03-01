// ---------------------------------------------------------------------------
// E2E Test: 429 storm and client degradation behavior
// ---------------------------------------------------------------------------
//
// Simulates a burst of REST calls that all return 429, and verifies:
//   1. Circuit breaker trips after N consecutive failures
//   2. Rate limiter blocks burst after token exhaustion
//   3. Client properly queues/blocks subsequent requests
//   4. After cooldown, breaker transitions to HALF_OPEN and resumes
//
// Usage:
//   npx tsx --test src/infra/__tests__/rate-limit-e2e.test.ts
// ---------------------------------------------------------------------------

import { test, describe } from "node:test";
import assert from "node:assert/strict";
import { CircuitBreaker, CircuitBreakerOpenError } from "../circuit-breaker";
import { RateLimiter } from "../rate-limiter";

describe("429 storm — client-side degradation E2E", () => {

  test("circuit breaker trips after 429 storm", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3, cooldownMs: 1000 });
    const rl = new RateLimiter({ capacity: 10, refillRate: 0 });

    const callResults: Array<"success" | "rate_limited" | "breaker_open" | "429"> = [];

    // Simulate 10 calls that all return 429
    for (let i = 0; i < 10; i++) {
      if (!rl.tryConsume()) {
        callResults.push("rate_limited");
        continue;
      }
      if (!cb.canRequest()) {
        callResults.push("breaker_open");
        continue;
      }

      // Simulate server returning 429
      cb.recordFailure(429);
      callResults.push("429");
    }

    // First 3 calls get 429 (hits threshold), rest should be breaker_open
    assert.equal(callResults[0], "429");
    assert.equal(callResults[1], "429");
    assert.equal(callResults[2], "429");

    // After 3 failures, breaker opens
    for (let i = 3; i < 10; i++) {
      assert.equal(callResults[i], "breaker_open",
        `Call ${i} should be blocked by breaker, got ${callResults[i]}`);
    }

    assert.equal(cb.getSnapshot().state, "OPEN");
    assert.equal(cb.getSnapshot().totalTrips, 1);
  });

  test("rate limiter prevents burst resends", () => {
    const rl = new RateLimiter({ capacity: 5, refillRate: 0 });

    const results: boolean[] = [];
    for (let i = 0; i < 10; i++) {
      results.push(rl.tryConsume());
    }

    // First 5 succeed, remaining 5 blocked
    assert.deepEqual(results.slice(0, 5), [true, true, true, true, true]);
    assert.deepEqual(results.slice(5), [false, false, false, false, false]);
  });

  test("combined protection: rate limit + circuit breaker degrade gracefully", () => {
    const cb = new CircuitBreaker({ failureThreshold: 3, cooldownMs: 100 });
    const rl = new RateLimiter({ capacity: 5, refillRate: 0 });

    type Result = "success" | "rate_limited" | "breaker_open" | "server_429";

    function simulateCall(serverStatus: number): Result {
      if (!rl.tryConsume()) return "rate_limited";
      if (!cb.canRequest()) return "breaker_open";

      if (serverStatus === 429) {
        cb.recordFailure(429);
        return "server_429";
      } else {
        cb.recordSuccess();
        return "success";
      }
    }

    // Phase 1: Server returning 429
    const phase1: Result[] = [];
    for (let i = 0; i < 8; i++) {
      phase1.push(simulateCall(429));
    }

    // First 3 hit server (429), then breaker opens, last 2 rate limited
    assert.equal(phase1[0], "server_429");
    assert.equal(phase1[1], "server_429");
    assert.equal(phase1[2], "server_429");
    assert.equal(phase1[3], "breaker_open");
    assert.equal(phase1[4], "breaker_open");
    assert.equal(phase1[5], "rate_limited"); // tokens exhausted
    assert.equal(phase1[6], "rate_limited");
    assert.equal(phase1[7], "rate_limited");

    assert.equal(cb.getSnapshot().state, "OPEN");
  });

  test("recovery after cooldown: breaker allows probe then closes", () => {
    const cb = new CircuitBreaker({ failureThreshold: 2, cooldownMs: 50 });

    // Trip the breaker
    cb.recordFailure(429);
    cb.recordFailure(429);
    assert.equal(cb.getSnapshot().state, "OPEN");

    // Wait for cooldown
    const start = Date.now();
    while (Date.now() - start < 60) { /* spin */ }

    // Should auto-transition to HALF_OPEN
    assert.equal(cb.state, "HALF_OPEN");

    // Probe succeeds
    cb.recordSuccess();
    assert.equal(cb.state, "CLOSED");
    assert.equal(cb.canRequest(), true);
  });

  test("UI action queue simulation during rate limit", () => {
    const rl = new RateLimiter({ capacity: 2, refillRate: 10 });

    // Simulate 5 rapid user actions
    const queued: number[] = [];
    const executed: number[] = [];

    for (let i = 0; i < 5; i++) {
      if (rl.tryConsume()) {
        executed.push(i);
      } else {
        queued.push(i);
      }
    }

    // 2 execute immediately, 3 queued
    assert.equal(executed.length, 2);
    assert.equal(queued.length, 3);

    // After brief pause and refill, queued can execute
    const start = Date.now();
    while (Date.now() - start < 200) { /* spin — 200ms = ~2 tokens refilled */ }

    let retried = 0;
    for (const _ of queued) {
      if (rl.tryConsume()) retried++;
    }

    // Should be able to retry at least 1-2
    assert.ok(retried >= 1, `Expected at least 1 retry, got ${retried}`);
  });

  test("aggregated feed auto-switch on server hint (simulated)", () => {
    // When server returns a "use_aggregated_feed" hint,
    // the client should auto-switch subscription mode.
    // This is a behavioral test — simulates the decision logic.

    interface ServerHint {
      action: "use_aggregated_feed";
      reason: "rate_limit_exceeded";
    }

    let aggregatedMode = false;

    function handleServerHint(hint: ServerHint): void {
      if (hint.action === "use_aggregated_feed") {
        aggregatedMode = true;
      }
    }

    assert.equal(aggregatedMode, false);

    // Server sends hint after 429 storm
    handleServerHint({
      action: "use_aggregated_feed",
      reason: "rate_limit_exceeded",
    });

    assert.equal(aggregatedMode, true);
  });
});
