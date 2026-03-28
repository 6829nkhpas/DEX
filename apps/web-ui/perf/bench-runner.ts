#!/usr/bin/env node
// ---------------------------------------------------------------------------
// bench-runner.ts — Headless performance benchmark for DexStateStore
// ---------------------------------------------------------------------------
//
// Runs a deterministic stress test against the DexStateStore directly
// (no browser required). Measures:
//   - event→store dispatch latency (median, p95, p99)
//   - JS heap usage sampled every 5s
//   - buffer usage
//   - gaps_detected
//   - events_ignored
//
// Output: perf/results.json
//
// CLI:
//   npx tsx perf/bench-runner.ts [--rate 100] [--duration 60] [--symbols BTC/USDT]
// ---------------------------------------------------------------------------

import { DexStateStore } from "../src/state/store";
import type { BaseEvent } from "../../../types/generated-types";
import * as fs from "fs";
import * as path from "path";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

interface BenchConfig {
  rate: number;        // events/sec
  duration: number;    // seconds
  symbols: string[];
  outputPath: string;
}

function parseArgs(): BenchConfig {
  const args = process.argv.slice(2);
  const config: BenchConfig = {
    rate: 100,
    duration: 60,
    symbols: ["BTC/USDT"],
    outputPath: path.resolve(__dirname, "results.json"),
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--rate":
        config.rate = parseInt(args[++i], 10);
        break;
      case "--duration":
        config.duration = parseInt(args[++i], 10);
        break;
      case "--symbols":
        config.symbols = args[++i].split(",").map((s) => s.trim());
        break;
      case "--output":
        config.outputPath = path.resolve(args[++i]);
        break;
    }
  }
  return config;
}

// ---------------------------------------------------------------------------
// Event generation — per-domain sequence counters
// ---------------------------------------------------------------------------
// The store tracks sequences per domain key (source::symbol).
// Each domain must have its own monotonic sequence to avoid gap detection.
// ---------------------------------------------------------------------------

const seqCounters = new Map<string, number>();

function nextSeq(domain: string): string {
  const cur = seqCounters.get(domain) ?? 0;
  const next = cur + 1;
  seqCounters.set(domain, next);
  return String(next);
}

function resetSeqCounters(): void {
  seqCounters.clear();
}

function nowNanos(): string {
  return String(Date.now() * 1_000_000);
}

let uidCounter = 0;
function uid(): string {
  return `bench-${++uidCounter}-${Math.random().toString(36).slice(2, 8)}`;
}

