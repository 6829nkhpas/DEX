// ---------------------------------------------------------------------------
// Performance KPI targets — Phase 14
// ---------------------------------------------------------------------------

export const PERF_TARGETS = {
  single_symbol: {
    rate: 100,              // msg/sec sustained
    median_latency_ms: 100, // dispatch median <100ms
    p95_latency_ms: 300,    // dispatch p95 <300ms
  },
  multi_symbol: {
    symbols: 5,
    rate_per_symbol: 100,   // total 500 msg/sec
  },
  memory: {
    max_heap_growth_pct: 10, // <10% over 5 minutes
  },
  buffer: {
    max_buffer_pct: 1,       // <1% of MAX_BUFFER_SIZE (10,000)
  },
} as const;
