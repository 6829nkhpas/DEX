// ---------------------------------------------------------------------------
// Client-side rate limiter — token-bucket algorithm
// ---------------------------------------------------------------------------
//
// Prevents burst resends of REST calls. Each call consumes a token.
// Tokens refill at a steady rate up to a configured capacity.
// When no tokens are available, the call is rejected with a reason.
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
