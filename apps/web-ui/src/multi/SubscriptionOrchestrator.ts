// ---------------------------------------------------------------------------
// SubscriptionOrchestrator — manages multi-symbol subscription lifecycle
// ---------------------------------------------------------------------------
//
// Responsibilities:
//   1. Track active symbol subscriptions with reference counting
//   2. Coalesce duplicate subscription requests
//   3. Prioritize the focused (foreground) symbol with full data
//   4. Degrade background symbols to ticker-only mode
//   5. Auto-unsubscribe when no listeners remain
//   6. Aggregation mode when symbol count exceeds threshold
// ---------------------------------------------------------------------------

import type { WsChannel } from "../ws/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Subscription detail level for a symbol. */
export type SubscriptionTier = "full" | "ticker-only";

/** Internal tracking for a single symbol subscription. */
export interface SymbolSubscription {
  symbol: string;
  tier: SubscriptionTier;
  listenerCount: number;
  channels: Set<WsChannel>;
  subscribedAt: number; // monotonic timestamp
}

/** Callback interface for the orchestrator to communicate with the WS layer. */
export interface SubscriptionTransport {
  subscribe(channel: WsChannel, params: Record<string, string>): void;
  unsubscribe(channel: WsChannel, params: Record<string, string>): void;
}

/** Configuration for the orchestrator. */
export interface OrchestratorConfig {
  /** Max symbols before switching to aggregated mode. Default: 20 */
  aggregationThreshold: number;
  /** Channels active in full mode. Default: ["market_data", "trades"] */
  fullChannels: WsChannel[];
  /** Channels active in ticker-only mode. Default: ["market_data"] */
  tickerOnlyChannels: WsChannel[];
}

/** Snapshot of orchestrator state for debugging/monitoring. */
export interface OrchestratorSnapshot {
  focusedSymbol: string | null;
  subscriptions: ReadonlyMap<string, Readonly<SymbolSubscription>>;
  totalListeners: number;
  aggregatedMode: boolean;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const DEFAULT_CONFIG: OrchestratorConfig = {
  aggregationThreshold: 20,
  fullChannels: ["market_data", "trades"],
  tickerOnlyChannels: ["market_data"],
};

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/**
 * Manages multi-symbol WebSocket subscriptions with focus prioritization
 * and automatic lifecycle management.
 *
 * Usage:
 *   const orch = new SubscriptionOrchestrator(transport);
 *   orch.addListener("BTC/USDT");   // subscribes full
 *   orch.setFocus("BTC/USDT");      // promotes to focused
 *   orch.addListener("ETH/USDT");   // subscribes ticker-only (background)
 *   orch.removeListener("ETH/USDT"); // auto-unsubscribes (last listener gone)
 */
export class SubscriptionOrchestrator {
  private readonly subs = new Map<string, SymbolSubscription>();
  private focusedSymbol: string | null = null;
  private aggregatedMode = false;
  private readonly config: OrchestratorConfig;
  private readonly transport: SubscriptionTransport;
  private monotonicClock = 0;

