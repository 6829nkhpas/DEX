// ---------------------------------------------------------------------------
// Metrics — Reads store state and exports Prometheus-compatible dex_* metrics
// ---------------------------------------------------------------------------
//
// This module bridges the DexStateStore's internal metrics to the
// observability server's /metrics endpoint. It periodically collects
// store metrics and pushes them to the observability sidecar.
//
// Usage:
//   import { MetricsCollector } from './metrics';
//   const collector = new MetricsCollector(store);
//   collector.start();  // begins periodic collection
//   collector.stop();   // cleanup
//
// All metric names are prefixed with `dex_` per convention.
// ---------------------------------------------------------------------------

export interface StoreMetricsSource {
  getState(): {
    metrics: {
      events_ignored: number;
      gaps_detected: number;
      buffer_size_by_stream: Map<string, number>;
      last_seq_by_stream: Map<string, string>;
    };
  };
}

export interface CollectedMetrics {
  /** Sequence numbers per stream — string-encoded */
  last_seq_by_stream: Record<string, string>;
  /** Total buffered events across all streams */
  buffer_size_total: number;
  /** Total duplicate events ignored */
  events_ignored_total: number;
  /** Total sequence gaps detected */
  gaps_detected_total: number;
  /** Number of active streams (proxy for connected clients) */
  connected_clients: number;
  /** Uptime in seconds */
  uptime_seconds: number;
}

export interface MetricsCollectorConfig {
  /** Collection interval in ms. Default: 5000 */
  intervalMs: number;
  /** Observability server endpoint. Default: http://localhost:9091 */
  observabilityEndpoint: string;
  /** Whether to push metrics to the observability server. Default: true */
  pushEnabled: boolean;
}

const DEFAULT_CONFIG: MetricsCollectorConfig = {
  intervalMs: 5_000,
  observabilityEndpoint: "http://localhost:9091",
  pushEnabled: true,
};

export class MetricsCollector {
  private readonly config: MetricsCollectorConfig;
  private readonly store: StoreMetricsSource;
  private timer: ReturnType<typeof setInterval> | null = null;
  private readonly startTime: number;
  private lastCollected: CollectedMetrics | null = null;

  constructor(
    store: StoreMetricsSource,
    config?: Partial<MetricsCollectorConfig>,
  ) {
    this.store = store;
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.startTime = Date.now();
  }

  /**
   * Collect metrics from the store immediately.
   */
  collect(): CollectedMetrics {
    const state = this.store.getState();
    const m = state.metrics;

    const lastSeq: Record<string, string> = {};
    for (const [k, v] of m.last_seq_by_stream) {
      lastSeq[k] = v;
    }

    let totalBuffer = 0;
    for (const [, v] of m.buffer_size_by_stream) {
      totalBuffer += v;
    }

    const collected: CollectedMetrics = {
      last_seq_by_stream: lastSeq,
      buffer_size_total: totalBuffer,
      events_ignored_total: m.events_ignored,
      gaps_detected_total: m.gaps_detected,
      connected_clients: m.last_seq_by_stream.size,
      uptime_seconds: Math.floor((Date.now() - this.startTime) / 1000),
    };

    this.lastCollected = collected;
    return collected;
  }

  /**
   * Push collected metrics to the observability server.
   * Fire-and-forget — never throws.
   */
  private async push(metrics: CollectedMetrics): Promise<void> {
    if (!this.config.pushEnabled) return;

    try {
      if (typeof fetch !== "undefined") {
        await fetch(
          `${this.config.observabilityEndpoint}/metrics/update`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(metrics),
          },
        ).catch(() => {
          // silently ignore push failures
        });
      }
    } catch {
      // silently ignore
    }
  }

  /**
   * Start periodic metric collection and push.
   */
  start(): void {
    if (this.timer) return;
    this.timer = setInterval(() => {
      const m = this.collect();
      void this.push(m);
    }, this.config.intervalMs);

    // Immediate first collection
    const m = this.collect();
    void this.push(m);
  }

  /**
   * Stop periodic collection.
   */
  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  /**
   * Get the last collected metrics snapshot.
   */
  getLastCollected(): CollectedMetrics | null {
    return this.lastCollected;
  }

  /**
   * Render metrics in Prometheus text exposition format.
   * Can be used if serving /metrics directly from the app.
   */
  renderPrometheus(): string {
    const m = this.lastCollected ?? this.collect();
    const lines: string[] = [];

    lines.push("# HELP dex_ws_last_seq Last WebSocket sequence number per symbol");
    lines.push("# TYPE dex_ws_last_seq gauge");
    for (const [stream, seq] of Object.entries(m.last_seq_by_stream)) {
      const safe = stream.replace(/[^a-zA-Z0-9_]/g, "_");
      lines.push(`dex_ws_last_seq{symbol="${safe}"} ${seq}`);
    }

    lines.push("");
    lines.push("# HELP dex_buffer_size_total Total buffered events across all streams");
    lines.push("# TYPE dex_buffer_size_total gauge");
    lines.push(`dex_buffer_size_total ${m.buffer_size_total}`);

    lines.push("");
    lines.push("# HELP dex_events_ignored_total Total duplicate events ignored");
    lines.push("# TYPE dex_events_ignored_total counter");
    lines.push(`dex_events_ignored_total ${m.events_ignored_total}`);

    lines.push("");
    lines.push("# HELP dex_gaps_detected_total Total sequence gaps detected");
    lines.push("# TYPE dex_gaps_detected_total counter");
    lines.push(`dex_gaps_detected_total ${m.gaps_detected_total}`);

    lines.push("");
    lines.push("# HELP dex_connected_clients Number of active data streams");
    lines.push("# TYPE dex_connected_clients gauge");
    lines.push(`dex_connected_clients ${m.connected_clients}`);

    lines.push("");
    lines.push("# HELP dex_uptime_seconds Time since metrics collector started");
    lines.push("# TYPE dex_uptime_seconds gauge");
    lines.push(`dex_uptime_seconds ${m.uptime_seconds}`);

    lines.push("");
    return lines.join("\n") + "\n";
  }
}
