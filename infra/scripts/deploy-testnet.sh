#!/usr/bin/env bash
# ──────────────────────────────────────────────────────
# deploy-testnet.sh — Deploy to Testnet Environment
# ──────────────────────────────────────────────────────
# Usage:
#   ./deploy-testnet.sh                  # deploy latest
#   ./deploy-testnet.sh v1.2.3           # deploy specific version
#   ./deploy-testnet.sh --dry-run        # validate only
# ──────────────────────────────────────────────────────
set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly INFRA_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly NAMESPACE="dex-exchange-testnet"
readonly REGISTRY="${REGISTRY:-ghcr.io}"
readonly IMAGE_PREFIX="${IMAGE_PREFIX:-dex-exchange}"

# ─── Color output ────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[testnet]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[testnet]${NC} $*" >&2; }
log_error() { echo -e "${RED}[testnet]${NC} $*" >&2; }
log_step()  { echo -e "${CYAN}[testnet]${NC} ═══ $* ═══"; }

# ─── Configuration ───────────────────────────────────
VERSION="${1:-latest}"
DRY_RUN=false
SERVICES=(gateway matching-engine market-data risk-engine persistence)
TIMEOUT="300s"

if [[ "$VERSION" == "--dry-run" ]]; then
  DRY_RUN=true
  VERSION="latest"
  log_warn "DRY RUN MODE — no changes will be applied"
fi

# ─── Pre-flight checks ──────────────────────────────
preflight() {
  log_step "Pre-flight checks"

  if ! command -v kubectl &> /dev/null; then
    log_error "kubectl not found in PATH"
    exit 1
  fi

  if ! kubectl cluster-info &> /dev/null; then
    log_error "Cannot connect to Kubernetes cluster"
    exit 1
  fi

  # Verify testnet chain RPC is reachable
  if [[ -n "${CHAIN_RPC_URL:-}" ]]; then
    log_info "Checking chain RPC: $CHAIN_RPC_URL"
    if ! curl -sf --max-time 5 "$CHAIN_RPC_URL" > /dev/null 2>&1; then
      log_warn "Chain RPC may be unreachable: $CHAIN_RPC_URL"
    fi
  else
    log_warn "CHAIN_RPC_URL not set — chain features will be unavailable"
  fi

  if ! kubectl get namespace "$NAMESPACE" &> /dev/null; then
    log_info "Creating namespace: $NAMESPACE"
    if [[ "$DRY_RUN" == false ]]; then
      kubectl create namespace "$NAMESPACE"
    fi
  fi

  log_info "Pre-flight checks passed"
}

# ─── Apply environment config ───────────────────────
apply_config() {
  log_step "Loading testnet configuration"

  local env_file="$INFRA_DIR/config/env.testnet.env"

  if [[ ! -f "$env_file" ]]; then
    log_error "Testnet env file not found: $env_file"
    exit 1
  fi

  # Create ConfigMap from env file (non-secret values)
  if [[ "$DRY_RUN" == false ]]; then
    kubectl -n "$NAMESPACE" create configmap dex-config \
      --from-env-file="$env_file" \
      --dry-run=client -o yaml | kubectl apply -f -
  fi

  log_info "Configuration applied"
}

# ─── Apply base manifests ───────────────────────────
apply_base() {
  log_step "Applying base manifests"

  local manifests=(
    "$INFRA_DIR/k8s/namespace.yml"
    "$INFRA_DIR/k8s/network-policy.yml"
    "$INFRA_DIR/k8s/ingress.yml"
  )

  for manifest in "${manifests[@]}"; do
    log_info "Applying: $(basename "$manifest")"
    if [[ "$DRY_RUN" == true ]]; then
      kubectl apply --dry-run=client -f "$manifest"
    else
      kubectl apply -f "$manifest"
    fi
  done
}

# ─── Deploy services ────────────────────────────────
deploy_services() {
  log_step "Deploying services (version: $VERSION)"

  for svc in "${SERVICES[@]}"; do
    local manifest="$INFRA_DIR/k8s/$svc/deployment.yml"

    if [[ ! -f "$manifest" ]]; then
      log_warn "Manifest not found for $svc: $manifest"
      continue
    fi

    log_info "Deploying $svc..."

    if [[ "$DRY_RUN" == true ]]; then
      kubectl apply --dry-run=client -f "$manifest"
    else
      kubectl apply -f "$manifest" -n "$NAMESPACE"
      kubectl -n "$NAMESPACE" set image \
        "deployment/$svc" \
        "$svc=$REGISTRY/$IMAGE_PREFIX/$svc:$VERSION"
    fi
  done
}

# ─── Wait for rollout ───────────────────────────────
wait_rollout() {
  if [[ "$DRY_RUN" == true ]]; then
    log_info "Skipping rollout wait (dry run)"
    return
  fi

  log_step "Waiting for rollout completion"

  for svc in "${SERVICES[@]}"; do
    log_info "Waiting for $svc..."
    if ! kubectl -n "$NAMESPACE" rollout status \
        "deployment/$svc" --timeout="$TIMEOUT"; then
      log_error "Rollout FAILED for $svc"
      log_error "Rolling back $svc..."
      kubectl -n "$NAMESPACE" rollout undo "deployment/$svc"
      exit 1
    fi
    log_info "$svc: READY"
  done
}

# ─── Health verification ────────────────────────────
verify_health() {
  if [[ "$DRY_RUN" == true ]]; then
    log_info "Skipping health verification (dry run)"
    return
  fi

  log_step "Verifying service health"

  sleep 15  # Testnet needs more stabilization time

  local failed=0
  for svc in "${SERVICES[@]}"; do
    local ready
    ready=$(kubectl -n "$NAMESPACE" get deployment "$svc" \
      -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
    local desired
    desired=$(kubectl -n "$NAMESPACE" get deployment "$svc" \
      -o jsonpath='{.spec.replicas}' 2>/dev/null || echo "0")

    if [[ "$ready" == "$desired" && "$ready" != "0" ]]; then
      log_info "$svc: $ready/$desired pods ready ✓"
    else
      log_error "$svc: $ready/$desired pods ready ✗"
      failed=$((failed + 1))
    fi
  done

  if [[ $failed -gt 0 ]]; then
    log_error "$failed service(s) failed health check"
    exit 1
  fi

  log_info "All services healthy on testnet"
}

# ─── Main ────────────────────────────────────────────
main() {
  log_step "DEX Exchange — Testnet Deployment"
  log_info "Version: $VERSION"
  log_info "Namespace: $NAMESPACE"
  log_info "Network: testnet"

  preflight
  apply_config
  apply_base
  deploy_services
  wait_rollout
  verify_health

  log_step "Testnet Deployment COMPLETE"
  log_info "Version $VERSION deployed to $NAMESPACE"
  log_info "Chain RPC: ${CHAIN_RPC_URL:-not set}"
}

main