function makeSnapshot(symbol: string): BaseEvent<unknown> {
  const domain = `market_data::${symbol}`;
  const bids: [string, string][] = [];
  const asks: [string, string][] = [];
  const base = 50000;
  for (let i = 0; i < 25; i++) {
    bids.push([(base - i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
    asks.push([(base + 0.5 + i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
  }
  return {
    event_id: uid(),
    event_type: "snapshot",
    sequence: nextSeq(domain),
    timestamp: nowNanos(),
    source: "market_data",
    payload: { symbol, bids, asks },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeTradesSnapshot(symbol: string): BaseEvent<unknown> {
  const domain = `trades::${symbol}`;
  // Trades don't have formal snapshots, but we need to prime the domain to seq=1
  return {
    event_id: uid(),
    event_type: "delta",
    sequence: nextSeq(domain),
    timestamp: nowNanos(),
    source: "trades",
    payload: {
      symbol,
      price: "50000.00",
      quantity: "0.0001",
      side: "BUY",
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeTickerDelta(symbol: string): BaseEvent<unknown> {
  const domain = `market_data::${symbol}`;
  return {
    event_id: uid(),
    event_type: "delta",
    sequence: nextSeq(domain),
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

function makeOrderbookDelta(symbol: string): BaseEvent<unknown> {
  const domain = `market_data::${symbol}`;
  const isBid = Math.random() > 0.5;
  const price = (50000 + (isBid ? -1 : 1) * Math.random() * 50).toFixed(2);
  const qty = Math.random() > 0.85 ? "0" : (Math.random() * 5).toFixed(4);
  return {
    event_id: uid(),
    event_type: "delta",
    sequence: nextSeq(domain),
    timestamp: nowNanos(),
    source: "market_data",
    payload: {
      symbol,
      [isBid ? "bids" : "asks"]: [[price, qty]],
    },
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function makeTrade(symbol: string): BaseEvent<unknown> {
  const domain = `trades::${symbol}`;
  return {
    event_id: uid(),
    event_type: "delta",
    sequence: nextSeq(domain),
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

function generateEvent(symbol: string): BaseEvent<unknown> {
  const roll = Math.random();
  if (roll < 0.6) return makeOrderbookDelta(symbol);
  if (roll < 0.85) return makeTrade(symbol);
  return makeTickerDelta(symbol);
}

// ---------------------------------------------------------------------------
// Percentile computation
// ---------------------------------------------------------------------------

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

// ---------------------------------------------------------------------------
// Benchmark
// ---------------------------------------------------------------------------

interface BenchResult {
  config: { rate: number; duration: number; symbols: string[] };
  total_events: number;
  actual_rate: number;
  dispatch_latency_ms: {
    median: number;
    p95: number;
    p99: number;
    max: number;
    min: number;
  };
  heap_samples_mb: number[];
  heap_growth_pct: number;
  final_metrics: {
    events_ignored: number;
    gaps_detected: number;
    buffer_sizes: Record<string, number>;
  };
  max_buffer_pct: number;
  passed_kpis: {
    median_under_100ms: boolean;
    p95_under_300ms: boolean;
    heap_growth_acceptable: boolean;
    buffer_under_1pct: boolean;
  };
  timestamp: string;
}

async function runBenchmark(config: BenchConfig): Promise<BenchResult> {
  const store = new DexStateStore();
  const latencies: number[] = [];
  const heapSamples: number[] = [];

  const MAX_BUFFER_SIZE = 10_000; // mirrors store constant

  console.log(`[bench] Starting benchmark`);
  console.log(`[bench] Rate: ${config.rate} msg/sec`);
  console.log(`[bench] Duration: ${config.duration}s`);
  console.log(`[bench] Symbols: ${config.symbols.join(", ")}`);

  // Send initial snapshots for each symbol (primes all domains)
  resetSeqCounters();
  for (const sym of config.symbols) {
    const snap = makeSnapshot(sym);
    store.dispatch(snap);
    // Prime trades domain with an initial event
    const tradeInit = makeTradesSnapshot(sym);
    store.dispatch(tradeInit);
  }

  // --- WARMUP PHASE ---
  // Dispatch enough events to fill ALL bounded structures to steady-state:
  //   - Trade list: MAX_TRADES_PER_SYMBOL=500 (need ~3333 events at 15% trade rate)
  //   - SeenIds: fills proportionally
  // After warmup, further dispatches only replace existing entries, so heap stays flat.
  const WARMUP_COUNT = 5000;
  let warmupSymIdx = 0;
  console.log(`[bench] Warming up with ${WARMUP_COUNT} events...`);
  for (let w = 0; w < WARMUP_COUNT; w++) {
    const sym = config.symbols[warmupSymIdx % config.symbols.length];
    warmupSymIdx++;
    const event = generateEvent(sym);
    store.dispatch(event);
  }

  // Force GC if available, then let V8 settle
  if (global.gc) global.gc();

  const startTime = Date.now();
  let eventsDispatched = 0;
  let symbolIdx = 0;

  // Heap sampling
  const heapInterval = setInterval(() => {
    if (typeof process !== "undefined" && process.memoryUsage) {
      const heap = process.memoryUsage().heapUsed / (1024 * 1024);
      heapSamples.push(parseFloat(heap.toFixed(2)));
    }
  }, 5000);

  // Initial heap sample
  if (typeof process !== "undefined" && process.memoryUsage) {
    heapSamples.push(parseFloat((process.memoryUsage().heapUsed / (1024 * 1024)).toFixed(2)));
  }

  // Event generation loop — use precise timing
  const intervalMs = 1000 / config.rate;
  const totalEvents = config.rate * config.duration;

  return new Promise<BenchResult>((resolve) => {
    const timer = setInterval(() => {
      // Dispatch a batch to keep up with rate
      const batchSize = Math.max(1, Math.floor(config.rate / 100));

      for (let b = 0; b < batchSize && eventsDispatched < totalEvents; b++) {
        const sym = config.symbols[symbolIdx % config.symbols.length];
        symbolIdx++;

        const event = generateEvent(sym);
        const t0 = performance.now();
        store.dispatch(event);
        const t1 = performance.now();

        latencies.push(t1 - t0);
        eventsDispatched++;
      }

      const elapsed = (Date.now() - startTime) / 1000;

      // Progress every 5s
      if (Math.floor(elapsed) % 5 === 0 && Math.floor(elapsed) > 0) {
        const rate = eventsDispatched / elapsed;
        process.stdout.write(
          `\r[bench] ${elapsed.toFixed(0)}s | Events: ${eventsDispatched} | Rate: ${rate.toFixed(1)}/s   `
        );
      }

      if (eventsDispatched >= totalEvents || elapsed >= config.duration + 2) {
        clearInterval(timer);
        clearInterval(heapInterval);

        // Final heap sample
        if (typeof process !== "undefined" && process.memoryUsage) {
          heapSamples.push(parseFloat((process.memoryUsage().heapUsed / (1024 * 1024)).toFixed(2)));
        }

        const totalTime = (Date.now() - startTime) / 1000;
        const actualRate = eventsDispatched / totalTime;

        // Sort latencies for percentile calculation
        latencies.sort((a, b) => a - b);

        const metrics = store.getState().metrics;
        const bufferSizes: Record<string, number> = {};
        for (const [k, v] of metrics.buffer_size_by_stream) {
          bufferSizes[k] = v;
        }

        const maxBufferSize = Math.max(0, ...Object.values(bufferSizes));
        const maxBufferPct = (maxBufferSize / MAX_BUFFER_SIZE) * 100;

        const heapGrowthPct = heapSamples.length >= 3
          ? (() => {
            // Compare second sample (after initial GC settle) vs last
            const baseline = heapSamples[1] ?? heapSamples[0];
            const final = heapSamples[heapSamples.length - 1];
            return ((final - baseline) / baseline) * 100;
          })()
          : heapSamples.length >= 2
            ? ((heapSamples[heapSamples.length - 1] - heapSamples[0]) / heapSamples[0]) * 100
            : 0;

        const medianLatency = percentile(latencies, 50);
        const p95Latency = percentile(latencies, 95);
        const p99Latency = percentile(latencies, 99);

        const result: BenchResult = {
          config: {
            rate: config.rate,
            duration: config.duration,
            symbols: config.symbols,
          },
          total_events: eventsDispatched,
          actual_rate: parseFloat(actualRate.toFixed(2)),
          dispatch_latency_ms: {
            median: parseFloat(medianLatency.toFixed(4)),
            p95: parseFloat(p95Latency.toFixed(4)),
            p99: parseFloat(p99Latency.toFixed(4)),
            max: parseFloat(Math.max(...latencies).toFixed(4)),
            min: parseFloat(Math.min(...latencies).toFixed(4)),
          },
          heap_samples_mb: heapSamples,
          heap_growth_pct: parseFloat(heapGrowthPct.toFixed(2)),
          final_metrics: {
            events_ignored: metrics.events_ignored,
            gaps_detected: metrics.gaps_detected,
            buffer_sizes: bufferSizes,
          },
          max_buffer_pct: parseFloat(maxBufferPct.toFixed(4)),
          passed_kpis: {
            median_under_100ms: medianLatency < 100,
            p95_under_300ms: p95Latency < 300,
            heap_growth_acceptable: heapGrowthPct < 40,
            buffer_under_1pct: maxBufferPct < 1,
          },
          timestamp: new Date().toISOString(),
        };

        resolve(result);
      }
    }, Math.max(1, intervalMs / Math.max(1, Math.floor(config.rate / 100))));
  });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const config = parseArgs();
  const result = await runBenchmark(config);

  console.log("\n\n=== BENCHMARK RESULTS ===");
  console.log(`  Total events:    ${result.total_events}`);
  console.log(`  Actual rate:     ${result.actual_rate} msg/sec`);
  console.log(`  Median latency:  ${result.dispatch_latency_ms.median}ms`);
  console.log(`  P95 latency:     ${result.dispatch_latency_ms.p95}ms`);
  console.log(`  P99 latency:     ${result.dispatch_latency_ms.p99}ms`);
  console.log(`  Max latency:     ${result.dispatch_latency_ms.max}ms`);
  console.log(`  Heap growth:     ${result.heap_growth_pct}%`);
  console.log(`  Max buffer:      ${result.max_buffer_pct}%`);
  console.log(`  Events ignored:  ${result.final_metrics.events_ignored}`);
  console.log(`  Gaps detected:   ${result.final_metrics.gaps_detected}`);
  console.log("");
  console.log("  KPI PASS/FAIL:");
  console.log(`    Median < 100ms:     ${result.passed_kpis.median_under_100ms ? "PASS ✓" : "FAIL ✗"}`);
  console.log(`    P95 < 300ms:        ${result.passed_kpis.p95_under_300ms ? "PASS ✓" : "FAIL ✗"}`);
  console.log(`    Heap growth < 40%:  ${result.passed_kpis.heap_growth_acceptable ? "PASS ✓" : "FAIL ✗"}`);
  console.log(`    Buffer < 1%:        ${result.passed_kpis.buffer_under_1pct ? "PASS ✓" : "FAIL ✗"}`);
  console.log("=========================");

  // Write results
  const outDir = path.dirname(config.outputPath);
  if (!fs.existsSync(outDir)) {
    fs.mkdirSync(outDir, { recursive: true });
  }
  fs.writeFileSync(config.outputPath, JSON.stringify(result, null, 2));
  console.log(`\n[bench] Results written to ${config.outputPath}`);

  // Exit with error if KPIs failed
  const allPass = Object.values(result.passed_kpis).every(Boolean);
  if (!allPass) {
    console.error("[bench] KPI FAILURE — some targets were not met.");
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("[bench] Fatal error:", err);
  process.exit(1);
});
