#!/usr/bin/env node
// ---------------------------------------------------------------------------
// Observability server — /healthz, /readyz, /metrics endpoints
// ---------------------------------------------------------------------------
//
// Runs alongside the web-ui dev server. Exposes health checks and
// Prometheus-format metrics for the frontend application state.
//
// In production, these would be served by the gateway or a sidecar.
// In dev mode, this standalone server provides equivalent endpoints.
//
// Usage:
//   npx tsx ops/observability-server.ts [--port 9091] [--ws-url ws://localhost:8080/v1/ws]
// ---------------------------------------------------------------------------

import { createServer, IncomingMessage, ServerResponse } from "http";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

function parseArgs(): { port: number; wsUrl: string } {
  const args = process.argv.slice(2);
  let port = 9091;
  let wsUrl = "ws://localhost:8080/v1/ws";
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--port" && args[i + 1]) {
      port = parseInt(args[i + 1], 10);
      i++;
    }
    if (args[i] === "--ws-url" && args[i + 1]) {
      wsUrl = args[i + 1];
      i++;
    }
  }
  return { port, wsUrl };
}

// ---------------------------------------------------------------------------
// Metrics state (populated externally or via mock data)
// ---------------------------------------------------------------------------

interface MetricsState {
  last_seq_by_stream: Record<string, string>;
  buffer_size_total: number;
  events_ignored_total: number;
  gaps_detected_total: number;
  connected_clients: number;
  uptime_seconds: number;
}

const startTime = Date.now();

// Mutable metrics — in a real integration, these would be updated by the app
const metrics: MetricsState = {
  last_seq_by_stream: {},
  buffer_size_total: 0,
  events_ignored_total: 0,
  gaps_detected_total: 0,
  connected_clients: 0,
  uptime_seconds: 0,
};

// External metric update API
function updateMetrics(update: Partial<MetricsState>): void {
  Object.assign(metrics, update);
}

// ---------------------------------------------------------------------------
// Prometheus text format
// ---------------------------------------------------------------------------

function renderPrometheusMetrics(): string {
  const lines: string[] = [];

  lines.push("# HELP dex_uptime_seconds Time since observability server started");
  lines.push("# TYPE dex_uptime_seconds gauge");
  lines.push(`dex_uptime_seconds ${((Date.now() - startTime) / 1000).toFixed(1)}`);

  lines.push("");
  lines.push("# HELP dex_events_ignored_total Total events ignored (duplicates)");
  lines.push("# TYPE dex_events_ignored_total counter");
  lines.push(`dex_events_ignored_total ${metrics.events_ignored_total}`);

  lines.push("");
  lines.push("# HELP dex_gaps_detected_total Total sequence gaps detected");
  lines.push("# TYPE dex_gaps_detected_total counter");
  lines.push(`dex_gaps_detected_total ${metrics.gaps_detected_total}`);

  lines.push("");
  lines.push("# HELP dex_buffer_size_total Total buffered events across all streams");
  lines.push("# TYPE dex_buffer_size_total gauge");
  lines.push(`dex_buffer_size_total ${metrics.buffer_size_total}`);

  lines.push("");
  lines.push("# HELP dex_connected_clients Number of WebSocket client connections (dev mode)");
  lines.push("# TYPE dex_connected_clients gauge");
  lines.push(`dex_connected_clients ${metrics.connected_clients}`);

  lines.push("");
  lines.push("# HELP dex_last_seq_by_stream Last sequence number per data stream");
  lines.push("# TYPE dex_last_seq_by_stream gauge");
  for (const [stream, seq] of Object.entries(metrics.last_seq_by_stream)) {
    const safeStream = stream.replace(/[^a-zA-Z0-9_]/g, "_");
    lines.push(`dex_last_seq_by_stream{stream="${safeStream}"} ${seq}`);
  }

  lines.push("");
  return lines.join("\n") + "\n";
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

const { port, wsUrl } = parseArgs();

const server = createServer((req: IncomingMessage, res: ServerResponse) => {
  if (req.url === "/healthz") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      status: "ok",
      uptime_seconds: Math.floor((Date.now() - startTime) / 1000),
      timestamp: new Date().toISOString(),
    }));
    return;
  }

  if (req.url === "/readyz") {
    // Check if the store is alive and WS is connectable
    const ready = true; // In real integration, would check actual connectivity
    res.writeHead(ready ? 200 : 503, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      status: ready ? "ready" : "not_ready",
      checks: {
        store_alive: true,
        ws_connectable: true,
      },
    }));
    return;
  }

  if (req.url === "/metrics") {
    res.writeHead(200, { "Content-Type": "text/plain; version=0.0.4; charset=utf-8" });
    res.end(renderPrometheusMetrics());
    return;
  }

  // POST /metrics/update — allows app to push metrics
  if (req.url === "/metrics/update" && req.method === "POST") {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      try {
        const update = JSON.parse(body);
        updateMetrics(update);
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "updated" }));
      } catch {
        res.writeHead(400);
        res.end("Bad JSON");
      }
    });
    return;
  }

  // Default
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify({
    status: "observability-server",
    endpoints: {
      "GET /healthz": "Health check",
      "GET /readyz": "Readiness check",
      "GET /metrics": "Prometheus metrics",
      "POST /metrics/update": "Push metrics from app",
    },
  }));
});

server.listen(port, () => {
  console.log(`[observability] Listening on port ${port}`);
  console.log(`[observability] Endpoints:`);
  console.log(`  GET  http://localhost:${port}/healthz`);
  console.log(`  GET  http://localhost:${port}/readyz`);
  console.log(`  GET  http://localhost:${port}/metrics`);
  console.log(`  POST http://localhost:${port}/metrics/update`);
});
