// ---------------------------------------------------------------------------
// Telemetry — sampled event emission for observability
// ---------------------------------------------------------------------------
//
// Events are sampled at a configurable rate (default 1%) to minimize
// performance overhead. In production, events are POSTed to a telemetry
// endpoint. In dev, they can be logged to console or collected by a stub.
//
// All telemetry is fire-and-forget — never blocks or throws.
// ---------------------------------------------------------------------------

export type TelemetryEventType =
  | "connection_lifecycle"
  | "gap_detected"
  | "buffer_overflow"
  | "snapshot_request"
  | "subscription_count"
  | "cpu_warning"
  | "circuit_breaker_trip"
  | "rate_limit_hit"
  | "ws_reconnect";

export interface TelemetryEvent {
  type: TelemetryEventType;
  timestamp: string; // ISO 8601
  data: Record<string, unknown>;
  session_id?: string;
}

export interface TelemetryConfig {
  /** Sampling rate 0..1 (e.g. 0.01 = 1%). Default: 0.01 */
  sampleRate: number;
  /** Telemetry endpoint URL. Default: "/telemetry" */
  endpoint: string;
  /** Whether telemetry is enabled at all. Default: true */
  enabled: boolean;
  /** Batch size before flush. Default: 10 */
  batchSize: number;
  /** Flush interval in ms. Default: 5000 */
  flushIntervalMs: number;
  /** Dev mode: log to console instead of sending. Default: false */
  devMode: boolean;
}

const DEFAULT_CONFIG: TelemetryConfig = {
  sampleRate: 0.01,
  endpoint: "/telemetry",
  enabled: true,
  batchSize: 10,
  flushIntervalMs: 5_000,
  devMode: false,
};

export class TelemetryClient {
  private readonly config: TelemetryConfig;
  private buffer: TelemetryEvent[] = [];
  private flushTimer: ReturnType<typeof setInterval> | null = null;
  private sessionId: string;
  private totalEmitted = 0;
  private totalSampled = 0;
  private totalDropped = 0;

  constructor(config?: Partial<TelemetryConfig>) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.sessionId = `tel-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    if (this.config.enabled && this.config.flushIntervalMs > 0) {
      this.flushTimer = setInterval(() => this.flush(), this.config.flushIntervalMs);
    }
  }

  /**
   * Emit a telemetry event. Subject to sampling — may be silently dropped.
   */
  emit(type: TelemetryEventType, data: Record<string, unknown> = {}): void {
    if (!this.config.enabled) return;

    this.totalEmitted++;

    // Sampling gate
    if (Math.random() > this.config.sampleRate) {
      this.totalDropped++;
      return;
    }

    this.totalSampled++;

    const event: TelemetryEvent = {
      type,
      timestamp: new Date().toISOString(),
      data,
      session_id: this.sessionId,
    };

    if (this.config.devMode) {
      // eslint-disable-next-line no-console
      console.debug("[telemetry]", event.type, event.data);
    }

    this.buffer.push(event);

    if (this.buffer.length >= this.config.batchSize) {
      this.flush();
    }
  }

  /**
   * Force-emit (bypasses sampling). Use for critical events.
   */
  forceEmit(type: TelemetryEventType, data: Record<string, unknown> = {}): void {
    if (!this.config.enabled) return;

    this.totalEmitted++;
    this.totalSampled++;

    const event: TelemetryEvent = {
      type,
      timestamp: new Date().toISOString(),
      data,
      session_id: this.sessionId,
    };

    if (this.config.devMode) {
      // eslint-disable-next-line no-console
      console.debug("[telemetry:force]", event.type, event.data);
    }

    this.buffer.push(event);

    if (this.buffer.length >= this.config.batchSize) {
      this.flush();
    }
  }

  /**
   * Flush buffered events to the endpoint.
   */
  flush(): void {
    if (this.buffer.length === 0) return;

    const batch = this.buffer.splice(0);

    if (this.config.devMode) {
      // In dev mode, just log; do not send
      return;
    }

    // Fire-and-forget POST
    try {
      if (typeof fetch !== "undefined") {
        fetch(this.config.endpoint, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ events: batch }),
        }).catch(() => {
          // Silently ignore telemetry send failures
        });
      }
    } catch {
      // Silently ignore
    }
  }

  /** Get telemetry stats for debugging. */
  getStats(): { totalEmitted: number; totalSampled: number; totalDropped: number; bufferSize: number } {
    return {
      totalEmitted: this.totalEmitted,
      totalSampled: this.totalSampled,
      totalDropped: this.totalDropped,
      bufferSize: this.buffer.length,
    };
  }

  /** Update sampling rate at runtime. */
  setSampleRate(rate: number): void {
    (this.config as { sampleRate: number }).sampleRate = Math.max(0, Math.min(1, rate));
  }

  /** Dispose timers. */
  dispose(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }
    this.flush();
  }
}

// ---------------------------------------------------------------------------
// Singleton instance — configure via env/config at startup
// ---------------------------------------------------------------------------

let _instance: TelemetryClient | null = null;

export function getTelemetryClient(config?: Partial<TelemetryConfig>): TelemetryClient {
  if (!_instance) {
    _instance = new TelemetryClient(config);
  }
  return _instance;
}

export function resetTelemetryClient(): void {
  if (_instance) {
    _instance.dispose();
    _instance = null;
  }
}
