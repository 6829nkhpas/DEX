#!/usr/bin/env node
// ---------------------------------------------------------------------------
// stress-matrix.ts — Run performance matrix across configurations
// ---------------------------------------------------------------------------
//
// Matrix:
//   1 symbol  @ [100, 200, 500] msg/sec
//   5 symbols @ [100, 200] msg/sec
//   20 symbols @ 100 msg/sec (if feasible)
//
// Output: perf/results-matrix.json
// ---------------------------------------------------------------------------

import { DexStateStore } from "../src/state/store";
import type { BaseEvent } from "../../../types/generated-types";
import * as fs from "fs";
import * as path from "path";

// ---------------------------------------------------------------------------
// Event generation
// ---------------------------------------------------------------------------

let seqCounters: Record<string, number> = {};

function getSeq(domain: string): string {
  if (!seqCounters[domain]) seqCounters[domain] = 0;
  return String(++seqCounters[domain]);
}

function nowNanos(): string {
  return String(Date.now() * 1_000_000);
}

function makeSnapshot(symbol: string, domain: string): BaseEvent<unknown> {
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

function makeDelta(symbol: string, _domain: string): BaseEvent<unknown> {
  const roll = Math.random();

  if (roll < 0.6) {
    // Orderbook delta — market_data domain
    const mdDomain = `market_data::${symbol}`;
    const seq = getSeq(mdDomain);
    const isBid = Math.random() > 0.5;
    const price = (50000 + (isBid ? -1 : 1) * Math.random() * 50).toFixed(2);
    const qty = Math.random() > 0.85 ? "0" : (Math.random() * 5).toFixed(4);
    return {
      event_id: `delta-${mdDomain}-${seq}`,
      event_type: "delta",
      sequence: seq,
      timestamp: nowNanos(),
      source: "market_data",
      payload: { symbol, [isBid ? "bids" : "asks"]: [[price, qty]] },
      metadata: { version: "1.0", correlation_id: "", causation_id: "" },
    };
  }

  if (roll < 0.85) {
    // Trade — trades domain (separate sequence)
    const trDomain = `trades::${symbol}`;
    const seq = getSeq(trDomain);
    return {
      event_id: `trade-${trDomain}-${seq}`,
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

  // Ticker delta — market_data domain
  const mdDomain = `market_data::${symbol}`;
  const seq = getSeq(mdDomain);
  return {
    event_id: `ticker-${mdDomain}-${seq}`,
    event_type: "delta",
    sequence: seq,
    timestamp: nowNanos(),
    source: "market_data",
    payload: {
      symbol,
      last_price: (50000 + Math.random() * 100).toFixed(2),
      mark_price: (50000 + Math.random() * 100).toFixed(2),
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
// Single scenario run
// ---------------------------------------------------------------------------

interface ScenarioConfig {
  symbols: string[];
  ratePerSymbol: number;
  duration: number; // seconds
}

interface ScenarioResult {
  label: string;
  symbols: number;
  rate_per_symbol: number;
  total_rate: number;
  duration: number;
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
  heap_growth_pct: number;
  events_ignored: number;
  gaps_detected: number;
  max_buffer_pct: number;
  stable: boolean;
}

function runScenario(config: ScenarioConfig): ScenarioResult {
  const MAX_BUFFER_SIZE = 10_000;
  const store = new DexStateStore();
  const latencies: number[] = [];

  // Reset counters
  seqCounters = {};

  const label = `${config.symbols.length}sym @ ${config.ratePerSymbol}/s`;
  console.log(`\n--- Running: ${label} for ${config.duration}s ---`);

  // Domain keys map: for multi-symbol, each symbol gets its own sequence domain
  // We use "market_data::<symbol>" as domains
  const domains = new Map<string, string>();
  for (const sym of config.symbols) {
    domains.set(sym, `market_data::${sym}`);
  }

  // Send initial snapshots and prime trades domains
  for (const [sym, domain] of domains) {
    const snap = makeSnapshot(sym, domain);
    store.dispatch(snap);
    // Prime trades domain
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

  const heapStart = process.memoryUsage().heapUsed / (1024 * 1024);
  const totalRate = config.ratePerSymbol * config.symbols.length;
  const totalEvents = totalRate * config.duration;

  const startTime = performance.now();
  let eventsDispatched = 0;
  let symIdx = 0;

  // Synchronous tight loop for deterministic measurement
  while (eventsDispatched < totalEvents) {
    const sym = config.symbols[symIdx % config.symbols.length];
    const domain = domains.get(sym)!;
    symIdx++;

    const event = makeDelta(sym, domain);
    const t0 = performance.now();
    store.dispatch(event);
    const t1 = performance.now();

    latencies.push(t1 - t0);
    eventsDispatched++;

    // Print progress every 10000 events
    if (eventsDispatched % 10000 === 0) {
      const elapsed = (performance.now() - startTime) / 1000;
      const rate = eventsDispatched / elapsed;
      process.stdout.write(`\r  [${label}] ${eventsDispatched}/${totalEvents} events, ${rate.toFixed(0)}/s   `);
    }
  }

  const endTime = performance.now();
  const heapEnd = process.memoryUsage().heapUsed / (1024 * 1024);
  const totalTime = (endTime - startTime) / 1000;
  const actualRate = eventsDispatched / totalTime;

  latencies.sort((a, b) => a - b);

  const metrics = store.getState().metrics;
  let maxBuf = 0;
  for (const [, v] of metrics.buffer_size_by_stream) {
    if (v > maxBuf) maxBuf = v;
  }
  const maxBufferPct = (maxBuf / MAX_BUFFER_SIZE) * 100;
  const heapGrowth = ((heapEnd - heapStart) / heapStart) * 100;

  const result: ScenarioResult = {
    label,
    symbols: config.symbols.length,
    rate_per_symbol: config.ratePerSymbol,
    total_rate: totalRate,
    duration: config.duration,
    total_events: eventsDispatched,
    actual_rate: parseFloat(actualRate.toFixed(2)),
    dispatch_latency_ms: {
      median: parseFloat(percentile(latencies, 50).toFixed(4)),
      p95: parseFloat(percentile(latencies, 95).toFixed(4)),
      p99: parseFloat(percentile(latencies, 99).toFixed(4)),
      max: parseFloat(Math.max(...latencies).toFixed(4)),
    },
    heap_start_mb: parseFloat(heapStart.toFixed(2)),
    heap_end_mb: parseFloat(heapEnd.toFixed(2)),
    heap_growth_pct: parseFloat(heapGrowth.toFixed(2)),
    events_ignored: metrics.events_ignored,
    gaps_detected: metrics.gaps_detected,
    max_buffer_pct: parseFloat(maxBufferPct.toFixed(4)),
    stable: percentile(latencies, 50) < 100 && percentile(latencies, 95) < 300 && heapGrowth < 10,
  };

  console.log(`\n  Result: median=${result.dispatch_latency_ms.median}ms p95=${result.dispatch_latency_ms.p95}ms heap_growth=${result.heap_growth_pct}% stable=${result.stable}`);

  return result;
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------

function generateSymbols(n: number): string[] {
  const bases = ["BTC", "ETH", "SOL", "AVAX", "DOGE", "LINK", "UNI", "AAVE", "MKR", "CRV",
    "MATIC", "DOT", "ADA", "XRP", "LTC", "ATOM", "FTM", "NEAR", "APT", "ARB"];
  return bases.slice(0, n).map((b) => `${b}/USDT`);
}

async function main(): Promise<void> {
  const BENCH_DURATION = 10; // 10s per scenario for the matrix

  const scenarios: ScenarioConfig[] = [
    // 1 symbol @ [100, 200, 500]
    { symbols: generateSymbols(1), ratePerSymbol: 100, duration: BENCH_DURATION },
    { symbols: generateSymbols(1), ratePerSymbol: 200, duration: BENCH_DURATION },
    { symbols: generateSymbols(1), ratePerSymbol: 500, duration: BENCH_DURATION },
    // 5 symbols @ [100, 200]
    { symbols: generateSymbols(5), ratePerSymbol: 100, duration: BENCH_DURATION },
    { symbols: generateSymbols(5), ratePerSymbol: 200, duration: BENCH_DURATION },
    // 20 symbols @ 100
    { symbols: generateSymbols(20), ratePerSymbol: 100, duration: BENCH_DURATION },
  ];

  const results: ScenarioResult[] = [];

  console.log("=== STRESS MATRIX ===");
  console.log(`Running ${scenarios.length} scenarios, ${BENCH_DURATION}s each\n`);

  for (const scenario of scenarios) {
    // Force GC between scenarios if available
    if (global.gc) global.gc();
    const result = runScenario(scenario);
    results.push(result);
  }

  // Find max sustained safe throughput
  const stableResults = results.filter((r) => r.stable);
  const maxSafe = stableResults.reduce((max, r) => (r.total_rate > max ? r.total_rate : max), 0);

  const matrixOutput = {
    timestamp: new Date().toISOString(),
    total_scenarios: scenarios.length,
    scenarios_passed: stableResults.length,
    max_sustained_safe_throughput: maxSafe,
    results,
  };

  console.log("\n\n=== STRESS MATRIX SUMMARY ===");
  console.log(`  Scenarios:  ${scenarios.length}`);
  console.log(`  Passed:     ${stableResults.length}`);
  console.log(`  Max safe:   ${maxSafe} msg/sec`);
  console.log("");

  for (const r of results) {
    console.log(
      `  ${r.label.padEnd(20)} | ` +
      `rate=${r.actual_rate.toFixed(0).padStart(8)}/s | ` +
      `median=${r.dispatch_latency_ms.median.toFixed(3).padStart(8)}ms | ` +
      `p95=${r.dispatch_latency_ms.p95.toFixed(3).padStart(8)}ms | ` +
      `heap=${r.heap_growth_pct.toFixed(1).padStart(6)}% | ` +
      `${r.stable ? "PASS ✓" : "FAIL ✗"}`
    );
  }
  console.log("=============================");

  const outPath = path.resolve(__dirname, "results-matrix.json");
  fs.writeFileSync(outPath, JSON.stringify(matrixOutput, null, 2));
  console.log(`\nResults written to ${outPath}`);
}

main().catch((err) => {
  console.error("[stress-matrix] Fatal error:", err);
  process.exit(1);
});
