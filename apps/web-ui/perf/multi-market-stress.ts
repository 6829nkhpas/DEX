#!/usr/bin/env node
// ---------------------------------------------------------------------------
// multi-market-stress.ts — Phase 15 multi-market stress matrix
// ---------------------------------------------------------------------------
//
// Matrix:
//   1 symbol  @ 500 msg/sec
//   10 symbols @ 100 msg/sec
//   25 symbols @ 50 msg/sec
//   50 symbols @ 25 msg/sec
//
// Measures: latency, memory growth, buffer size, CPU %
//
// Output: perf/multi-market-results.json
// ---------------------------------------------------------------------------

import { DexStateStore } from "../src/state/store";
import type { BaseEvent } from "../../../types/generated-types";
import * as fs from "fs";
import * as path from "path";

// ---------------------------------------------------------------------------
// Symbol generation
// ---------------------------------------------------------------------------

const ALL_SYMBOLS = [
  "BTC", "ETH", "SOL", "AVAX", "DOGE", "LINK", "UNI", "AAVE", "MKR", "CRV",
  "MATIC", "DOT", "ADA", "XRP", "LTC", "ATOM", "FTM", "NEAR", "APT", "ARB",
  "OP", "SNX", "COMP", "SUSHI", "YFI", "BAL", "1INCH", "PERP", "DYDX", "GMX",
  "INJ", "SEI", "SUI", "TIA", "JUP", "PYTH", "WIF", "BONK", "JTO", "RNDR",
  "FET", "AGIX", "OCEAN", "GRT", "FIL", "AR", "THETA", "HNT", "IOTX", "AKT",
];

function generateSymbols(n: number): string[] {
  return ALL_SYMBOLS.slice(0, n).map((b) => `${b}/USDT`);
}

// ---------------------------------------------------------------------------
// Event generation — per-domain sequence counters
// ---------------------------------------------------------------------------

let seqCounters: Record<string, number> = {};

function getSeq(domain: string): string {
  if (!seqCounters[domain]) seqCounters[domain] = 0;
  return String(++seqCounters[domain]);
}

function nowNanos(): string {
  return String(Date.now() * 1_000_000);
}

