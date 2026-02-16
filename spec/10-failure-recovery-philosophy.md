# Failure Recovery Philosophy Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the failure recovery philosophy for the distributed exchange, ensuring system resilience and data integrity under all failure scenarios.

## 2. Core Principles

### 2.1 Design Tenets

1. **Fail Fast**: Detect failures immediately, don't hide them
2. **Fail Safe**: Never corrupt data, even if service unavailable
3. **Idempotent**: All operations can be safely retried
4. **Auditable**: All state changes logged immutably
5. **Recoverable**: System can restore from any failure point

### 2.2 Priorities (in order)

1. **Data Integrity**: Never lose or corrupt user funds/positions
2. **Consistency**: Never create inconsistent state
3. **Availability**: Minimize downtime (but not at cost of integrity)
4. **Performance**: Optimize only after above guaranteed

## 3. Failure Categories

### 3.1 Transient Failures

**Definition**: Temporary issues that resolve quickly  
**Examples**:
- Network timeouts
- Database connection pool exhaustion
- External service unavailable (< 30s)

**Strategy**: Retry with exponential backoff
```
Retry intervals: 100ms, 200ms, 400ms, 800ms, 1.6s, 3.2s
Max retries: 6
Total time: ~6.3 seconds
```

### 3.2 Persistent Failures

**Definition**: Issues lasting > 30 seconds  
**Examples**:
- Database crash
- Matching engine down
- Network partition

**Strategy**: Circuit breaker + degraded mode
```
Circuit breaker: OPEN after 5 failures in 10s window
Degraded mode: Queue requests or return cached data
Recovery: Half-open after 30s, close after 3 successes
```

### 3.3 Catastrophic Failures

**Definition**: Data center loss, multi-region failure  
**Examples**:
- AWS region outage
- Database corruption
- Cyber attack

**Strategy**: Disaster recovery
```
RTO: 5 minutes (fail over to backup region)
RPO: 0 (zero data loss via synchronous replication)
Failback: Manual, after root cause analysis
```

## 4. Crash Recovery

### 4.1 Service Crash

**Detection**: Health check failure (3 consecutive failures)  
**Action**:
1. Orchestrator (Kubernetes) restarts service
2. Service reads last committed state from database
3. Service replays events from last checkpoint
4. Service resumes normal operation

**Guarantees**: No lost requests (queued/persistent)

### 4.2 Database Crash

