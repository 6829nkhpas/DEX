# Replay Requirements Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the replay requirements for the distributed exchange, ensuring that system state can be deterministically reconstructed from event logs.

## 2. Core Requirements

### 2.1 Full System Replay

**Capability**: Reconstruct entire exchange state from genesis event

**Process**:
```
1. Start with empty state (no accounts, no orders, no positions)
2. Read all events from event store ORDER BY sequence ASC
3. Apply each event to state sequentially
4. Verify final state matches production checksums
```

**Use Cases**:
- Disaster recovery
- Audit verification
- Testing against historical data
- Bug investigation

### 2.2 Partial Replay

**Capability**: Reconstruct specific entity state (account, order, position)

**Process**:
```
1. Identify entity ID (account_id, order_id, etc.)
2. Query events WHERE entity_id = ? ORDER BY sequence ASC
3. Apply events to empty entity state
4. Return reconstructed entity
```

**Use Cases**:
- Customer support queries
- Dispute resolution
- PnL verification

## 3. Event Stream Properties

### 3.1 Completeness

**Requirement**: Every state change emits exactly one event

**Validation**:
```sql
-- Count state changes
SELECT COUNT(*) FROM orders WHERE updated_at > ?

-- Count events
SELECT COUNT(*) FROM events 
WHERE event_type LIKE 'Order%' AND timestamp > ?

-- Must be equal
```

### 3.2 Ordering

**Requirement**: Events ordered by sequence number (monotonic, gapless)

**Validation**:
```sql
-- Check for gaps
SELECT sequence FROM events 
WHERE NOT EXISTS (
  SELECT 1 FROM events e2 WHERE e2.sequence = events.sequence - 1
) AND sequence > 1;

-- Should return empty set
```

### 3.3 Causality

**Requirement**: Dependent events appear in causal order

**Examples**:
```
OrderSubmitted (seq 100) → OrderAccepted (seq 101) → TradeExecuted (seq 102)
```

**Violation Example** (WRONG):
```
TradeExecuted (seq 102) appears before OrderAccepted (seq 101)
```

## 4. Deterministic Replay

### 4.1 Inputs

**Guaranteed Deterministic**:
- Event payload (all data needed for state transition)
- Event sequence number
- Event timestamp (exchange time, not wall clock)

**NOT Deterministic**:
- Current wall clock time
- Random number generators
- External API calls (prices from 3rd parties)

### 4.2 State Transitions

**Requirement**: Same event → same state change (always)

**Implementation**:
```rust
fn apply_event(state: &mut State, event: &Event) -> Result<()> {
    match event.event_type {
        "OrderAccepted" => {
            let order = state.orders.get_mut(&event.order_id)?;
            order.state = OrderState::Pending;
            order.updated_at = event.timestamp;  // Use event timestamp
            // DO NOT: order.updated_at = SystemTime::now()  // NON-DETERMINISTIC
            Ok(())
        }
        _ => unimplemented!()
    }
}
```

### 4.3 No Side Effects

**Requirement**: Replay does NOT:
- Send emails/notifications
- Call external APIs
- Generate new events
- Modify production database

**Implementation**: Replay flag in runtime context
```rust
struct ReplayContext {
    is_replay: bool,
}

fn send_notification(ctx: &ReplayContext, msg: Notification) {
    if !ctx.is_replay {
        email_service.send(msg);
    }
}
```

## 5. Replay Modes

### 5.1 Live Replay

**Purpose**: Catch up after downtime  
**Target**: Production database  
**Side Effects**: Enabled (emails, etc.)

**Process**:
```
1. Service starts, reads last_processed_sequence from db
2. Query events WHERE sequence > last_processed_sequence
3. Apply events, updating last_processed_sequence
4. When caught up, switch to live event stream
```

### 5.2 Historical Replay

**Purpose**: Investigation, testing  
**Target**: Separate database/in-memory  
**Side Effects**: Disabled

**Process**:
```
1. Create empty test database
2. Replay events from time_start to time_end
3. Analyze resulting state
4. Discard test database
```

### 5.3 Time-Travel Queries

**Purpose**: "What was the state at timestamp T?"

**Process**:
```
1. Load latest snapshot before T
2. Replay events from snapshot_time to T
3. Return state at T
```

**Example**:
```
Question: "What was user ABC's balance at 2024-02-01 12:00:00?"
Answer: Replay events up to that timestamp, read balance
```

## 6. Snapshots

### 6.1 Purpose

**Problem**: Replaying 1 billion events takes too long  
**Solution**: Periodic snapshots of full state

### 6.2 Snapshot Creation

**Frequency**: Every 1 million events (or 1 hour, whichever comes first)

