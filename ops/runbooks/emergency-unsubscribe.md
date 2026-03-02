# Runbook: Emergency Unsubscribe

**Severity**: P1 — resource exhaustion, potential cascading failure  
**Owner**: Frontend Platform  
**Last updated**: 2025-07-11

---

## Symptoms

- Client CPU spiking (> 80%) — `cpu_warning` telemetry events firing
- Telemetry: `subscription_count` exceeds safe threshold (> 20 symbols)
- Browser tab unresponsive or extremely slow
- Memory usage climbing rapidly (> 500 MB)
- User reports: "everything is lagging", "page won't respond"
- Prometheus: `dex_subscription_count` abnormally high

## Impact

- Client-side resource exhaustion degrades entire browser session
- Excessive subscriptions increase backend WS gateway load
- Buffer overflow cascade: too many delta streams fill buffers simultaneously
- May trigger reconnect storm if WS connection drops under load

## Root Causes

| Cause                                                    | Likelihood | Detection                                |
| -------------------------------------------------------- | ---------- | ---------------------------------------- |
| User opened many market tabs / watchlists                | High       | Check `subscription_count` telemetry     |
| Subscription leak — component unmount didn't unsubscribe | Medium     | Check for orphaned subscriptions         |
| AggregatedFeedManager not batching properly              | Low        | Check aggregated vs individual sub count |
| Runaway subscription loop (code bug)                     | Low        | Check for rapid subscribe events in logs |

## Diagnosis

### 1. Capture traces

```bash
# Check subscription count trend
curl -s http://localhost:9100/events | jq '.[] | select(.type == "subscription_count")' | tail -10

# Check CPU warnings
curl -s http://localhost:9100/events | jq '.[] | select(.type == "cpu_warning")' | head -10

# Check buffer sizes
curl -s http://localhost:9100/metrics | grep -E "subscription|buffer|cpu"

# Check readiness
curl -s http://localhost:9100/readyz | jq .
```

### 2. Identify subscription inventory

In browser DevTools console:

```javascript
// List all active subscriptions
window.__DEX_DEBUG__?.subscriptionOrchestrator?.getActiveSubscriptions?.();

// Count subscriptions
window.__DEX_DEBUG__?.subscriptionOrchestrator?.getActiveSubscriptions?.()
  ?.length;

// Check AggregatedFeedManager state
window.__DEX_DEBUG__?.aggregatedFeedManager?.getState?.();
```

### 3. Check for subscription leaks

```bash
# Look for rapid subscribe/unsubscribe patterns
curl -s http://localhost:9100/events | \
  jq '[.[] | select(.type == "subscription_count")] |
      [.[-5:][].data.count] |
      {counts: ., rising: (.[0] < .[-1])}'
```

In DevTools → Performance tab:

- Record a 10-second trace
- Check for frequent WebSocket message handlers
- Look for components re-rendering excessively

## Mitigation

### Immediate (< 2 min)

1. **Emergency unsubscribe all non-essential streams**:

   In browser DevTools console:

   ```javascript
   // Unsubscribe everything except the current view
   const currentSymbol = window.__DEX_DEBUG__?.currentSymbol || "BTC-USDC";
   window.__DEX_DEBUG__?.subscriptionOrchestrator?.unsubscribeAll?.();

   // Re-subscribe only to the essential stream
   window.__DEX_DEBUG__?.subscriptionOrchestrator?.subscribe?.({
     stream: "orderbook",
     symbol: currentSymbol,
   });
   ```

2. **If DevTools are unresponsive**, force-close the tab and reopen. Fresh page load starts with minimal subscriptions.

3. **Server-side emergency**: Kill specific client connections from admin:

   ```bash
   # List connected clients
   curl -s http://localhost:8080/admin/connections \
     -H "Authorization: Bearer $ADMIN_TOKEN" | jq '.[] | {id, subscriptions: (.subs | length)}'

   # Force disconnect a specific overloaded client
   curl -X DELETE http://localhost:8080/admin/connections/$CLIENT_ID \
     -H "Authorization: Bearer $ADMIN_TOKEN"
   ```

### Short-term (< 30 min)

1. **Identify the source of excessive subscriptions**:

   ```bash
   # Check if it's a single user or widespread
   curl -s http://localhost:9100/events | \
     jq '[.[] | select(.type == "subscription_count" and .data.count > 15)] | length'
   ```

2. **Check for subscription leak in code**: If subscriptions grow without user action, there's likely a component lifecycle bug:

   ```bash
   # Search for subscribe calls without matching unsubscribe
   grep -rn "subscribe\|unsubscribe" apps/web-ui/src/ --include="*.ts" --include="*.tsx"
   ```

3. **Request forced snapshot** for remaining subscriptions to ensure clean state:
   ```bash
   curl -X POST http://localhost:8080/admin/force-snapshot \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"stream": "all"}'
   ```

## Rollback

If caused by a recent frontend deploy with subscription leak:

```bash
# Identify the bad commit
git log --oneline -10 -- apps/web-ui/src/

# Revert the deployment
# (CDN/deployment system specific)

# Force-disconnect all clients to clean up server-side subscription state
curl -X POST http://localhost:8080/admin/disconnect-all \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

## Prevention

- **SubscriptionOrchestrator** manages subscribe/unsubscribe lifecycle
- **AggregatedFeedManager** batches multi-symbol subscriptions
- Subscription count tracked via telemetry with alerting threshold
- React `useEffect` cleanup must always unsubscribe on unmount
- Hard cap on maximum concurrent subscriptions (configurable, default 50)
- CPU warning telemetry fires early (> 70% usage) for proactive intervention
- Consider implementing server-side per-client subscription limits

## Escalation

1. **Immediate**: If affecting multiple clients, page frontend on-call
2. **Within 5 minutes**: Check if WS gateway is under stress from subscription load
3. **Within 15 minutes**: Consider rate-limiting new subscriptions at gateway level
4. **Post-incident**: Audit all subscription lifecycle code for leaks
