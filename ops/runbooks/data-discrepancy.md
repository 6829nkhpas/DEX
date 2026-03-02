# Runbook: Data Discrepancy (Client vs Server)

**Severity**: P1 — potential financial impact, user trust issue  
**Owner**: Frontend Platform + Backend Matching Engine  
**Last updated**: 2025-07-11

---

## Symptoms

- User reports order/balance amounts don't match backend API
- Telemetry alert: `gap_detected` events with unrecoverable gaps
- Prometheus: `dex_gap_detected_total` rising
- Orderbook displayed prices diverge from REST API `/v1/orderbook`
- Account balances in UI differ from `/v1/accounts/:id/balances`

## Impact

- **Critical**: Users may make trading decisions based on incorrect data
- Stale orderbook can lead to failed orders (price moved)
- Incorrect balance display may cause withdrawal/deposit confusion
- Regulatory risk if displayed values don't match settlement

## Root Causes

| Cause                                      | Likelihood | Detection                                  |
| ------------------------------------------ | ---------- | ------------------------------------------ |
| Missed WebSocket delta (sequence gap)      | High       | `gap_detected` telemetry event             |
| Snapshot response was stale / delayed      | Medium     | Compare snapshot seq with latest delta seq |
| Client reducer bug (incorrect state merge) | Low        | Compare client state vs REST API           |
| Server publishing out-of-order events      | Low        | Check server-side sequence logs            |
| Decimal precision mismatch                 | Low        | Compare raw string values                  |

## Diagnosis

### 1. Capture traces

```bash
# Check for gap events
curl -s http://localhost:9100/events | jq '.[] | select(.type == "gap_detected")' | head -20

# Check snapshot request events
curl -s http://localhost:9100/events | jq '.[] | select(.type == "snapshot_request")' | head -20

# Check observability health
curl -s http://localhost:9100/readyz | jq .
curl -s http://localhost:9100/metrics | grep -E "gap|snapshot|sequence"
```

### 2. Compare client state vs server truth

```bash
# Get server-side orderbook
curl -s http://localhost:8080/v1/orderbook/BTC-USDC | jq '.bids[:3], .asks[:3]'

# Get server-side account balances
curl -s http://localhost:8080/v1/accounts/$ACCOUNT_ID/balances \
  -H "Authorization: Bearer $TOKEN" | jq .
```

In browser DevTools console:

```javascript
// Get client-side orderbook state
window.__DEX_DEBUG__?.store?.getOrderbook?.("BTC-USDC");

// Get client-side balances
window.__DEX_DEBUG__?.store?.getAccountState?.();

// Check sequence numbers
window.__DEX_DEBUG__?.store?.getSequenceState?.();
```

### 3. Verify decimal handling

All monetary values must be string-encoded decimals. Check for floating-point conversion:

```javascript
// In DevTools — values should be strings, not numbers
const ob = window.__DEX_DEBUG__?.store?.getOrderbook?.("BTC-USDC");
console.log(typeof ob?.bids?.[0]?.price); // Should be "string"
console.log(typeof ob?.bids?.[0]?.quantity); // Should be "string"
```

### 4. Check server-side event ordering

```bash
# Check matching engine logs for sequence issues
kubectl -n dex logs -l app=matching-engine --since=10m | grep -i "sequence\|out.of.order"

# Check persistence service for event replay status
kubectl -n dex logs -l app=persistence --since=10m | grep -i "replay\|gap"
```

## Mitigation

### Immediate (< 5 min)

1. **Force client snapshot recovery**: The most reliable fix is forcing a fresh snapshot:

   ```bash
   # Request forced snapshot via admin endpoint
   curl -X POST http://localhost:8080/admin/force-snapshot \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"stream": "orderbook", "symbol": "BTC-USDC"}'
   ```

2. **Instruct user to refresh**: A page refresh clears all client state and requests fresh snapshots from server.

3. **Verify recovery**: After snapshot, compare client vs server:
   ```bash
   # Check snapshot was received
   curl -s http://localhost:9100/events | jq '.[] | select(.type == "snapshot_request")' | tail -3
   ```

### Short-term (< 1 hour)

1. **Audit sequence continuity**:

   ```bash
   # Check for persistent gaps
   curl -s http://localhost:9100/events | \
     jq '[.[] | select(.type == "gap_detected")] | length'
   ```

2. **Check if issue is widespread**: Query telemetry for multiple clients reporting gaps:

   ```bash
   # Aggregate gap events by client
   curl -s http://localhost:9100/events | \
     jq '[.[] | select(.type == "gap_detected")] | group_by(.data.clientId) | .[] | {client: .[0].data.clientId, gaps: length}'
   ```

3. **If server-side issue**: Check matching engine and persistence service logs for event publishing errors.

## Rollback

If discrepancy was introduced by a frontend deploy:

```bash
# Check recent frontend deploys
git log --oneline -10 -- apps/web-ui/

# Revert to last known good build
# (CDN rollback depends on deployment system)
```

If server-side event publishing issue:

```bash
# Rollback matching engine
kubectl -n dex rollout undo deployment/matching-engine

# Rollback persistence service
kubectl -n dex rollout undo deployment/persistence

# Force full snapshot broadcast
curl -X POST http://localhost:8080/admin/force-snapshot \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"stream": "all"}'
```

## Prevention

- Per-stream sequence tracking with automatic gap detection
- Automatic `snapshot_since` recovery when gaps are detected
- All monetary values stored and transmitted as string-encoded decimals (never float)
- Pure deterministic reducers — no side effects in state transitions
- Dedup guard prevents double-processing of events
- E2E reconciliation tests compare client state vs REST API

## Escalation

Data discrepancies are **P1** by default:

1. **Immediately**: Notify trading operations team
2. **Within 5 minutes**: Force snapshots for all affected streams
3. **Within 15 minutes**: If persistent, page backend on-call
4. **Within 30 minutes**: If widespread, consider enabling maintenance mode
5. **Post-incident**: Full audit of event sequence logs from persistence service