**Content**:
```
Snapshot {
  sequence: 5_000_000,
  timestamp: 1708123456789000000,
  state: {
    accounts: Map<AccountID, Account>,
    orders: Map<OrderID, Order>,
    positions: Map<PositionID, Position>,
    balances: Map<(AccountID, Asset), Balance>,
  },
  checksum: "sha256_hash_of_state"
}
```

**Storage**: Object storage (S3, Azure Blob)

### 6.3 Snapshot Restore

**Process**:
```
1. Download snapshot for sequence S
2. Load state into memory/database
3. Replay events from sequence S+1 to current
4. Resume normal operation
```

**Time Savings**:
- Without snapshot: Replay 10M events = 10 minutes
- With snapshot at 9M: Replay 1M events = 1 minute

## 7. Validation

### 7.1 Checksum Verification

**Process**:
```
1. Replay all events in test environment
2. Compute checksum of final state
3. Compare with production state checksum
4. If mismatch: ALERT (data corruption or replay bug)
```

**Frequency**: Weekly

### 7.2 Invariant Checks

**During Replay**:
```
After each event:
  assert(total_balances == sum_of_all_accounts)
  assert(order.filled + order.remaining == order.total)
  assert(position.size >= 0)
```

**On Violation**: Halt replay, log event sequence number

### 7.3 State Reconciliation

**Process**:
```
1. Take snapshot of production state
2. Replay events from genesis
3. Compare replayed state vs production state
4. Report any discrepancies
```

**Possible Discrepancies**:
- Replay bug (logic error)
- Missing events (event store corruption)
- Non-deterministic code

## 8. Performance Optimization

### 8.1 Parallel Replay

**Strategy**: Replay independent entities in parallel

**Example**:
```
Account A events → Thread 1
Account B events → Thread 2
(No cross-account dependencies)
```

**Speedup**: 10x on 16-core machine

### 8.2 Incremental Snapshots

**Strategy**: Update snapshot incrementally instead of full rebuild

**Process**:
```
1. Load previous snapshot (sequence 5M)
2. Apply events 5M to 6M
3. Save new snapshot (sequence 6M)
4. Delta: Only 1M events, not full rebuild
```

### 8.3 Compressed Events

**Storage Savings**: Compress events with zstd (70% reduction)  
**Trade-off**: Decompression adds 10% overhead to replay

## 9. Replay Testing

### 9.1 Continuous Validation

**Automated Test** (runs daily):
```
1. Replay events from last 7 days
2. Compare final state with production
3. Report pass/fail
```

**Alerts**: If fails, page on-call engineer

### 9.2 Chaos Replay

**Test**: Introduce random failures during replay
- Kill replay process mid-stream
- Corrupt random event
- Skip random events

**Expected**: Replay should detect and fail gracefully

## 10. Compliance

### 10.1 Audit Requirements

**Regulation**: Financial exchanges must retain full audit trail

**Compliance**:
- Events retained forever (never deleted)
- Tamper-proof (append-only log)
- Replayable for auditors

### 10.2 Right to Explanation

**User Request**: "Why was my order rejected at timestamp T?"

**Response**:
```
1. Replay events up to T
2. Find OrderRejected event
3. Extract rejection reason from event payload
4. Provide explanation with evidence
```

## 11. Disaster Recovery

### 11.1 Total Loss Scenario

**Scenario**: All production databases lost

**Recovery**:
```
1. Retrieve event log from backup (immutable storage)
2. Provision new database cluster
3. Replay all events from genesis
4. Verify state integrity
5. Resume trading
```

**RTO**: 30 minutes (for 100M events at 50k events/sec replay rate)

### 11.2 Backup Strategy

**Event Log Backups**:
- Real-time replication to 3 regions
- Daily backups to glacier storage
- Retention: Infinite

## 12. Invariants

1. **Gapless Sequences**: seq[i+1] = seq[i] + 1
2. **Idempotent Replay**: Replay events twice → same final state
3. **Causal Consistency**: Parent event before child event
4. **Completeness**: All state changes represented by events

## 13. Anti-Patterns

### 13.1 Using Wall Clock Time

**WRONG**:
```rust
order.created_at = SystemTime::now();  // Non-deterministic!
```

**CORRECT**:
```rust
order.created_at = event.timestamp;  // Deterministic
```

### 13.2 External Dependencies

**WRONG**:
```rust
let price = fetch_price_from_external_api();  // Non-deterministic!
```

**CORRECT**:
```rust
let price = event.payload.price;  // Stored in event
```

### 13.3 Random Numbers

**WRONG**:
```rust
let trade_id = random_uuid();  // Non-deterministic!
```

**CORRECT**:
```rust
let trade_id = event.trade_id;  // Pre-assigned, in event
```

## 14. Versioning

**Current Version**: v1.0.0  
**Event Schema Changes**: Must be backward compatible  
**Replay Logic Changes**: Test against production event log before deploy
