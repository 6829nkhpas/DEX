#!/usr/bin/env node
// ---------------------------------------------------------------------------
// ws-publisher.ts — High-rate event publisher for stress testing
// ---------------------------------------------------------------------------
//
// Generates events at a configurable rate and sends them directly to the
// mock-ws-server via its HTTP control API, or generates events via its own
// WebSocket connection for direct store-level benchmarking.
//
// CLI:
//   npx tsx tools/ws-publisher.ts --rate 500 --symbols BTC/USDT --duration 60
//   npx tsx tools/ws-publisher.ts --rate 100 --symbols BTC/USDT,ETH/USDT,SOL/USDT --duration 30
//
// Acceptance:
//   - Can sustain 500 msg/sec for 1 symbol
//   - Reports actual throughput with ±5% accuracy
// ---------------------------------------------------------------------------

interface PublisherConfig {
  rate: number;        // events per second
  symbols: string[];
  duration: number;    // seconds
  serverUrl: string;   // mock server base URL
  mode: "server" | "direct"; // "server" = use HTTP control API, "direct" = generate locally
}

function parseArgs(): PublisherConfig {
  const args = process.argv.slice(2);
  const config: PublisherConfig = {
    rate: 100,
    symbols: ["BTC/USDT"],
    duration: 60,
    serverUrl: "http://localhost:8080",
    mode: "server",
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--rate":
        config.rate = parseInt(args[++i], 10);
        break;
      case "--symbols":
        config.symbols = args[++i].split(",").map((s) => s.trim());
        break;
      case "--duration":
        config.duration = parseInt(args[++i], 10);
        break;
      case "--server":
        config.serverUrl = args[++i];
        break;
      case "--mode":
        config.mode = args[++i] as "server" | "direct";
        break;
    }
  }
  return config;
}

// ---------------------------------------------------------------------------
// Event generation (for direct mode)
// ---------------------------------------------------------------------------

let seqCounter = 0;

interface BaseEvent<T = unknown> {
  event_id: string;
  event_type: string;
  sequence: string;
  timestamp: string;
  source: string;
  payload: T;
  metadata: { version: string; correlation_id: string; causation_id: string };
}

