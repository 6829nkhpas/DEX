// ---------------------------------------------------------------------------
// multi/ barrel export — Phase 15 multi-market infrastructure
// ---------------------------------------------------------------------------

export { SubscriptionOrchestrator } from "./SubscriptionOrchestrator";
export type {
  SubscriptionTier,
  SymbolSubscription,
  SubscriptionTransport,
  OrchestratorConfig,
  OrchestratorSnapshot,
} from "./SubscriptionOrchestrator";

export { AggregatedFeedManager } from "./AggregatedFeedManager";
export type {
  AggregationConfig,
  AggregationStats,
} from "./AggregatedFeedManager";

export { MarketGrid } from "./MarketGrid";
export type { MarketGridProps } from "./MarketGrid";

export { MarketTile } from "./MarketTile";
export type { MarketTileProps } from "./MarketTile";
