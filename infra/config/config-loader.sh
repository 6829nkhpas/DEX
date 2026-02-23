#!/usr/bin/env bash
# ──────────────────────────────────────────────────────
# config-loader.sh — Environment Config Loader
# ──────────────────────────────────────────────────────
# Validates required environment variables at container
# startup and fails fast if any are missing.
#
# Usage:
#   source config-loader.sh <env-file>
#   ./config-loader.sh --validate <env-file>
# ──────────────────────────────────────────────────────
set -euo pipefail

readonly SCRIPT_NAME="$(basename "$0")"

# ─── Color output ────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[${SCRIPT_NAME}]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[${SCRIPT_NAME}]${NC} $*" >&2; }
log_error() { echo -e "${RED}[${SCRIPT_NAME}]${NC} $*" >&2; }

# ─── Required variables per service ──────────────────
declare -A REQUIRED_VARS
REQUIRED_VARS=(
  [gateway]="GATEWAY_HOST GATEWAY_PORT GATEWAY_JWT_SECRET REDIS_URL"
  [matching-engine]="MATCHING_ENGINE_HOST MATCHING_ENGINE_PORT"
  [market-data]="MARKET_DATA_HOST MARKET_DATA_PORT"
  [risk-engine]="RISK_ENGINE_HOST RISK_ENGINE_PORT"
  [persistence]="PERSISTENCE_HOST PERSISTENCE_PORT"
)

# ─── Global required variables ───────────────────────
GLOBAL_REQUIRED="ENVIRONMENT LOG_LEVEL"

# ─── Functions ───────────────────────────────────────

load_env_file() {
  local env_file="$1"

  if [[ ! -f "$env_file" ]]; then
    log_error "Environment file not found: $env_file"
    return 1
  fi

  log_info "Loading environment from: $env_file"

  while IFS= read -r line || [[ -n "$line" ]]; do
    # Skip empty lines and comments
    [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue

    # Strip inline comments
    line="${line%%#*}"
    line="${line%"${line##*[![:space:]]}"}"

    # Export the variable
    if [[ "$line" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]]; then
      export "${line?}"
    fi
  done < "$env_file"

  log_info "Environment loaded successfully"
}

validate_required() {
  local service_name="${SERVICE_NAME:-unknown}"
  local missing=0

  log_info "Validating config for service: $service_name"

  # Check global required vars
  for var in $GLOBAL_REQUIRED; do
    if [[ -z "${!var:-}" ]]; then
      log_error "Missing required global variable: $var"
      missing=$((missing + 1))
    fi
  done

  # Check service-specific required vars
  local service_vars="${REQUIRED_VARS[$service_name]:-}"
  if [[ -n "$service_vars" ]]; then
    for var in $service_vars; do
      if [[ -z "${!var:-}" ]]; then
        log_error "Missing required variable for $service_name: $var"
        missing=$((missing + 1))
      fi
    done
  else
    log_warn "No required variables defined for service: $service_name"
  fi

  if [[ $missing -gt 0 ]]; then
    log_error "$missing required variable(s) missing — aborting"
    return 1
  fi

  log_info "All required variables present"
  return 0
}

resolve_secrets() {
  log_info "Resolving secret placeholders..."

  local resolved=0
  local unresolved=0

  # Find vars with ${...} placeholders and check if they're set
  while IFS='=' read -r key value; do
    if [[ "$value" =~ \$\{([^}]+)\} ]]; then
      local secret_name="${BASH_REMATCH[1]}"
      if [[ -n "${!secret_name:-}" ]]; then
        export "$key=${!secret_name}"
        resolved=$((resolved + 1))
      else
        log_warn "Unresolved secret: \${$secret_name} for $key"
        unresolved=$((unresolved + 1))
      fi
    fi
  done < <(env | grep -E '=\$\{')

  log_info "Secrets resolved: $resolved, unresolved: $unresolved"

  if [[ $unresolved -gt 0 ]]; then
    log_warn "$unresolved secret(s) remain unresolved"
  fi
}

print_config_summary() {
  log_info "──────── Configuration Summary ────────"
  log_info "ENVIRONMENT:  ${ENVIRONMENT:-unset}"
  log_info "SERVICE_NAME: ${SERVICE_NAME:-unset}"
  log_info "LOG_LEVEL:    ${LOG_LEVEL:-unset}"
  log_info "METRICS:      ${METRICS_ENABLED:-false}"
  log_info "───────────────────────────────────────"
}

# ─── Main ────────────────────────────────────────────

main() {
  local mode="${1:---help}"

  case "$mode" in
    --validate)
      local env_file="${2:?Usage: $SCRIPT_NAME --validate <env-file>}"
      load_env_file "$env_file"
      resolve_secrets
      validate_required
      print_config_summary
      ;;
    --load)
      local env_file="${2:?Usage: $SCRIPT_NAME --load <env-file>}"
      load_env_file "$env_file"
      resolve_secrets
      validate_required
      print_config_summary
      ;;
    --help)
      echo "Usage:"
      echo "  $SCRIPT_NAME --validate <env-file>  Validate config without starting service"
      echo "  $SCRIPT_NAME --load <env-file>       Load config and export variables"
      echo "  source $SCRIPT_NAME                  Source directly for shell usage"
      ;;
    *)
      log_error "Unknown mode: $mode"
      exit 1
      ;;
  esac
}

# Run main only if not sourced
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main "$@"
fi
