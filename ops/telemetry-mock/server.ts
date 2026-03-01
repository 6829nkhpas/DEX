#!/usr/bin/env node
// ---------------------------------------------------------------------------
// Telemetry Mock Receiver — development stub for telemetry events
// ---------------------------------------------------------------------------
//
// Starts an HTTP server that accepts POST /telemetry with batched events.
// Stores events in-memory and exposes them via GET /events for inspection.
//
// Usage:
//   npx tsx ops/telemetry-mock/server.ts [--port 9090]
// ---------------------------------------------------------------------------

import { createServer, IncomingMessage, ServerResponse } from "http";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

function parsePort(): number {
  const args = process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port" && args[i + 1]) {
      return parseInt(args[i + 1], 10);
    }
  }
  return 9090;
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

interface TelemetryEvent {
  type: string;
  timestamp: string;
  data: Record<string, unknown>;
  session_id?: string;
}

const collectedEvents: TelemetryEvent[] = [];
const MAX_EVENTS = 10_000;

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

const port = parsePort();

const server = createServer((req: IncomingMessage, res: ServerResponse) => {
  // CORS headers for local dev
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");

  if (req.method === "OPTIONS") {
    res.writeHead(204);
    res.end();
    return;
  }

  // POST /telemetry — receive events
  if (req.url === "/telemetry" && req.method === "POST") {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      try {
        const parsed = JSON.parse(body);
        const events: TelemetryEvent[] = parsed.events || [];

        for (const event of events) {
          collectedEvents.push(event);
          console.log(
            `[telemetry] ${event.type} | ${event.timestamp} | ${JSON.stringify(event.data)}`,
          );
        }

        // Cap stored events
        if (collectedEvents.length > MAX_EVENTS) {
          collectedEvents.splice(0, collectedEvents.length - MAX_EVENTS);
        }

        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ accepted: events.length, total: collectedEvents.length }));
      } catch {
        res.writeHead(400, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Invalid JSON" }));
      }
    });
    return;
  }

  // GET /events — retrieve collected events
  if (req.url === "/events" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ events: collectedEvents, total: collectedEvents.length }));
    return;
  }

  // GET /events/count — just the count
  if (req.url === "/events/count" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ total: collectedEvents.length }));
    return;
  }

  // POST /events/clear — clear all events
  if (req.url === "/events/clear" && req.method === "POST") {
    collectedEvents.length = 0;
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "cleared" }));
    return;
  }

  // GET /healthz
  if (req.url === "/healthz") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "ok", events_collected: collectedEvents.length }));
    return;
  }

  // Default
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(
    JSON.stringify({
      status: "telemetry-mock",
      endpoints: {
        "POST /telemetry": "Receive batched telemetry events",
        "GET /events": "Retrieve all collected events",
        "GET /events/count": "Get event count",
        "POST /events/clear": "Clear all events",
        "GET /healthz": "Health check",
      },
    }),
  );
});

server.listen(port, () => {
  console.log(`[telemetry-mock] Listening on port ${port}`);
  console.log(`[telemetry-mock] Endpoints:`);
  console.log(`  POST http://localhost:${port}/telemetry     — receive events`);
  console.log(`  GET  http://localhost:${port}/events        — list events`);
  console.log(`  GET  http://localhost:${port}/events/count  — count events`);
  console.log(`  POST http://localhost:${port}/events/clear  — clear events`);
  console.log(`  GET  http://localhost:${port}/healthz       — health check`);
});
