#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# bench-prod-like.sh — Run production-like performance benchmark
# ---------------------------------------------------------------------------
# Starts the mock WS server, runs the bench-runner at a configurable
# production-like rate for 5 minutes, and outputs results.
#
# Usage:
#   ./scripts/bench-prod-like.sh [--rate 500] [--duration 300]
#
# Output:
#   apps/web-ui/perf/results-prod-like.json
# ---------------------------------------------------------------------------

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WEB_UI_DIR="$PROJECT_ROOT/apps/web-ui"

# ── Defaults ──────────────────────────────────────────────────
RATE=500
DURATION=300
SYMBOLS="BTC/USDT,ETH/USDT,SOL/USDT"
OUTPUT="$WEB_UI_DIR/perf/results-prod-like.json"
MOCK_PORT=8080

# ── Parse args ────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case $1 in
    --rate) RATE="$2"; shift 2 ;;
    --duration) DURATION="$2"; shift 2 ;;
    --symbols) SYMBOLS="$2"; shift 2 ;;
    --output) OUTPUT="$2"; shift 2 ;;
    --port) MOCK_PORT="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

echo "╔═══════════════════════════════════════════════╗"
echo "║   DEX Web UI — Production-Like Benchmark     ║"
echo "╠═══════════════════════════════════════════════╣"
echo "║  Rate:     ${RATE} msg/sec                        ║"
echo "║  Duration: ${DURATION}s                            ║"
echo "║  Symbols:  ${SYMBOLS}             ║"
echo "╚═══════════════════════════════════════════════╝"
echo ""

cd "$WEB_UI_DIR"

# ── Step 1: Install deps if needed ────────────────────────────
if [ ! -d node_modules ]; then
  echo "▶ Installing dependencies..."
  npm ci
fi

# ── Step 2: Start mock WS server ─────────────────────────────
echo "▶ Starting mock WebSocket server on port $MOCK_PORT..."
npx tsx tools/mock-ws-server.ts --port "$MOCK_PORT" --symbols "$SYMBOLS" &
MOCK_PID=$!

# Wait for server to start
sleep 3

# Verify mock server is running
if ! kill -0 $MOCK_PID 2>/dev/null; then
  echo "✗ Mock server failed to start"
  exit 1
fi
echo "  ✓ Mock server started (PID: $MOCK_PID)"
echo ""

# ── Step 3: Run benchmark ────────────────────────────────────
echo "▶ Running benchmark (${RATE} msg/sec × ${DURATION}s)..."
echo "  This will take approximately $((DURATION + 10)) seconds..."
echo ""

npx tsx perf/bench-runner.ts \
  --rate "$RATE" \
  --duration "$DURATION" \
  --symbols "$SYMBOLS" \
  --output "$OUTPUT"

BENCH_EXIT=$?

# ── Step 4: Cleanup ──────────────────────────────────────────
echo ""
echo "▶ Stopping mock server..."
kill $MOCK_PID 2>/dev/null || true
wait $MOCK_PID 2>/dev/null || true
echo "  ✓ Cleanup complete"
echo ""

# ── Step 5: Report ───────────────────────────────────────────
if [ $BENCH_EXIT -eq 0 ]; then
  echo "════════════════════════════════════════════════"
  echo "  ✓ ALL KPIs PASSED"
  echo ""
  echo "  Results: $OUTPUT"
  echo "════════════════════════════════════════════════"
else
  echo "════════════════════════════════════════════════"
  echo "  ✗ KPI FAILURE — review results"
  echo ""
  echo "  Results: $OUTPUT"
  echo "════════════════════════════════════════════════"
  exit 1
fi
