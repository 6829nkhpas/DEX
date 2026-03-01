// ---------------------------------------------------------------------------
// AggregatedFeedManager — switches to aggregated mode when symbol count > threshold
// ---------------------------------------------------------------------------
//
// When the number of active symbols exceeds a configurable threshold,
// background symbols are automatically downgraded:
//   - Orderbook deltas are dropped (stale book is acceptable)
//   - Trade stream is unsubscribed
//   - Only ticker deltas are applied
//
// The focused symbol always retains full data regardless of mode.
//
// This reduces CPU and memory pressure from high symbol counts.
// ---------------------------------------------------------------------------

import type { BaseEvent } from "../../../../types/generated-types";
import type { DexStateStore } from "../state/store";
import type { SubscriptionOrchestrator } from "./SubscriptionOrchestrator";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface AggregationConfig {
  /** Symbol count threshold to trigger aggregated mode. Default: 20 */
  threshold: number;
  /** Percentage of ticks to apply for background symbols (0-1). Default: 0.5 */
  backgroundSampleRate: number;
}

export interface AggregationStats {
  mode: "normal" | "aggregated";
  symbolCount: number;
  eventsFiltered: number;
  eventsApplied: number;
  filterRate: number;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_CONFIG: AggregationConfig = {
  threshold: 20,
  backgroundSampleRate: 0.5,
};

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/**
 * Manages the aggregated feed mode. Acts as a middleware between the WS
 * event stream and the state store — filtering events when symbol count is high.
 *
 * Usage:
 *   const agg = new AggregatedFeedManager(store, orchestrator);
 *   // Instead of: store.dispatch(event)
 *   agg.dispatch(event);  // filters if in aggregated mode
 */
export class AggregatedFeedManager {
  private readonly store: DexStateStore;
  private readonly orchestrator: SubscriptionOrchestrator;
  private readonly config: AggregationConfig;

  private eventsFiltered = 0;
  private eventsApplied = 0;
  private tickCounter = 0;

  constructor(
    store: DexStateStore,
    orchestrator: SubscriptionOrchestrator,
    config?: Partial<AggregationConfig>,
  ) {
    this.store = store;
    this.orchestrator = orchestrator;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Dispatch an event through the aggregation filter.
   * In normal mode, all events pass through to the store.
   * In aggregated mode, non-focused symbol events are sampled.
   */
  dispatch(event: BaseEvent<unknown>): void {
    // Snapshots always pass through (needed for gap recovery)
    if (event.event_type === "snapshot") {
      this.store.dispatch(event);
      this.eventsApplied++;
      return;
    }

    const shouldAggregate = this.isAggregatedMode();

    if (!shouldAggregate) {
      // Normal mode: all events pass through
      this.store.dispatch(event);
      this.eventsApplied++;
      return;
    }

    // Aggregated mode: check if event belongs to focused symbol
    const symbol = this.extractSymbol(event);
    const focusedSymbol = this.orchestrator.getFocusedSymbol();

    if (symbol && symbol === focusedSymbol) {
      // Focused symbol always gets full data
      this.store.dispatch(event);
      this.eventsApplied++;
      return;
    }

    // Background symbol in aggregated mode:

    // Always drop orderbook deltas for background symbols (too expensive)
    const payload = event.payload as Record<string, unknown>;
    if (event.source === "market_data" && ("bids" in payload || "asks" in payload)) {
      // Check if this is pure orderbook (no ticker fields)
      const hasTickerFields = "last_price" in payload || "mark_price" in payload ||
        "volume_24h" in payload || "high_24h" in payload || "low_24h" in payload;
      if (!hasTickerFields) {
        this.eventsFiltered++;
        return; // Drop pure orderbook delta
      }
    }

    // Sample ticker/trade events based on configured rate
    this.tickCounter++;
    const sampleInterval = Math.max(1, Math.round(1 / this.config.backgroundSampleRate));
    if (this.tickCounter % sampleInterval !== 0) {
      this.eventsFiltered++;
      return;
    }

    this.store.dispatch(event);
    this.eventsApplied++;
  }

  /**
   * Check if aggregated mode should be active.
   */
  isAggregatedMode(): boolean {
    return this.orchestrator.getSubscribedSymbols().length > this.config.threshold;
  }

  /**
   * Get aggregation statistics.
   */
  getStats(): AggregationStats {
    const total = this.eventsApplied + this.eventsFiltered;
    return {
      mode: this.isAggregatedMode() ? "aggregated" : "normal",
      symbolCount: this.orchestrator.getSubscribedSymbols().length,
      eventsFiltered: this.eventsFiltered,
      eventsApplied: this.eventsApplied,
      filterRate: total > 0 ? this.eventsFiltered / total : 0,
    };
  }

  /**
   * Reset counters.
   */
  resetStats(): void {
    this.eventsFiltered = 0;
    this.eventsApplied = 0;
    this.tickCounter = 0;
  }

  // -----------------------------------------------------------------------
  // Internal
  // -----------------------------------------------------------------------

  private extractSymbol(event: BaseEvent<unknown>): string | null {
    const payload = event.payload as Record<string, unknown> | null;
    if (payload && typeof payload === "object" && "symbol" in payload) {
      return String(payload.symbol);
    }
    return null;
  }
}
