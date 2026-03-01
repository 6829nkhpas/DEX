# Runbook: Delta Buffer Overflow

**Severity**: P2 — data consistency risk, degraded performance  
**Owner**: Frontend Platform  
**Last updated**: 2025-07-11

---

## Symptoms

- Telemetry alert: `buffer_overflow` events detected
- Prometheus metric `dex_buffer_overflow_total` incrementing
- Client memory usage climbing steadily (> 200 MB)
- Browser tab becoming sluggish or unresponsive
- Observability `/metrics` shows `dex_buffer_size` near 10000

## Impact

- Oldest deltas evicted from bounded buffer (10,000 cap)
- Gap recovery may fail if evicted deltas are needed for replay
- Potential stale orderbook / position data displayed to users
- Browser memory pressure can crash the tab

## Root Causes

| Cause                                               | Likelihood | Detection                                 |
| --------------------------------------------------- | ---------- | ----------------------------------------- |
| Market event storm (high volatility)                | High       | Check `dex_buffer_size` metric trend      |
| Client left open overnight without interaction      | Medium     | Check session duration in telemetry       |
| Subscription fanout too wide (many symbols)         | Medium     | Check `subscription_count` telemetry      |
| Gap recovery loop (requesting snapshots repeatedly) | Low        | Check `snapshot_request` events frequency |

## Diagnosis

### 1. Capture traces

```bash
# Check buffer overflow events
curl -s http://localhost:9100/events | jq '.[] | select(.type == "buffer_overflow")' | head -20

# Check current buffer sizes via metrics
curl -s http://localhost:9100/metrics | grep buffer

# Check subscription count
curl -s http://localhost:9100/events | jq '.[] | select(.type == "subscription_count")' | tail -5
```

### 2. Identify which stream is overflowing

```bash
# Group buffer overflow events by stream
curl -s http://localhost:9100/events | \
  jq '[.[] | select(.type == "buffer_overflow")] | group_by(.data.stream) | .[] | {stream: .[0].data.stream, count: length}'
```

### 3. Check client-side state

In browser DevTools console:

```javascript
// Check store buffer sizes
window.__DEX_DEBUG__?.store?.getBufferSizes?.();

// Check active subscriptions
window.__DEX_DEBUG__?.subscriptionOrchestrator?.getActiveSubscriptions?.();
```

### 4. Check memory consumption

In browser DevTools → Memory tab:

- Take heap snapshot
- Search for `DexStateStore` to check retained size
- Check if dedup set (`seenIds`) is at 10,000 cap

## Mitigation

### Immediate (< 5 min)

1. **Reduce subscription fanout**: If user has many symbols open, the aggregated feed manager should be throttling. Verify:

   ```bash
   curl -s http://localhost:9100/events | jq '.[] | select(.type == "subscription_count")' | tail -1
   ```

2. **Force client to request fresh snapshot**: The buffer overflow handler automatically triggers a snapshot request. Verify it succeeded:

   ```bash
   curl -s http://localhost:9100/events | jq '.[] | select(.type == "snapshot_request")' | tail -5
   ```

3. **If persistent, request forced snapshot via admin endpoint**:
   ```bash
   curl -X POST http://localhost:8080/admin/force-snapshot \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"stream": "orderbook", "symbol": "BTC-USDC"}'
   ```

### Short-term (< 1 hour)

1. Check if the high event rate is legitimate (market volatility) or anomalous:

   ```bash
   # Check matching engine event rate
   kubectl -n dex logs -l app=matching-engine --tail=50 | grep "events/sec"
   ```

2. If event rate is anomalous, investigate matching engine:

   ```bash
   kubectl -n dex logs -l app=matching-engine --since=15m | grep -i "error\|warn"
   ```

3. Consider temporarily switching affected clients to aggregated feed mode (lower granularity, fewer events).

## Rollback

Buffer overflow is a client-side state issue — no deployment rollback needed. To reset:

1. Client can refresh the page (clears all in-memory state, requests fresh snapshots)
2. If server-side issue, roll back matching engine:
   ```bash
   kubectl -n dex rollout undo deployment/matching-engine
   ```

## Prevention

- Bounded delta buffers (10,000 cap) prevent unbounded memory growth
- Dedup guard (10,000 seenIds) prevents duplicate processing
- `buffer_overflow` telemetry event fires when eviction occurs — set alert threshold
- AggregatedFeedManager automatically batches updates for multi-symbol views
- Consider adding a "stale data" banner when buffer overflow is detected

## Escalation

If buffer overflow is persistent across many clients:

1. Check matching engine event throughput — may need rate limiting on the server
2. Page backend on-call to investigate event source
3. Consider enabling server-side delta compression
