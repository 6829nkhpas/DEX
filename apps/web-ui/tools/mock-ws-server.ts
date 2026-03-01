#!/usr/bin/env node
// ---------------------------------------------------------------------------
// mock-ws-server.ts — Canonical mock WebSocket server for stress testing
// ---------------------------------------------------------------------------
//
// Provides:
//   - Canonical event envelope generation (BaseEvent)
//   - Snapshot + delta flow per channel
//   - snapshot_since support
//   - Configurable rate, symbols, duration
//   - Multi-symbol capable
//   - Sequence as string, Timestamp as string
//
// Usage:
//   npx tsx tools/mock-ws-server.ts [--port 8080] [--symbols BTC/USDT,ETH/USDT]
// ---------------------------------------------------------------------------

import { createServer, IncomingMessage, ServerResponse } from "http";
import { WebSocketServer, WebSocket } from "ws";

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

function parseArgs(): { port: number; symbols: string[] } {
  const args = process.argv.slice(2);
  let port = 8080;
  let symbols = ["BTC/USDT"];

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port" && args[i + 1]) {
      port = parseInt(args[i + 1], 10);
      i++;
    }
    if (args[i] === "--symbols" && args[i + 1]) {
      symbols = args[i + 1].split(",").map((s) => s.trim());
      i++;
    }
  }
  return { port, symbols };
}

// ---------------------------------------------------------------------------
// Event envelope factory
// ---------------------------------------------------------------------------

let globalSeqCounter = 0;

interface BaseEvent<T = unknown> {
  event_id: string;
  event_type: string;
  sequence: string;
  timestamp: string;
  source: string;
  payload: T;
  metadata: { version: string; correlation_id: string; causation_id: string };
}

function nextSeq(): string {
  return String(++globalSeqCounter);
}

function nowNanos(): string {
  return String(Date.now() * 1_000_000);
}