function makeSnapshot(symbol: string): BaseEvent<unknown> {
  const domain = `market_data::${symbol}`;
  const bids: [string, string][] = [];
  const asks: [string, string][] = [];
  for (let i = 0; i < 25; i++) {
    bids.push([(50000 - i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
    asks.push([(50000.5 + i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
  }
  const seq = getSeq(domain);
  return {
    event_id: `snap-${domain}-${seq}`,
    event_type: "snapshot",
    sequence: seq,
    timestamp: nowNanos(),
    source: "market_data",
    payload: { symbol, bids, asks },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeDelta(symbol: string): BaseEvent<unknown> {
  const roll = Math.random();

  if (roll < 0.55) {
    // Orderbook delta
    const domain = `market_data::${symbol}`;
    const seq = getSeq(domain);
    const isBid = Math.random() > 0.5;
    const price = (50000 + (isBid ? -1 : 1) * Math.random() * 50).toFixed(2);
    const qty = Math.random() > 0.85 ? "0" : (Math.random() * 5).toFixed(4);
    return {
      event_id: `delta-${domain}-${seq}`,
      event_type: "delta",
      sequence: seq,
      timestamp: nowNanos(),
      source: "market_data",
      payload: { symbol, [isBid ? "bids" : "asks"]: [[price, qty]] },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
  }

  if (roll < 0.8) {
    // Trade
    const domain = `trades::${symbol}`;
    const seq = getSeq(domain);
    return {
      event_id: `trade-${domain}-${seq}`,
      event_type: "delta",
      sequence: seq,
      timestamp: nowNanos(),
      source: "trades",
      payload: {
        symbol,
        price: (50000 + Math.random() * 100).toFixed(2),
        quantity: (Math.random() * 2).toFixed(4),
        side: Math.random() > 0.5 ? "BUY" : "SELL",
      },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
  }

  // Ticker delta
  const domain = `market_data::${symbol}`;
  const seq = getSeq(domain);
  return {
    event_id: `ticker-${domain}-${seq}`,
    event_type: "delta",
    sequence: seq,
    timestamp: nowNanos(),
    source: "market_data",
    payload: {
      symbol,
      last_price: (50000 + Math.random() * 100).toFixed(2),
      mark_price: (50000 + Math.random() * 100).toFixed(2),
      volume_24h: (Math.random() * 10000).toFixed(2),
      high_24h: (51000 + Math.random() * 500).toFixed(2),
      low_24h: (49000 - Math.random() * 500).toFixed(2),
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

// ---------------------------------------------------------------------------
// Percentile
// ---------------------------------------------------------------------------

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

// ---------------------------------------------------------------------------
// Scenario runner
// ---------------------------------------------------------------------------

interface ScenarioConfig {
  symbols: string[];
  ratePerSymbol: number;
  duration: number;
}

interface ScenarioResult {
  label: string;
  symbols: number;
  rate_per_symbol: number;
  total_rate: number;
  duration_sec: number;
  total_events: number;
  actual_rate: number;
  dispatch_latency_ms: {
    median: number;
    p95: number;
    p99: number;
    max: number;
  };
  heap_start_mb: number;
  heap_end_mb: number;
  heap_growth_mb: number;
  heap_growth_pct: number;
  heap_per_symbol_mb: number;
  events_ignored: number;
  gaps_detected: number;
  max_buffer_size: number;
  max_buffer_pct: number;
  cpu_time_user_ms: number;
  cpu_time_system_ms: number;
  stable: boolean;
  notes: string[];
}

function runScenario(config: ScenarioConfig): ScenarioResult {
  const MAX_BUFFER_SIZE = 10_000;
  const store = new DexStateStore();
  const latencies: number[] = [];
  const notes: string[] = [];

  seqCounters = {};

  const label = `${config.symbols.length} sym @ ${config.ratePerSymbol}/s`;
  console.log(`\n--- Running: ${label} for ${config.duration}s ---`);

  // Prime with snapshots + initial trade per symbol
  for (const sym of config.symbols) {
    store.dispatch(makeSnapshot(sym));
    const trDomain = `trades::${sym}`;
    const trSeq = getSeq(trDomain);
    store.dispatch({
      event_id: `init-trade-${sym}`,
      event_type: "delta",
      sequence: trSeq,
      timestamp: nowNanos(),
      source: "trades",
      payload: { symbol: sym, price: "50000.00", quantity: "0.0001", side: "BUY" },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    } as BaseEvent<unknown>);
  }

  const totalRate = config.ratePerSymbol * config.symbols.length;
  const totalEvents = totalRate * config.duration;

  // Capture CPU baseline
  const cpuStart = process.cpuUsage();
  const heapStart = process.memoryUsage().heapUsed / (1024 * 1024);
  const startTime = performance.now();

  // Heap samples every 2s
  const heapSamples: number[] = [heapStart];
  let lastSample = startTime;

  let eventsDispatched = 0;
  let symIdx = 0;

  while (eventsDispatched < totalEvents) {
    const sym = config.symbols[symIdx % config.symbols.length];
    symIdx++;

    const event = makeDelta(sym);
    const t0 = performance.now();
    store.dispatch(event);
    const t1 = performance.now();

    latencies.push(t1 - t0);
    eventsDispatched++;

    // Sample heap periodically
    if (t1 - lastSample > 2000) {
      heapSamples.push(process.memoryUsage().heapUsed / (1024 * 1024));
      lastSample = t1;
    }

    // Progress
    if (eventsDispatched % 10000 === 0) {
      const elapsed = (performance.now() - startTime) / 1000;
      const rate = eventsDispatched / elapsed;
      process.stdout.write(`\r  [${label}] ${eventsDispatched}/${totalEvents} events, ${rate.toFixed(0)}/s   `);
    }
  }

  const endTime = performance.now();
  const cpuEnd = process.cpuUsage(cpuStart);
  const heapEnd = process.memoryUsage().heapUsed / (1024 * 1024);
  heapSamples.push(heapEnd);

  const totalTime = (endTime - startTime) / 1000;
  const actualRate = eventsDispatched / totalTime;

  latencies.sort((a, b) => a - b);

  // Metrics from store
  const metrics = store.getState().metrics;
  let maxBuf = 0;
  for (const [, v] of metrics.buffer_size_by_stream) {
    if (v > maxBuf) maxBuf = v;
  }
  const maxBufferPct = (maxBuf / MAX_BUFFER_SIZE) * 100;
  const heapGrowthMb = heapEnd - heapStart;
  const heapGrowthPct = heapStart > 0 ? ((heapEnd - heapStart) / heapStart) * 100 : 0;
  const heapPerSymbol = config.symbols.length > 0 ? heapGrowthMb / config.symbols.length : 0;

  // Stability criteria
  const medianLat = percentile(latencies, 50);
  const p95Lat = percentile(latencies, 95);
  const stable = medianLat < 1 && p95Lat < 5 && maxBufferPct < 1;

  if (medianLat >= 1) notes.push(`median latency ${medianLat.toFixed(3)}ms exceeds 1ms target`);
  if (p95Lat >= 5) notes.push(`p95 latency ${p95Lat.toFixed(3)}ms exceeds 5ms target`);
  if (maxBufferPct >= 1) notes.push(`buffer utilization ${maxBufferPct.toFixed(2)}% exceeds 1% cap`);
  if (metrics.gaps_detected > 0) notes.push(`${metrics.gaps_detected} gaps detected`);

  const result: ScenarioResult = {
    label,
    symbols: config.symbols.length,
    rate_per_symbol: config.ratePerSymbol,
    total_rate: totalRate,
    duration_sec: config.duration,
    total_events: eventsDispatched,
    actual_rate: parseFloat(actualRate.toFixed(2)),
    dispatch_latency_ms: {
      median: parseFloat(medianLat.toFixed(4)),
      p95: parseFloat(p95Lat.toFixed(4)),
      p99: parseFloat(percentile(latencies, 99).toFixed(4)),
      max: parseFloat(Math.max(...latencies).toFixed(4)),
    },
    heap_start_mb: parseFloat(heapStart.toFixed(2)),
    heap_end_mb: parseFloat(heapEnd.toFixed(2)),
    heap_growth_mb: parseFloat(heapGrowthMb.toFixed(2)),
    heap_growth_pct: parseFloat(heapGrowthPct.toFixed(2)),
    heap_per_symbol_mb: parseFloat(heapPerSymbol.toFixed(4)),
    events_ignored: metrics.events_ignored,
    gaps_detected: metrics.gaps_detected,
    max_buffer_size: maxBuf,
    max_buffer_pct: parseFloat(maxBufferPct.toFixed(4)),
    cpu_time_user_ms: Math.round(cpuEnd.user / 1000),
    cpu_time_system_ms: Math.round(cpuEnd.system / 1000),
    stable,
    notes,
  };

  console.log(`\n  Result: median=${result.dispatch_latency_ms.median}ms p95=${result.dispatch_latency_ms.p95}ms ` +
    `heap_growth=${result.heap_growth_pct}% stable=${result.stable}`);

  return result;
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const BENCH_DURATION = 10; // 10s per scenario

  const scenarios: ScenarioConfig[] = [
    { symbols: generateSymbols(1), ratePerSymbol: 500, duration: BENCH_DURATION },
    { symbols: generateSymbols(10), ratePerSymbol: 100, duration: BENCH_DURATION },
    { symbols: generateSymbols(25), ratePerSymbol: 50, duration: BENCH_DURATION },
    { symbols: generateSymbols(50), ratePerSymbol: 25, duration: BENCH_DURATION },
  ];

  const results: ScenarioResult[] = [];

  console.log("=== PHASE 15 — MULTI-MARKET STRESS MATRIX ===");
  console.log(`Running ${scenarios.length} scenarios, ${BENCH_DURATION}s each\n`);

  for (const scenario of scenarios) {
    if (global.gc) global.gc();
    const result = runScenario(scenario);
    results.push(result);
  }

  // Summary
  const stableResults = results.filter((r) => r.stable);
  const maxSafe = stableResults.reduce((max, r) => (r.total_rate > max ? r.total_rate : max), 0);
  const maxSymbols = stableResults.reduce((max, r) => (r.symbols > max ? r.symbols : max), 0);

  const output = {
    phase: 15,
    timestamp: new Date().toISOString(),
    matrix: "multi-market-stress",
    total_scenarios: scenarios.length,
    scenarios_passed: stableResults.length,
    max_sustained_safe_throughput: maxSafe,
    max_stable_symbol_count: maxSymbols,
    scaling_analysis: {
      memory_per_symbol_mb: results.map((r) => ({
        symbols: r.symbols,
        heap_per_symbol_mb: r.heap_per_symbol_mb,
      })),
      linear_scaling: results.every((r) => r.heap_per_symbol_mb < 5)
        ? "confirmed"
        : "degraded at high symbol counts",
    },
    results,
  };

  console.log("\n\n=== MULTI-MARKET STRESS MATRIX SUMMARY ===");
  console.log(`  Scenarios:      ${scenarios.length}`);
  console.log(`  Passed:         ${stableResults.length}`);
  console.log(`  Max safe rate:  ${maxSafe} msg/sec`);
  console.log(`  Max symbols:    ${maxSymbols}`);
  console.log("");

  for (const r of results) {
    console.log(
      `  ${r.label.padEnd(24)} | ` +
      `rate=${String(r.actual_rate.toFixed(0)).padStart(8)}/s | ` +
      `median=${r.dispatch_latency_ms.median.toFixed(3).padStart(8)}ms | ` +
      `p95=${r.dispatch_latency_ms.p95.toFixed(3).padStart(8)}ms | ` +
      `heap_growth=${r.heap_growth_pct.toFixed(1).padStart(6)}% | ` +
      `heap/sym=${r.heap_per_symbol_mb.toFixed(2).padStart(6)}MB | ` +
      `buf=${String(r.max_buffer_size).padStart(5)} | ` +
      `${r.stable ? "PASS ✓" : "FAIL ✗"}`
    );
  }
  console.log("==========================================");

  const outPath = path.resolve(__dirname, "multi-market-results.json");
  fs.writeFileSync(outPath, JSON.stringify(output, null, 2));
  console.log(`\nResults written to ${outPath}`);
}

main().catch((err) => {
  console.error("[multi-market-stress] Fatal error:", err);
  process.exit(1);
});