**Detection**: Connection failures  
**Action**:
1. All services fail health checks (can't query DB)
2. Load balancer redirects to healthy replicas
3. If all replicas down, promote read replica to primary
4. Replay WAL (Write-Ahead Log) to recover

**Guarantees**: No data loss (WAL guarantees durability)

### 4.3 Matching Engine Crash

**Special Case**: Matching engine is stateful (in-memory order book)

**Recovery**:
1. Reload order book from database (all PENDING/PARTIAL orders)
2. Sort orders by price-time priority
3. Replay unprocessed matches from event log
4. Resume matching

**Downtime**: < 10 seconds (depends on order book size)

## 5. Split Brain Prevention

**Problem**: Network partition causes two primaries

**Detection**:
- Consensus protocol (Raft/Paxos)
- Fencing tokens (monotonic lease IDs)
- Quorum-based writes (majority acknowledgment)

**Prevention**:
```
IF cannot reach quorum (> 50% nodes) THEN
  refuse writes, serve reads only (degraded mode)
END
```

**Resolution**: Automatic (partition heals, quorum restored)

## 6. State Recovery

### 6.1 Event Sourcing

**Pattern**: All state derived from immutable events

**Recovery Process**:
```
1. Read all events from event store (ordered by sequence)
2. Apply events to empty state (replay)
3. Verify final state matches expected checksums
4. Mark recovery complete
```

**Validation**: Periodic full replay to verify event log integrity

### 6.2 Snapshots

**Purpose**: Speed up recovery (avoid replaying millions of events)

**Strategy**:
```
- Create snapshot every 1 million events
- Store: snapshot state + event_sequence_number
- Recovery: Load latest snapshot + replay events since snapshot
```

**Example**:
```
Snapshot at sequence 5,000,000 (account balances, positions)
Current sequence: 5,100,000
Recovery: Load snapshot + replay events 5,000,001 to 5,100,000
```

### 6.3 Checkpoints

**Frequency**: Every 60 seconds  
**Content**: Service state + last processed event sequence  
**Purpose**: Fast restart without full replay

## 7. Graceful Degradation

### 7.1 Read-Only Mode

**When**: Database primary down, replicas available  
**Behavior**:
- Serve all GET requests (from replicas)
- Reject all POST/PUT/DELETE requests (503 Service Unavailable)
- Display warning banner to users

### 7.2 Order Queuing

**When**: Matching engine unavailable  
**Behavior**:
- Accept orders, persist to database
- Return "PENDING_SUBMISSION" status
- Process queue when matching engine recovers

**Guarantee**: No lost orders

### 7.3 Stale Market Data

**When**: Market data service down  
**Behavior**:
- Serve last known market data
- Display timestamp of last update
- If data > 60s old, return 503

## 8. Data Integrity Safeguards

### 8.1 Checksums

**Usage**:
- Event log entries (detect corruption)
- Database pages (detect bit rot)
- Network messages (detect transmission errors)

**Algorithm**: CRC32C (hardware-accelerated)

### 8.2 Invariant Validation

**Timing**: After every state change  
**Checks**:
```
- Total balances = sum of all accounts + fees
- Position size × entry price = margin × leverage
- Order filled + remaining = total quantity
```

**Violation**: PANIC, halt trading, alert on-call

### 8.3 Write-Ahead Logging (WAL)

**Process**:
```
1. Write change to WAL (persistent storage)
2. Acknowledge write
3. Apply change to in-memory state (async)
```

**Guarantee**: Durability even if crash before applying

## 9. Idempotency

### 9.1 Idempotency Keys

**Pattern**: Every request includes unique idempotency key

**Example**:
```
POST /v1/orders
{
  "idempotency_key": "req_abc123",
  "symbol": "BTC/USDT",
  "quantity": "1.0",
  ...
}
```

**Behavior**:
- First request: Process normally
- Duplicate request: Return cached response (no-op)

**Storage**: Idempotency key → response mapping (TTL: 24 hours)

### 9.2 Natural Idempotency

**Examples**:
- Settlement by trade_id (can settle same trade multiple times = no-op)
- Cancel order by order_id (can cancel canceled order = no-op)
- Update balance by transaction_id (same transaction_id = no-op)

**Implementation**: Upsert with unique constraint

## 10. Retry Logic

### 10.1 Exponential Backoff

```
def retry(operation, max_attempts=6):
  for attempt in range(max_attempts):
    try:
      return operation()
    except TransientError as e:
      if attempt == max_attempts - 1:
        raise
      sleep(100ms * 2^attempt + random_jitter(50ms))
```

### 10.2 Jitter

**Purpose**: Prevent thundering herd  
**Implementation**: Add random(0, 50ms) to each retry delay

### 10.3 Timeout Policy

| Operation | Timeout |
|-----------|---------|
| Database query | 5s |
| RPC call (internal) | 3s |
| RPC call (external) | 10s |
| HTTP request | 30s |

**After timeout**: Retry or fail fast (context-dependent)

## 11. Monitoring for Recovery

### 11.1 Health Indicators

**Green** (Healthy):
- All services responding
- Database replication lag < 1s
- Event processing lag < 1s

**Yellow** (Degraded):
- Some services in read-only mode
- Replication lag 1-10s
- Non-critical service down

**Red** (Unhealthy):
- Critical service down (matching engine, settlement)
- Replication lag > 10s
- Data integrity violation detected

### 11.2 Automatic Remediation

| Issue | Auto-Remediation |
|-------|------------------|
| Service crash | Restart (k8s) |
| Memory leak | Restart after 80% memory used |
| DB connection leak | Close idle connections > 5 min |
| Disk full | Archive old logs |

### 11.3 Alerting

**Severity Levels**:
- P0 (Critical): Page on-call immediately
- P1 (High): Email + Slack ping
- P2 (Medium): Email only
- P3 (Low): Dashboard only

## 12. Testing Recovery

### 12.1 Chaos Engineering

**Regular Tests** (monthly):
- Kill random service instance
- Introduce network latency (100-500ms)
- Fill disk to 95%
- Corrupt random database page

**Annual Tests**:
- Multi-AZ failure simulation
- Full region failover drill

### 12.2 Disaster Recovery Drill

**Frequency**: Quarterly  
**Process**:
1. Announce drill (scheduled downtime)
2. Shut down primary region
3. Fail over to DR region
4. Verify all services operational
5. Test critical user flows
6. Fail back to primary
7. Post-mortem and improvements

## 13. Recovery Time Objectives

| Failure Type | RTO | RPO |
|--------------|-----|-----|
| Single service crash | 30s | 0 |
| Database failover | 60s | 0 |
| AZ failure | 2 min | 0 |
| Region failure | 5 min | 0 |
| Data corruption | 30 min | 0 |

**Zero RPO**: Synchronous replication (data never lost)

## 14. Versioning

**Current Version**: v1.0.0  
**Philosophy Changes**: Rare (fundamental principles)  
**Process Updates**: As needed (operational details)