  constructor(transport: SubscriptionTransport, config?: Partial<OrchestratorConfig>) {
    this.transport = transport;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  // -----------------------------------------------------------------------
  // Public API
  // -----------------------------------------------------------------------

  /**
   * Add a listener for a symbol. If the symbol is not yet subscribed,
   * opens the appropriate WS channels. Duplicate calls increment the
   * reference count without re-subscribing.
   */
  addListener(symbol: string): void {
    const existing = this.subs.get(symbol);
    if (existing) {
      existing.listenerCount++;
      return; // Already subscribed, coalesced
    }

    // Determine tier: focused gets full, others get ticker-only
    const tier = this.determineTier(symbol);
    const channels = this.channelsForTier(tier);

    const sub: SymbolSubscription = {
      symbol,
      tier,
      listenerCount: 1,
      channels: new Set(channels),
      subscribedAt: ++this.monotonicClock,
    };

    this.subs.set(symbol, sub);

    // Actually subscribe on transport
    for (const ch of channels) {
      this.transport.subscribe(ch, { symbol });
    }

    // Re-evaluate aggregation threshold
    this.evaluateAggregation();
  }

  /**
   * Remove a listener for a symbol. When the last listener is removed,
   * the symbol is automatically unsubscribed from all channels.
   */
  removeListener(symbol: string): void {
    const sub = this.subs.get(symbol);
    if (!sub) return;

    sub.listenerCount--;

    if (sub.listenerCount <= 0) {
      // Auto-unsubscribe — no listeners remain
      for (const ch of sub.channels) {
        this.transport.unsubscribe(ch, { symbol });
      }
      this.subs.delete(symbol);

      // If this was the focused symbol, clear focus
      if (this.focusedSymbol === symbol) {
        this.focusedSymbol = null;
      }

      this.evaluateAggregation();
    }
  }

  /**
   * Set the focused (foreground) symbol. The focused symbol gets full
   * data channels; all other symbols are degraded to ticker-only.
   */
  setFocus(symbol: string): void {
    if (this.focusedSymbol === symbol) return;

    const previousFocus = this.focusedSymbol;
    this.focusedSymbol = symbol;

    // Demote previous focus to ticker-only (if it still has listeners)
    if (previousFocus) {
      const prevSub = this.subs.get(previousFocus);
      if (prevSub && prevSub.tier === "full") {
        this.changeTier(previousFocus, "ticker-only");
      }
    }

    // Promote new focus to full
    const newSub = this.subs.get(symbol);
    if (newSub && newSub.tier !== "full") {
      this.changeTier(symbol, "full");
    }
  }

  /**
   * Get the current focused symbol.
   */
  getFocusedSymbol(): string | null {
    return this.focusedSymbol;
  }

  /**
   * Check if a symbol currently has an active subscription.
   */
  isSubscribed(symbol: string): boolean {
    return this.subs.has(symbol);
  }

  /**
   * Get the subscription tier for a symbol.
   */
  getTier(symbol: string): SubscriptionTier | null {
    return this.subs.get(symbol)?.tier ?? null;
  }

  /**
   * Get the listener count for a symbol.
   */
  getListenerCount(symbol: string): number {
    return this.subs.get(symbol)?.listenerCount ?? 0;
  }

  /**
   * Whether the orchestrator has activated aggregated mode.
   */
  isAggregatedMode(): boolean {
    return this.aggregatedMode;
  }

  /**
   * Get a read-only snapshot of the current orchestrator state.
   */
  getSnapshot(): OrchestratorSnapshot {
    let totalListeners = 0;
    for (const [, sub] of this.subs) {
      totalListeners += sub.listenerCount;
    }
    return {
      focusedSymbol: this.focusedSymbol,
      subscriptions: this.subs,
      totalListeners,
      aggregatedMode: this.aggregatedMode,
    };
  }

  /**
   * Get all currently subscribed symbols.
   */
  getSubscribedSymbols(): string[] {
    return Array.from(this.subs.keys());
  }

  /**
   * Force re-evaluation of all tiers (e.g., after config change).
   */
  rebalance(): void {
    for (const [symbol] of this.subs) {
      const desiredTier = this.determineTier(symbol);
      const current = this.subs.get(symbol)!;
      if (current.tier !== desiredTier) {
        this.changeTier(symbol, desiredTier);
      }
    }
    this.evaluateAggregation();
  }

  /**
   * Unsubscribe all symbols and reset state.
   */
  dispose(): void {
    for (const [symbol, sub] of this.subs) {
      for (const ch of sub.channels) {
        this.transport.unsubscribe(ch, { symbol });
      }
    }
    this.subs.clear();
    this.focusedSymbol = null;
    this.aggregatedMode = false;
  }

  // -----------------------------------------------------------------------
  // Internal
  // -----------------------------------------------------------------------

  private determineTier(symbol: string): SubscriptionTier {
    // Focused symbol always gets full
    if (this.focusedSymbol === symbol) return "full";
    // In aggregation mode, everything non-focused is ticker-only
    if (this.aggregatedMode) return "ticker-only";
    // If no focus set and this is the first symbol, give full
    if (!this.focusedSymbol && this.subs.size === 0) return "full";
    // Default: background is ticker-only
    return "ticker-only";
  }

  private channelsForTier(tier: SubscriptionTier): WsChannel[] {
    return tier === "full"
      ? [...this.config.fullChannels]
      : [...this.config.tickerOnlyChannels];
  }

  private changeTier(symbol: string, newTier: SubscriptionTier): void {
    const sub = this.subs.get(symbol);
    if (!sub) return;

    const oldChannels = sub.channels;
    const newChannels = new Set(this.channelsForTier(newTier));

    // Unsubscribe channels being removed
    for (const ch of oldChannels) {
      if (!newChannels.has(ch)) {
        this.transport.unsubscribe(ch, { symbol });
      }
    }

    // Subscribe channels being added
    for (const ch of newChannels) {
      if (!oldChannels.has(ch)) {
        this.transport.subscribe(ch, { symbol });
      }
    }

    sub.tier = newTier;
    sub.channels = newChannels;
  }

  private evaluateAggregation(): void {
    const prevMode = this.aggregatedMode;
    this.aggregatedMode = this.subs.size > this.config.aggregationThreshold;

    // If we just entered aggregation mode, demote all non-focused to ticker-only
    if (this.aggregatedMode && !prevMode) {
      for (const [symbol, sub] of this.subs) {
        if (symbol !== this.focusedSymbol && sub.tier === "full") {
          this.changeTier(symbol, "ticker-only");
        }
      }
    }
  }
}