function makeEvent<T>(source: string, eventType: string, payload: T): BaseEvent<T> {
  return {
    event_id: `pub-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    event_type: eventType,
    sequence: String(++seqCounter),
    timestamp: String(Date.now() * 1_000_000),
    source,
    payload,
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

function generateRandomEvent(symbol: string): BaseEvent<unknown> {
  const roll = Math.random();
  if (roll < 0.6) {
    // Orderbook delta
    const isBid = Math.random() > 0.5;
    const price = (50000 + (isBid ? -1 : 1) * Math.random() * 50).toFixed(2);
    const qty = Math.random() > 0.85 ? "0" : (Math.random() * 5).toFixed(4);
    return makeEvent("market_data", "delta", {
      symbol,
      [isBid ? "bids" : "asks"]: [[price, qty]],
    });
  } else if (roll < 0.85) {
    // Trade
    return makeEvent("trades", "delta", {
      symbol,
      price: (50000 + Math.random() * 100).toFixed(2),
      quantity: (Math.random() * 2).toFixed(4),
      side: Math.random() > 0.5 ? "BUY" : "SELL",
    });
  } else {
    // Ticker
    return makeEvent("market_data", "delta", {
      symbol,
      last_price: (50000 + Math.random() * 100).toFixed(2),
      mark_price: (50000 + Math.random() * 100).toFixed(2),
    });
  }
}

// ---------------------------------------------------------------------------
// Publishing — server mode (use HTTP control API)
// ---------------------------------------------------------------------------

async function publishServerMode(config: PublisherConfig): Promise<void> {
  console.log(`[ws-publisher] Server mode: ${config.serverUrl}`);
  console.log(`[ws-publisher] Rate: ${config.rate} msg/sec, Duration: ${config.duration}s`);
  console.log(`[ws-publisher] Symbols: ${config.symbols.join(", ")}`);

  // Start publishing on the server
  const startRes = await fetch(`${config.serverUrl}/control/start`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ rate: config.rate }),
  });
  if (!startRes.ok) {
    throw new Error(`Failed to start publishing: ${startRes.status}`);
  }
  console.log(`[ws-publisher] Publishing started at ${config.rate} msg/sec`);

  // Get initial status
  const initStatus = await (await fetch(`${config.serverUrl}/control/status`)).json() as { seq: number };
  const startSeq = initStatus.seq;
  const startTime = Date.now();

  // Wait for duration
  await new Promise<void>((resolve) => {
    const checkInterval = setInterval(async () => {
      const elapsed = (Date.now() - startTime) / 1000;
      const statusRes = await fetch(`${config.serverUrl}/control/status`);
      const status = await statusRes.json() as { seq: number; clients: number; rate: number };
      const eventsGenerated = status.seq - startSeq;
      const actualRate = eventsGenerated / elapsed;

      process.stdout.write(
        `\r[ws-publisher] ${elapsed.toFixed(1)}s | Events: ${eventsGenerated} | ` +
        `Actual rate: ${actualRate.toFixed(1)}/sec | Clients: ${status.clients}   `
      );

      if (elapsed >= config.duration) {
        clearInterval(checkInterval);
        resolve();
      }
    }, 1000);
  });

  // Stop publishing
  await fetch(`${config.serverUrl}/control/stop`, { method: "POST" });

  // Final stats
  const finalStatus = await (await fetch(`${config.serverUrl}/control/status`)).json() as { seq: number };
  const totalEvents = finalStatus.seq - startSeq;
  const totalTime = (Date.now() - startTime) / 1000;
  const actualRate = totalEvents / totalTime;
  const rateAccuracy = Math.abs(1 - actualRate / config.rate) * 100;

  console.log("\n");
  console.log("=== PUBLISHER RESULTS ===");
  console.log(`  Target rate:   ${config.rate} msg/sec`);
  console.log(`  Actual rate:   ${actualRate.toFixed(2)} msg/sec`);
  console.log(`  Total events:  ${totalEvents}`);
  console.log(`  Duration:      ${totalTime.toFixed(2)}s`);
  console.log(`  Rate accuracy: ${rateAccuracy.toFixed(2)}% deviation`);
  console.log(`  Within ±5%:    ${rateAccuracy <= 5 ? "YES ✓" : "NO ✗"}`);
  console.log("=========================");
}

// ---------------------------------------------------------------------------
// Publishing — direct mode (generate locally for store-level tests)
// ---------------------------------------------------------------------------

async function publishDirectMode(config: PublisherConfig): Promise<void> {
  console.log(`[ws-publisher] Direct mode — generating events to stdout`);
  console.log(`[ws-publisher] Rate: ${config.rate} msg/sec, Duration: ${config.duration}s`);

  const intervalMs = 1000 / config.rate;
  const startTime = Date.now();
  let eventsGenerated = 0;
  let symbolIdx = 0;

  return new Promise<void>((resolve) => {
    const timer = setInterval(() => {
      const symbol = config.symbols[symbolIdx % config.symbols.length];
      symbolIdx++;

      const event = generateRandomEvent(symbol);
      eventsGenerated++;
      process.stdout.write(JSON.stringify(event) + "\n");

      const elapsed = (Date.now() - startTime) / 1000;
      if (elapsed >= config.duration) {
        clearInterval(timer);
        const actualRate = eventsGenerated / elapsed;
        console.error("\n=== PUBLISHER RESULTS ===");
        console.error(`  Target rate:   ${config.rate} msg/sec`);
        console.error(`  Actual rate:   ${actualRate.toFixed(2)} msg/sec`);
        console.error(`  Total events:  ${eventsGenerated}`);
        console.error(`  Duration:      ${elapsed.toFixed(2)}s`);
        console.error("=========================");
        resolve();
      }
    }, intervalMs);
  });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const config = parseArgs();

  if (config.mode === "server") {
    await publishServerMode(config);
  } else {
    await publishDirectMode(config);
  }
}

main().catch((err) => {
  console.error("[ws-publisher] Fatal error:", err);
  process.exit(1);
});
