# Runbook: Incident Playbook

**Severity**: Varies (P1–P3)  
**Owner**: On-Call Engineer  
**Last updated**: 2025-07-18

---

## Incident Classification

| Priority | Impact                               | Response Time | Example                                         |
| -------- | ------------------------------------ | ------------- | ----------------------------------------------- |
| P1       | Service down, data loss risk         | 5 min         | WS gateway unreachable, order submission broken |
| P2       | Degraded performance, partial outage | 15 min        | High latency, buffer overflow, stale data       |
| P3       | Minor issue, no user impact          | 1 hour        | Elevated error rate, telemetry gap              |

## Incident Response Flow

```
1. DETECT → Alert fires or user reports issue
2. TRIAGE → Classify severity, assign on-call
3. DIAGNOSE → Identify root cause using indicators below
4. MITIGATE → Apply immediate fix (rollback, scale, restart)
5. RESOLVE → Deploy permanent fix
6. POST-MORTEM → Document and improve
```

## Common Scenarios & Quick Reference

### Scenario 1: WebSocket Disconnections

**Indicators:**

```promql
rate(dex_gaps_detected_total[5m]) > 0.1
dex_connected_clients == 0
```

**Immediate steps:**

1. Check gateway pods: `kubectl -n production get pods -l app=gateway`
2. Check gateway logs: `kubectl -n production logs -l app=gateway --tail=50`
3. Restart if needed: `kubectl -n production rollout restart deployment/gateway`

**Runbook:** [reconnect-storm.md](reconnect-storm.md)

### Scenario 2: Data Discrepancy

**Indicators:**

```promql
rate(dex_gaps_detected_total[5m]) > 0
dex_buffer_size_total > 5000
```

**Immediate steps:**

1. Force snapshot recovery for affected streams
2. Verify client state against REST API
3. Check matching engine sequence logs

**Runbook:** [data-discrepancy.md](data-discrepancy.md)

### Scenario 3: Performance Degradation

**Indicators:**

```promql
histogram_quantile(0.95, rate(dex_event_to_store_latency_seconds_bucket[5m])) > 0.3
process_heap_bytes / 1024 / 1024 > 400
```

**Immediate steps:**

1. Check pod resource usage: `kubectl -n production top pods -l app.kubernetes.io/name=dex-ui`
2. Scale up if CPU-bound: `kubectl -n production scale deployment/dex-ui --replicas=5`
3. Check for memory leaks via heap snapshots

**Runbook:** [buffer-overflow.md](buffer-overflow.md)

### Scenario 4: Deployment Failure

**Indicators:**

- Pod crash loops: `kubectl -n production get pods | grep CrashLoopBackOff`
- Health check failures after deploy

**Immediate steps:**

1. Rollback immediately: `helm rollback dex-ui 0 -n production`
2. Check logs: `kubectl -n production logs -l app.kubernetes.io/name=dex-ui --previous`

**Runbook:** [deploy-rollback.md](deploy-rollback.md)

## Diagnosis Commands

```bash
# ── Cluster Overview ──────────────────────────────────────────
kubectl -n production get pods -o wide
kubectl -n production get events --sort-by='.lastTimestamp' | tail -30
kubectl -n production top pods

# ── Application Health ────────────────────────────────────────
# Port-forward and check endpoints
kubectl -n production port-forward svc/dex-ui 9091:9091 &
curl -s http://localhost:9091/healthz | jq .
curl -s http://localhost:9091/readyz | jq .
curl -s http://localhost:9091/metrics | grep -E "dex_(gaps|buffer|connected)"

# ── Logs ──────────────────────────────────────────────────────
kubectl -n production logs deploy/dex-ui --tail=100 --since=10m
kubectl -n production logs deploy/dex-ui --previous  # crashed container

# ── Resource Pressure ────────────────────────────────────────
kubectl -n production describe node | grep -A5 "Allocated resources"
kubectl -n production get hpa
```

## Escalation Matrix

| Time   | Action                                    |
| ------ | ----------------------------------------- |
| 0 min  | On-call engineer paged, begins triage     |
| 5 min  | P1: Escalate to team lead if no diagnosis |
| 15 min | P1: Escalate to engineering manager       |
| 30 min | P1: Status page updated, exec notified    |
| 1 hour | P1: All-hands war room if unresolved      |

## Post-Incident

1. **Within 24h**: Write incident summary (what, when, impact, resolution)
2. **Within 48h**: Full post-mortem with root cause analysis
3. **Within 1 week**: Implement preventive measures from action items
4. **Template**: Use the incident template in `docs/07-incident-response.md`

## Communication

- **Internal**: `#dex-incidents` Slack channel
- **Status page**: Update at `status.dex.example.com`
- **Stakeholders**: Email if user-facing impact > 5 minutes
