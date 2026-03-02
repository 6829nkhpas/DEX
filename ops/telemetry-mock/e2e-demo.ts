#!/usr/bin/env node
// ---------------------------------------------------------------------------
// Telemetry E2E demo — emits events and verifies they reach the mock receiver
// ---------------------------------------------------------------------------
//
// This script:
//   1. Starts the telemetry mock server
//   2. Creates a TelemetryClient pointed at the mock
//   3. Emits sampled events
//   4. Flushes and verifies events were collected
//
// Usage:
//   npx tsx ops/telemetry-mock/e2e-demo.ts
// ---------------------------------------------------------------------------

import { createServer, IncomingMessage, ServerResponse } from "http";

// ---- Inline mini-telemetry client (no import dependency on src) ----

interface TelemetryEvent {
  type: string;
  timestamp: string;
  data: Record<string, unknown>;
  session_id?: string;
}

class DemoTelemetryClient {
  private buffer: TelemetryEvent[] = [];
  private endpoint: string;
  private sampleRate: number;
  private sampled = 0;
  private dropped = 0;

  constructor(endpoint: string, sampleRate: number) {
    this.endpoint = endpoint;
    this.sampleRate = sampleRate;
  }

  emit(type: string, data: Record<string, unknown> = {}): void {
    if (Math.random() > this.sampleRate) {
      this.dropped++;
      return;
    }
    this.sampled++;
    this.buffer.push({
      type,
      timestamp: new Date().toISOString(),
      data,
      session_id: "e2e-demo",
    });
  }

  forceEmit(type: string, data: Record<string, unknown> = {}): void {
    this.sampled++;
    this.buffer.push({
      type,
      timestamp: new Date().toISOString(),
      data,
      session_id: "e2e-demo",
    });
  }

  async flush(): Promise<number> {
    if (this.buffer.length === 0) return 0;
    const batch = this.buffer.splice(0);
    const res = await fetch(this.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ events: batch }),
    });
    if (!res.ok) throw new Error(`Flush failed: ${res.status}`);
    const json = await res.json() as { accepted: number };
    return json.accepted;
  }

  stats() { return { sampled: this.sampled, dropped: this.dropped, buffered: this.buffer.length }; }
}

// ---- Inline mock server ----

function startMockServer(port: number): Promise<ReturnType<typeof createServer>> {
  const events: TelemetryEvent[] = [];
  return new Promise((resolve) => {
    const srv = createServer((req: IncomingMessage, res: ServerResponse) => {
      res.setHeader("Access-Control-Allow-Origin", "*");
      res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
      res.setHeader("Access-Control-Allow-Headers", "Content-Type");

      if (req.url === "/telemetry" && req.method === "POST") {
        let body = "";
        req.on("data", (c) => (body += c));
        req.on("end", () => {
          const parsed = JSON.parse(body);
          for (const e of parsed.events || []) events.push(e);
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ accepted: (parsed.events || []).length, total: events.length }));
        });
        return;
      }
      if (req.url === "/events/count") {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ total: events.length }));
        return;
      }
      res.writeHead(200);
      res.end("ok");
    });
    srv.listen(port, () => resolve(srv));
  });
}

// ---- Main ----

async function main() {
  const PORT = 19090;
  console.log("[e2e-demo] Starting telemetry mock server...");
  const server = await startMockServer(PORT);

  const client = new DemoTelemetryClient(`http://localhost:${PORT}/telemetry`, 1.0);

  console.log("[e2e-demo] Emitting telemetry events...");

  // Emit various event types
  client.forceEmit("connection_lifecycle", { action: "connect", ws_url: "ws://localhost:8080" });
  client.forceEmit("gap_detected", { stream: "market_data::BTC/USDT", expected_seq: "100", received_seq: "105" });
  client.forceEmit("buffer_overflow", { stream: "market_data::ETH/USDT", buffer_size: 10001 });
  client.forceEmit("snapshot_request", { channel: "market_data", symbol: "BTC/USDT", since_seq: 100 });
  client.forceEmit("subscription_count", { count: 5, symbols: ["BTC/USDT", "ETH/USDT"] });
  client.forceEmit("cpu_warning", { usage_pct: 85, threshold: 80 });

  // Also emit some sampled events (at 100% rate, all should pass)
  for (let i = 0; i < 10; i++) {
    client.emit("connection_lifecycle", { action: "heartbeat", seq: i });
  }

  console.log(`[e2e-demo] Stats: ${JSON.stringify(client.stats())}`);

  // Flush to server
  const flushed = await client.flush();
  console.log(`[e2e-demo] Flushed ${flushed} events`);

  // Verify events were received
  const countRes = await fetch(`http://localhost:${PORT}/events/count`);
  const countData = await countRes.json() as { total: number };
  console.log(`[e2e-demo] Server received: ${countData.total} events`);

  if (countData.total >= 16) {
    console.log("[e2e-demo] ✅ PASS — all events received by mock server");
  } else {
    console.error(`[e2e-demo] ❌ FAIL — expected >= 16 events, got ${countData.total}`);
    process.exit(1);
  }

  server.close();
  console.log("[e2e-demo] Done.");
}

main().catch((err) => {
  console.error("[e2e-demo] Fatal:", err);
  process.exit(1);
});
