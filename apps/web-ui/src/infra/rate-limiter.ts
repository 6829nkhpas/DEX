// ---------------------------------------------------------------------------
// Client-side rate limiter — token-bucket algorithm
// ---------------------------------------------------------------------------
//
// Prevents burst resends of REST calls. Each call consumes a token.
// Tokens refill at a steady rate up to a configured capacity.
// When no tokens are available, the call is rejected with a reason.
//
// Also provides:
//   RateLimitError          — typed error carrying the action name + wait time
//   RateLimiterRegistry     — named registry of RateLimiter instances
//   defaultRateLimiterRegistry — singleton registry shared across the app
// ---------------------------------------------------------------------------

export interface RateLimiterConfig {
  /** Maximum tokens (burst capacity). Default: 10 */
  capacity: number;
  /** Tokens refilled per second. Default: 2 */
  refillRate: number;
}

export interface RateLimiterSnapshot {
  tokens: number;
  capacity: number;
  refillRate: number;
  lastRefillTime: number;
}

const DEFAULT_CONFIG: RateLimiterConfig = {
  capacity: 10,
  refillRate: 2,
};

export class RateLimiter {
  private tokens: number;
  private lastRefillTime: number;
  private readonly config: RateLimiterConfig;

  constructor(config?: Partial<RateLimiterConfig>) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.tokens = this.config.capacity;
    this.lastRefillTime = Date.now();
  }

  /**
   * Attempt to consume a token. Returns true if allowed, false if rate-limited.
   */
  tryConsume(): boolean {
    this.refill();
    if (this.tokens >= 1) {
      this.tokens -= 1;
      return true;
    }
    return false;
  }

  /**
   * Check if a request would be allowed without consuming a token.
   */
  canConsume(): boolean {
    this.refill();
    return this.tokens >= 1;
  }

  /**
   * Get the estimated wait time in ms until a token is available.
   */
  estimatedWaitMs(): number {
    this.refill();
    if (this.tokens >= 1) return 0;
    const deficit = 1 - this.tokens;
    return Math.ceil((deficit / this.config.refillRate) * 1000);
  }

  /** Get current limiter snapshot for monitoring. */
  getSnapshot(): RateLimiterSnapshot {
    this.refill();
    return {
      tokens: this.tokens,
      capacity: this.config.capacity,
      refillRate: this.config.refillRate,
      lastRefillTime: this.lastRefillTime,
    };
  }

  /** Force reset to full capacity. */
  reset(): void {
    this.tokens = this.config.capacity;
    this.lastRefillTime = Date.now();
  }

  private refill(): void {
    const now = Date.now();
    const elapsed = (now - this.lastRefillTime) / 1000; // seconds
    const tokensToAdd = elapsed * this.config.refillRate;
    this.tokens = Math.min(this.config.capacity, this.tokens + tokensToAdd);
    this.lastRefillTime = now;
  }
}

// ---------------------------------------------------------------------------
// Typed error for rate-limited actions
// ---------------------------------------------------------------------------

/**
 * Thrown (or returned as an error field) when an action is rate-limited.
 * Carries the action name and the estimated wait before a token refills.
 */
export class RateLimitError extends Error {
  public readonly action: string;
  public readonly waitMs: number;

  constructor(action: string, waitMs: number) {
    super(`Rate limit exceeded for "${action}". Retry in ${waitMs}ms.`);
    this.name = "RateLimitError";
    this.action = action;
    this.waitMs = waitMs;
  }
}

// ---------------------------------------------------------------------------
// Named registry of RateLimiter instances
// ---------------------------------------------------------------------------

/**
 * Registry that lazily creates and caches named RateLimiter instances.
 * Ensures each named action (e.g. "signIn", "cancelOrder") shares a
 * single limiter across multiple call sites in the same tab.
 */
export class RateLimiterRegistry {
  private readonly limiters = new Map<string, RateLimiter>();

  /**
   * Get the named limiter, creating it (with provided config) on first access.
   * Subsequent calls with the same name return the cached instance.
   */
  getOrCreate(name: string, config?: Partial<RateLimiterConfig>): RateLimiter {
    if (!this.limiters.has(name)) {
      this.limiters.set(name, new RateLimiter(config));
    }
    return this.limiters.get(name)!;
  }

  /** Remove a named limiter (e.g. on logout / reset). */
  remove(name: string): void {
    this.limiters.delete(name);
  }

  /** Reset all limiters to full capacity. */
  resetAll(): void {
    for (const limiter of this.limiters.values()) {
      limiter.reset();
    }
  }

  /** Check if a named limiter exists. */
  has(name: string): boolean {
    return this.limiters.has(name);
  }
}

/**
 * Singleton registry — shared across auth, order entry, and cancel paths.
 * Import this instead of creating a new RateLimiterRegistry per component.
 */
export const defaultRateLimiterRegistry = new RateLimiterRegistry();
