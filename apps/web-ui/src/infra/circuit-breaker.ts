// ---------------------------------------------------------------------------
// Circuit Breaker — protects REST calls from cascading failures
// ---------------------------------------------------------------------------
//
// States: CLOSED → OPEN → HALF_OPEN → CLOSED
//   CLOSED:    requests pass through normally
//   OPEN:      requests are immediately rejected (fast-fail)
//   HALF_OPEN: one probe request allowed; success → CLOSED, failure → OPEN
//
// Triggers open after `failureThreshold` consecutive 5xx or 429 responses.
// Auto-transitions to HALF_OPEN after `cooldownMs`.
// ---------------------------------------------------------------------------

export type BreakerState = "CLOSED" | "OPEN" | "HALF_OPEN";

export interface CircuitBreakerConfig {
  /** Number of consecutive failures before opening the breaker. Default: 5 */
  failureThreshold: number;
  /** Cooldown period in ms before transitioning OPEN → HALF_OPEN. Default: 30000 */
  cooldownMs: number;
  /** HTTP status codes that count as failures. Default: [429, 500, 502, 503, 504] */
  failureStatuses: number[];
}

export interface CircuitBreakerSnapshot {
  state: BreakerState;
  consecutiveFailures: number;
  lastFailureTime: number;
  totalTrips: number;
}

const DEFAULT_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 5,
  cooldownMs: 30_000,
  failureStatuses: [429, 500, 502, 503, 504],
};

export class CircuitBreaker {
  private _state: BreakerState = "CLOSED";
  private consecutiveFailures = 0;
  private lastFailureTime = 0;
  private totalTrips = 0;
  private readonly config: CircuitBreakerConfig;
  private listeners: ((state: BreakerState) => void)[] = [];

  constructor(config?: Partial<CircuitBreakerConfig>) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  get state(): BreakerState {
    this.checkTransition();
    return this._state;
  }

  private checkTransition(): void {
    // Check if we should auto-transition OPEN → HALF_OPEN
    if (this._state === "OPEN" && this.lastFailureTime > 0) {
      const elapsed = Date.now() - this.lastFailureTime;
      if (elapsed >= this.config.cooldownMs) {
        this._state = "HALF_OPEN";
        this.notifyListeners();
      }
    }
  }

  /**
   * Check if the breaker allows a request.
   * Returns true if allowed, false if the breaker is open.
   */
  canRequest(): boolean {
    const s = this.state; // triggers auto-transition check
    if (s === "CLOSED") return true;
    if (s === "HALF_OPEN") return true; // allow one probe
    return false; // OPEN
  }

  /**
   * Record a successful response. Resets the breaker to CLOSED.
   */
  recordSuccess(): void {
    this.consecutiveFailures = 0;
    if (this._state !== "CLOSED") {
      this._state = "CLOSED";
      this.notifyListeners();
    }
  }

  /**
   * Record a failure response. If threshold exceeded, opens the breaker.
   */
  recordFailure(statusCode?: number): void {
    // Only count configured failure statuses if a status is provided
    if (statusCode !== undefined && !this.config.failureStatuses.includes(statusCode)) {
      return;
    }

    this.consecutiveFailures++;
    this.lastFailureTime = Date.now();

    if (this._state === "HALF_OPEN") {
      // Probe failed → back to OPEN
      this._state = "OPEN";
      this.totalTrips++;
      this.notifyListeners();
      return;
    }

    if (this.consecutiveFailures >= this.config.failureThreshold && this._state === "CLOSED") {
      this._state = "OPEN";
      this.totalTrips++;
      this.notifyListeners();
    }
  }

  /**
   * Execute a fetch-like function through the breaker.
   * Rejects immediately if breaker is open.
   */
  async execute<T>(fn: () => Promise<Response>): Promise<Response> {
    if (!this.canRequest()) {
      throw new CircuitBreakerOpenError(this.getSnapshot());
    }

    try {
      const response = await fn();
      if (this.config.failureStatuses.includes(response.status)) {
        this.recordFailure(response.status);
      } else {
        this.recordSuccess();
      }
      return response;
    } catch (err) {
      this.recordFailure();
      throw err;
    }
  }

  /** Get current breaker snapshot for monitoring. */
  getSnapshot(): CircuitBreakerSnapshot {
    return {
      state: this.state,
      consecutiveFailures: this.consecutiveFailures,
      lastFailureTime: this.lastFailureTime,
      totalTrips: this.totalTrips,
    };
  }

  /** Register a listener for state changes. */
  onStateChange(listener: (state: BreakerState) => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  }

  /** Force reset (for testing or admin). */
  reset(): void {
    this._state = "CLOSED";
    this.consecutiveFailures = 0;
    this.lastFailureTime = 0;
    this.notifyListeners();
  }

  private notifyListeners(): void {
    for (const l of this.listeners) {
      l(this._state);
    }
  }
}

export class CircuitBreakerOpenError extends Error {
  public readonly snapshot: CircuitBreakerSnapshot;
  constructor(snapshot: CircuitBreakerSnapshot) {
    super(`Circuit breaker is OPEN (${snapshot.consecutiveFailures} consecutive failures, ${snapshot.totalTrips} total trips)`);
    this.name = "CircuitBreakerOpenError";
    this.snapshot = snapshot;
  }
}
