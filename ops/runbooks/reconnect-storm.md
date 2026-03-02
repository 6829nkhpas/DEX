# Runbook: WebSocket Reconnect Storm

**Severity**: P1 — service degradation, potential thundering herd  
**Owner**: Frontend Platform  
**Last updated**: 2025-07-11

---

## Symptoms

- Monitoring alert: `ws_reconnect` telemetry events spike above 50/min
- Prometheus metric `dex_ws_reconnect_total` rising steeply
- User reports: "prices frozen", "order status stale"
- Observability `/readyz` returns `{ ready: false }` with `websocket: "disconnected"`

## Impact

- Users see stale data and cannot place orders through the UI
- If many clients reconnect simultaneously, backend WS gateway may become overloaded
- Gap recovery requests (`snapshot_since`) spike, increasing backend load

## Root Causes

| Cause                                        | Likelihood | Detection                                  |
| -------------------------------------------- | ---------- | ------------------------------------------ |
| Backend WS gateway restart / deploy          | High       | Check k8s events, gateway pod restarts     |
| Network partition between CDN and cluster    | Medium     | Check ingress logs, ping latency           |
| Client-side exponential backoff exhausted    | Low        | Check `ws_reconnect` event `attempt` field |
| TLS certificate rotation dropped connections | Low        | Check cert-manager logs                    |

## Diagnosis

### 1. Capture traces

```bash
# Check telemetry events for reconnect pattern
curl -s http://localhost:9100/events | jq '.[] | select(.type == "ws_reconnect")' | head -50

# Check observability endpoint
curl -s http://localhost:9100/readyz | jq .

# Check Prometheus metrics
curl -s http://localhost:9100/metrics | grep ws_reconnect

# Check gateway pod status
kubectl -n dex get pods -l app=gateway -o wide
kubectl -n dex logs -l app=gateway --tail=100 --since=5m
```

### 2. Verify backend WS gateway health

```bash
# Check WS gateway readiness
kubectl -n dex exec -it deploy/gateway -- curl -s http://localhost:8080/healthz

# Check recent events
kubectl -n dex get events --sort-by='.lastTimestamp' | grep gateway | tail -20
```

### 3. Check client backoff state

In browser DevTools console:

```javascript
// Check current backoff delay
window.__DEX_DEBUG__?.wsClient?.getBackoffState?.();

// Check circuit breaker state
window.__DEX_DEBUG__?.circuitBreaker?.getSnapshot?.();
```

## Mitigation

### Immediate (< 5 min)

1. **If backend gateway is down**: Restart gateway pods

   ```bash
   kubectl -n dex rollout restart deployment/gateway
   ```

2. **If thundering herd**: Scale gateway horizontally

   ```bash
   kubectl -n dex scale deployment/gateway --replicas=5
   ```

3. **Force client snapshot recovery**: Clients will auto-recover via gap detection.
   No manual intervention needed — the `snapshot_since` mechanism handles stale state.

### Short-term (< 1 hour)

1. Review gateway logs for root cause:

   ```bash
   kubectl -n dex logs -l app=gateway --since=30m | grep -i "error\|panic\|close"
   ```

2. Verify sequence continuity after recovery:

   ```bash
   curl -s http://localhost:9100/events | jq '[.[] | select(.type == "gap_detected")] | length'
   ```

3. If gaps persist, request forced snapshot via admin endpoint:
   ```bash
   curl -X POST http://localhost:8080/admin/force-snapshot \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"stream": "orderbook", "symbol": "BTC-USDC"}'
   ```

## Rollback

If the storm was caused by a recent deployment:

```bash
# Check recent deployments
kubectl -n dex rollout history deployment/gateway

# Rollback to previous revision
kubectl -n dex rollout undo deployment/gateway

# Verify rollback
kubectl -n dex rollout status deployment/gateway
```

## Prevention

- Client exponential backoff with jitter (500ms → 16s cap, ±20%) prevents thundering herd
- Circuit breaker trips after 5 consecutive failures, providing 30s cooldown
- Maximum reconnect attempts tracked in telemetry for alerting
- Consider implementing server-side connection rate limiting on the WS gateway

## Escalation

If not resolved within 15 minutes:

1. Page on-call backend engineer
2. Check if issue is infrastructure-wide (other services affected)
3. Consider enabling maintenance mode banner in UI
