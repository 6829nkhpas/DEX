#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# deploy-local.sh — Local/staging deployment via Helm
# ---------------------------------------------------------------------------
# This script demonstrates how to deploy the DEX Web UI to a local
# Kubernetes cluster (minikube, kind, Docker Desktop) or staging.
#
# Prerequisites:
#   - kubectl configured for your cluster
#   - helm v3 installed
#   - Container image built and available
#
# Usage:
#   ./scripts/deploy-local.sh [staging|local]
# ---------------------------------------------------------------------------

set -euo pipefail

# ── Config ────────────────────────────────────────────────────
RELEASE_NAME="dex-ui"
CHART_DIR="deploy/helm/dex-ui"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ENVIRONMENT="${1:-local}"
NAMESPACE="${ENVIRONMENT}"

echo "╔═══════════════════════════════════════════════╗"
echo "║   DEX Web UI — Local Deployment              ║"
echo "╠═══════════════════════════════════════════════╣"
echo "║  Environment: ${ENVIRONMENT}                          ║"
echo "║  Namespace:   ${NAMESPACE}                          ║"
echo "╚═══════════════════════════════════════════════╝"
echo ""

# ── Step 1: Build Docker image (if local) ─────────────────────
if [ "$ENVIRONMENT" = "local" ]; then
  echo "▶ Building Docker image..."
  cd "$PROJECT_ROOT"
  docker build -t dex-web-ui:local -f apps/web-ui/Dockerfile .
  IMAGE_TAG="dex-web-ui:local"
  PULL_POLICY="Never"
  VALUES_FILE="$CHART_DIR/values-staging.yaml"
else
  IMAGE_TAG="${REGISTRY:-ghcr.io/org}/dex-web-ui:${IMAGE_TAG:-staging-latest}"
  PULL_POLICY="Always"
  VALUES_FILE="$CHART_DIR/values-staging.yaml"
fi

echo "  Image: $IMAGE_TAG"
echo ""

# ── Step 2: Create namespace ──────────────────────────────────
echo "▶ Creating namespace '$NAMESPACE' (if not exists)..."
kubectl create namespace "$NAMESPACE" --dry-run=client -o yaml | kubectl apply -f -

# ── Step 3: Helm lint ─────────────────────────────────────────
echo "▶ Linting Helm chart..."
cd "$PROJECT_ROOT"
helm lint "$CHART_DIR"
echo "  ✓ Lint passed"
echo ""

# ── Step 4: Helm install/upgrade ──────────────────────────────
echo "▶ Deploying with Helm..."
helm upgrade --install "$RELEASE_NAME" "$CHART_DIR" \
  -f "$VALUES_FILE" \
  --namespace "$NAMESPACE" \
  --set image.repository="${IMAGE_TAG%%:*}" \
  --set image.tag="${IMAGE_TAG##*:}" \
  --set image.pullPolicy="$PULL_POLICY" \
  --wait \
  --timeout 5m

echo "  ✓ Helm release deployed"
echo ""

# ── Step 5: Verify pods ──────────────────────────────────────
echo "▶ Verifying pods..."
kubectl get pods -n "$NAMESPACE" -l app.kubernetes.io/name=dex-ui
echo ""

# ── Step 6: Wait for rollout ─────────────────────────────────
echo "▶ Waiting for rollout..."
kubectl rollout status deployment/"$RELEASE_NAME" -n "$NAMESPACE" --timeout=3m
echo "  ✓ Rollout complete"
echo ""

# ── Step 7: Smoke test ───────────────────────────────────────
echo "▶ Running smoke test..."

# Port-forward to access the service
kubectl port-forward -n "$NAMESPACE" svc/"$RELEASE_NAME" 9091:9091 &
PF_PID=$!
sleep 3

# Health check
if curl -fsS http://localhost:9091/healthz > /dev/null 2>&1; then
  echo "  ✓ /healthz returned 200"
else
  echo "  ✗ /healthz failed (service may still be starting)"
fi

# Readiness check
if curl -fsS http://localhost:9091/readyz > /dev/null 2>&1; then
  echo "  ✓ /readyz returned 200"
else
  echo "  ✗ /readyz failed"
fi

# Metrics check
METRICS=$(curl -fsS http://localhost:9091/metrics 2>/dev/null || echo "")
if echo "$METRICS" | grep -q "dex_"; then
  echo "  ✓ /metrics returns dex_* metrics"
else
  echo "  ✗ /metrics missing dex_* metrics"
fi

# Cleanup port-forward
kill $PF_PID 2>/dev/null || true

echo ""
echo "════════════════════════════════════════════════"
echo "  Deployment complete!"
echo ""
echo "  Access the UI:"
echo "    kubectl port-forward -n $NAMESPACE svc/$RELEASE_NAME 3000:80"
echo "    open http://localhost:3000"
echo ""
echo "  Access metrics:"
echo "    kubectl port-forward -n $NAMESPACE svc/$RELEASE_NAME 9091:9091"
echo "    curl http://localhost:9091/metrics"
echo ""
echo "  Teardown:"
echo "    helm uninstall $RELEASE_NAME -n $NAMESPACE"
echo "════════════════════════════════════════════════"
