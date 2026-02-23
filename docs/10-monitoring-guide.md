# Monitoring Guide

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This guide defines the observability stack and operational metrics used to monitor the high-performance decentralized exchange, aligned with Spec 10 limits and the `INFRA-BASELINE`.

## 2. Core Observability Stack

The entire stack is configured in `/infra/monitoring/`:
- **Metrics**: Prometheus (`v2.51.0`) scraped every 5-15s.
- **Logs**: Promtail + Loki (`2.9.6`).
- **Visualization**: Grafana (`10.4.0`) with provisioned dashboards.

## 3. Key Dashboards

### 3.1 DEX Overview (`dex-overview.json`)
Monitors the global health of the exchange.
* **API Ingress**: HTTP 2xx/4xx/5xx rates. Look for 5xx > 0.01%.
* **Order Flow**: Submitted, Accepted, Rejected rates.
* **Database Connections**: Active, Idle, Max used. 
* **Active Replicas**: Desired vs Running pod counts.

### 3.2 Matching Engine (`matching-engine-dashboard.json`)
Deep dive into the HOT PATH.
* **Match Latency**: Histogram (p50, p90, p99). Ensure p99 < 500μs.
* **Order Book Depth**: Bid/Ask spread and liquidity density per symbol.
* **Event Sequence Lag**: Emitted sequence minus settled sequence. Lag > 1000 is an alert.
* **Memory Usage**: Rust arena allocations. Alert > 80% usage.

## 4. Alerting Rules

Defined in `alert-rules.yml`, routed via Alertmanager to PagerDuty/Slack.

### 4.1 P0 Alerts (Critical - Page immediately)
- **Financial Invariant Breach**: Calculated balances do not match on-chain settlements.
- **ME Panic**: Matching Engine instance crashed.
- **Split Brain Detected**: Multiple instances claiming Primary role.
- **Database Unreachable**: Connection pool exhausted or DB down.

### 4.2 P1 Alerts (High - Page immediately)
- **Latency Violation**: Matching latency p99 > 5ms for > 1 minute.
- **Event Processor Lag**: Downstream lag > 5000 events.
- **High Error Rate**: Gateway 5xx rate > 1% for 5 minutes.

### 4.3 P2 Alerts (Medium - Ticket/Email)
- **Pod CrashLoopBackOff**: Non-critical replica failing to start (e.g., secondary API node).
- **High CPU Utilization**: Specific pod nearing its hard limit (HPA failing to scale).
- **Disk Nearing Capacity**: Persistence PVC at 80% usage.

## 5. Log Searching

Loki is used for all log aggregation. Ensure queries are filtered by Namespace, App, and Level.

```logql
# Find slow matches in the last hour
{app="matching-engine", namespace="dex-production", level="warn"} |= "SLOW_MATCH"
```
```logql
# Track a specific user's order lifecycle across microservices
{namespace="dex-production"} |= "req_abc123"
```