function uid(): string {
  return `evt-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function makeEvent<T>(
  source: string,
  eventType: string,
  payload: T,
): BaseEvent<T> {
  return {
    event_id: uid(),
    event_type: eventType,
    sequence: nextSeq(),
    timestamp: nowNanos(),
    source,
    payload,
    metadata: { version: "1.0", correlation_id: "", causation_id: "" },
  };
}

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

function generateOrderbookSnapshot(symbol: string): BaseEvent<unknown> {
  const bids: [string, string][] = [];
  const asks: [string, string][] = [];
  const basePrice = 50000 + Math.random() * 100;
  for (let i = 0; i < 25; i++) {
    bids.push([(basePrice - i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
    asks.push([(basePrice + 0.5 + i * 0.5).toFixed(2), (Math.random() * 5 + 0.1).toFixed(4)]);
  }
  return makeEvent("market_data", "snapshot", { symbol, bids, asks });
}

function generateTickerDelta(symbol: string): BaseEvent<unknown> {
  return makeEvent("market_data", "delta", {
    symbol,
    last_price: (50000 + Math.random() * 100).toFixed(2),
    volume_24h: (10000 + Math.random() * 5000).toFixed(2),
    high_24h: (51000 + Math.random() * 200).toFixed(2),
    low_24h: (49000 + Math.random() * 200).toFixed(2),
    mark_price: (50000 + Math.random() * 100).toFixed(2),
  });
}

function generateOrderbookDelta(symbol: string): BaseEvent<unknown> {
  const isBid = Math.random() > 0.5;
  const price = (50000 + (isBid ? -1 : 1) * Math.random() * 50).toFixed(2);
  const qty = Math.random() > 0.85 ? "0" : (Math.random() * 5).toFixed(4);
  const side = isBid ? "bids" : "asks";
  return makeEvent("market_data", "delta", {
    symbol,
    [side]: [[price, qty]],
  });
}

function generateTrade(symbol: string): BaseEvent<unknown> {
  return makeEvent("trades", "delta", {
    symbol,
    price: (50000 + Math.random() * 100).toFixed(2),
    quantity: (Math.random() * 2).toFixed(4),
    side: Math.random() > 0.5 ? "BUY" : "SELL",
  });
}

function generateAccountSnapshot(accountId: string): BaseEvent<unknown> {
  return makeEvent("account", "snapshot", {
    account_id: accountId,
    balances: { USDT: "100000.00", BTC: "2.50000000", ETH: "25.00000000" },
    orders: [],
  });
}

// ---------------------------------------------------------------------------
// Store event history for snapshot_since
// ---------------------------------------------------------------------------

const eventHistory: BaseEvent<unknown>[] = [];
const MAX_HISTORY = 50_000;

function recordToHistory(event: BaseEvent<unknown>): void {
  eventHistory.push(event);
  if (eventHistory.length > MAX_HISTORY) {
    eventHistory.splice(0, eventHistory.length - MAX_HISTORY);
  }
}

function getEventsSince(fromSeq: number): BaseEvent<unknown>[] {
  return eventHistory.filter((e) => Number(e.sequence) > fromSeq);
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

const { port, symbols } = parseArgs();

const httpServer = createServer((_req: IncomingMessage, res: ServerResponse) => {
  // Health check endpoint
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ status: "ok", seq: globalSeqCounter }));
});

const wss = new WebSocketServer({ server: httpServer, path: "/v1/ws" });

const clients = new Set<WebSocket>();

wss.on("connection", (ws: WebSocket) => {
  clients.add(ws);

  // Send connected message
  ws.send(
    JSON.stringify({
      type: "connected",
      session_id: `sess-${Date.now()}`,
    }),
  );

  ws.on("message", (data: Buffer | string) => {
    try {
      const msg = JSON.parse(String(data));

      if (msg.type === "pong") {
        return; // heartbeat ack
      }

      if (msg.action === "subscribe") {
        // Acknowledge subscription
        const ackSeq = globalSeqCounter;
        ws.send(
          JSON.stringify({
            type: "subscribed",
            channel: msg.channel,
            params: msg.params,
            snapshot_seq: ackSeq,
          }),
        );

        // Send initial snapshot
        if (msg.channel === "market_data" && msg.params?.symbol) {
          const snap = generateOrderbookSnapshot(msg.params.symbol);
          recordToHistory(snap);
          ws.send(JSON.stringify(snap));

          // Also send initial ticker
          const ticker = generateTickerDelta(msg.params.symbol);
          recordToHistory(ticker);
          ws.send(JSON.stringify(ticker));
        }

        if (msg.channel === "account" && msg.params?.account_id) {
          const snap = generateAccountSnapshot(msg.params.account_id);
          recordToHistory(snap);
          ws.send(JSON.stringify(snap));
        }
      }

      if (msg.action === "snapshot_since") {
        const fromSeq = msg.params?.last_seq ?? 0;
        const events = getEventsSince(fromSeq);
        ws.send(
          JSON.stringify({
            type: "snapshot_since_response",
            channel: msg.channel,
            from_seq: fromSeq,
            to_seq: globalSeqCounter,
            events,
          }),
        );
      }

      if (msg.action === "unsubscribe") {
        ws.send(
          JSON.stringify({
            type: "unsubscribed",
            channel: msg.channel,
            params: msg.params,
          }),
        );
      }
    } catch {
      // Silently ignore malformed messages
    }
  });

  ws.on("close", () => {
    clients.delete(ws);
  });

  // Start heartbeat pings
  const pingTimer = setInterval(() => {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "ping" }));
    }
  }, 15_000);

  ws.on("close", () => clearInterval(pingTimer));
});

// ---------------------------------------------------------------------------
// Delta publishing loop — configurable via external publisher or internal
// ---------------------------------------------------------------------------

/** Internal publishing state, can be controlled via HTTP API */
let publishingRate = 0; // events/sec — 0 = no internal publishing
let publishingTimer: ReturnType<typeof setInterval> | null = null;

function startPublishing(rate: number): void {
  stopPublishing();
  if (rate <= 0) return;
  publishingRate = rate;

  const intervalMs = 1000 / rate;
  let symbolIdx = 0;

  publishingTimer = setInterval(() => {
    const sym = symbols[symbolIdx % symbols.length];
    symbolIdx++;

    // Generate a mixed event
    const roll = Math.random();
    let event: BaseEvent<unknown>;
    if (roll < 0.6) {
      event = generateOrderbookDelta(sym);
    } else if (roll < 0.85) {
      event = generateTrade(sym);
    } else {
      event = generateTickerDelta(sym);
    }

    recordToHistory(event);
    broadcast(event);
  }, intervalMs);
}

function stopPublishing(): void {
  if (publishingTimer) {
    clearInterval(publishingTimer);
    publishingTimer = null;
  }
  publishingRate = 0;
}

function broadcast(event: BaseEvent<unknown>): void {
  const msg = JSON.stringify(event);
  for (const ws of clients) {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(msg);
    }
  }
}

// Expose publish control on HTTP endpoints
const origListener = httpServer.listeners("request")[0] as (req: IncomingMessage, res: ServerResponse) => void;
httpServer.removeAllListeners("request");
httpServer.on("request", (req: IncomingMessage, res: ServerResponse) => {
  if (req.url === "/control/start" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      try {
        const { rate } = JSON.parse(body);
        startPublishing(rate);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "publishing", rate }));
      } catch {
        res.writeHead(400);
        res.end("Bad JSON");
      }
    });
    return;
  }

  if (req.url === "/control/stop" && req.method === "POST") {
    stopPublishing();
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "stopped" }));
    return;
  }

  if (req.url === "/control/status") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        seq: globalSeqCounter,
        clients: clients.size,
        publishing: publishingRate > 0,
        rate: publishingRate,
        history_size: eventHistory.length,
      }),
    );
    return;
  }

  if (req.url === "/control/reset" && req.method === "POST") {
    stopPublishing();
    globalSeqCounter = 0;
    eventHistory.length = 0;
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "reset" }));
    return;
  }

  // Health check fallback
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({ status: "ok", seq: globalSeqCounter, clients: clients.size }));
});

httpServer.listen(port, () => {
  console.log(`[mock-ws-server] Listening on port ${port}`);
  console.log(`[mock-ws-server] Symbols: ${symbols.join(", ")}`);
  console.log(`[mock-ws-server] WS endpoint: ws://localhost:${port}/v1/ws`);
  console.log(`[mock-ws-server] Control API:`);
  console.log(`  POST /control/start {"rate": 100}`);
  console.log(`  POST /control/stop`);
  console.log(`  GET  /control/status`);
  console.log(`  POST /control/reset`);
});
