# Operator Manual

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This manual provides the essential instructions for operating the distributed exchange in production environments. It covers daily monitoring, health checks, common operational tasks, and basic troubleshooting, consistent with the `INFRA-BASELINE-v1.0.md` specifications.

## 2. Daily Health Verification

Each morning or start of a shift, the on-call operator must verify system health using the designated monitoring suites.

### 2.1 Grafana Dashboards
1. **DEX Overview Dashboard** (`dex-overview.json`)
   - Ensure the aggregate error rate is <0.01%.
   - Verify HTTP 5xx responses from the API Gateway are at baseline levels.
   - Confirm active connections match expected diurnal patterns.

2. **Matching Engine Dashboard** (`matching-engine-dashboard.json`)
   - Check p99 latency is strictly under 500μs.
   - Confirm zero unhandled panics or matching anomalies.
   - Verify the order throughput supports current market volume.

### 2.2 Prometheus Alerts
- Access the `Alertmanager` interface.
- Ensure there are zero active **P0** (critical) or **P1** (high) alerts.
- Acknowledge and investigate any active **P2** or **P3** alerts based on standard operating procedures.

## 3. Configuration Management

Environment variables and configurations govern behavior like rate-limiting and timeouts.

### 3.1 Rate Limits
Rate limits are enforced by the API Gateway to prevent abuse.
- Token Bucket configurations are managed via environment files (`env.production.env`).
- Modifying limits requires a PR update to the infra repo and a rolling restart of the Gateway pods.

### 3.2 Feature Flags
Toggle system functions using the configuration loader script:
```bash
./infra/config/config-loader.sh --env production --set FLAG_MAINTENANCE_MODE=true
```
*Note: This triggers a hot reload if supported, or gracefully drains and restarts affected pods.*

## 4. Log Analysis

Logs for all components are aggregated in Loki and can be queried via Grafana Explorer or LogCLI.

### 4.1 Querying Logs (LogCLI)
```bash
logcli query '{app="matching-engine", namespace="dex-production"} |~ "ERROR"' --since=1h
```

### 4.2 Error Context
- **API Gateway (HTTP 429)**: Expected during high load; monitor for malicious IPs.
- **Order Service (Margin Failures)**: Normal user behavior; verify corresponding Risk Engine logs match.
- **Matching Engine (Any Error)**: **P0 Incident**. Raise immediately; ME errors indicate severe consistency issues.

## 5. Circuit Breakers

Services use circuit breakers to protect dependencies. 
- If a circuit breaker trip is detected in Prometheus (`circuit_breaker_state="open"`):
  1. Identify the failing dependency (e.g., Account Service).
  2. Check the logs for that dependency for OOM errors, heavy database locking, or network timeouts.
  3. The circuit breaker will transition to `half-open` automatically after 30 seconds. Monitor closely during this test phase.

## 6. Restart Procedures

Only restart services if auto-remediation (K8s) fails to clear an error state.

**Graceful Restart command:**
```bash
kubectl rollout restart deployment/<service-name> -n dex-production
```

## 7. Emergency Maintenance

If the exchange requires halting (e.g., critical bug discovery):

1. **Enable Maintenance Mode:**
   ```bash
   ./infra/scripts/toggle-maintenance.sh --enable
   ```
2. **Cancel All Pending Orders (if required):**
   ```bash
   # Utilizing admin CLI tool
   dex-admin orders cancel-all --reason "Emergency Maintenance"
   ```
3. **Notify Users** via the status page.
