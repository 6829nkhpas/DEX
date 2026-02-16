# Sequence Numbering Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the sequence numbering system for the distributed exchange, providing total ordering of all events and ensuring deterministic replay.

## 2. Global Sequence Numbers

### 2.1 Definition

**Global Sequence**: Single monotonic counter across entire exchange

**Properties**:
- Starts at 1 (first event = sequence 1)
- Strictly increasing (no gaps)
- Unbounded (u64: max ~18 quintillion)
- Unique per event (one-to-one mapping)

### 2.2 Assignment

**When**: Every event receives sequence number at creation

**Process**:
```rust
let sequence = sequence_service.next();
let event = Event {
    sequence,
    timestamp: clock.get_time(),
    ...
};
```

## 3. Sequence Service

### 3.1 Implementation

**Single Source of Truth**: One service owns sequence generation

**Algorithm**:
```rust
struct SequenceGenerator {
    current: AtomicU64,
}

impl SequenceGenerator {
    fn next(&self) -> u64 {
        self.current.fetch_add(1, Ordering::SeqCst)
    }
}
```

### 3.2 Persistence

**Checkpoint**: Save current sequence every 1000 events

**Recovery**:
```
1. Read last checkpointed sequence (e.g., 1,234,000)
2. Read max sequence from event store
3. Start from max(checkpoint, event_store_max) + 1
```

### 3.3 High Availability

**Primary/Backup**:
- Primary serves sequence numbers
- Backup syncs every 100ms
- On primary failure: Backup becomes primary

**Failover Safety**:
```
new_primary_start = last_known_sequence + 1000  // Gap buffer
```

**Trade-off**: 1000 sequence number gap vs zero downtime

## 4. Per-Entity Sequences

### 4.1 Account Sequence

**Purpose**: Order updates to same account

**Example**:
```
Account ABC:
  seq 1: BalanceUpdated (deposit)
  seq 2: OrderPlaced
  seq 3: TradeFilled
  seq 4: BalanceUpdated (trade settlement)
```

**Usage**: Detect missing updates, enable optimistic concurrency

### 4.2 Order Sequence

**Purpose**: Track order lifecycle events

**Example**:
```
Order XYZ:
  seq 1: OrderSubmitted
  seq 2: OrderAccepted
  seq 3: OrderPartiallyFilled
  seq 4: OrderPartiallyFilled
  seq 5: OrderFilled
```

### 4.3 Symbol Sequence

**Purpose**: Market data sequencing per trading pair

**Example**:
```
BTC/USDT:
  seq 1: TradeExecuted
  seq 2: TradeExecuted
  seq 3: OrderBookUpdated
```

## 5. Sequence Usage

### 5.1 Event Ordering

**Query**:
```sql
SELECT * FROM events 
ORDER BY sequence ASC
```

**Guarantee**: Chronological order of all events

### 5.2 Gap Detection

**Check**:
```sql
SELECT sequence 
FROM events 
WHERE sequence > ?
  AND NOT EXISTS (
    SELECT 1 FROM events e2 
    WHERE e2.sequence = events.sequence - 1
  )
```

**Expected**: Empty set (no gaps)

### 5.3 Replay Resumption

**Process**:
```
1. Service crashes at sequence 5,432,100
2. On restart: Query last_processed_sequence from database
3. Resume from sequence 5,432,101
```

## 6. Sequence Invariants

### 6.1 No Gaps

**Invariant**: For all sequences S, event with sequence S-1 exists

**Exception**: Sequence 1 (no predecessor)

**Violation**: CRITICAL error (event loss or corruption)

### 6.2 No Duplicates

**Invariant**: Each sequence appears exactly once

**Enforcement**: Unique constraint on sequence column

**Violation**: Impossible (database constraint)

### 6.3 Strictly Increasing

**Invariant**: seq[i+1] = seq[i] + 1

**Enforcement**: Atomic increment in sequence service

## 7. Multi-Partition Sequences

### 7.1 Partitioning Strategy

**When**: Scaling beyond single sequence service

**Approach**: Partition by entity type
```
Orders: Sequence range 0 - 1 trillion
Trades: Sequence range 1 trillion - 2 trillion
Accounts: Sequence range 2 trillion - 3 trillion
```

**Trade-off**: Partitioned ordering vs global ordering

### 7.2 Alternative: Lamport Timestamps

**Concept**: Each node has local counter + node_id

**Sequence**:
```
(node_id: u16, local_counter: u64) = u80 total

Event A from node 1: (1, 1000)
Event B from node 2: (2, 999)
Order: A < B (node_id tiebreaker)
```

**Pros**: No single bottleneck  
**Cons**: More complex ordering logic

## 8. Sequence Monitoring

### 8.1 Metrics

- Current global sequence
- Sequence generation rate (events/sec)
- Last checkpoint sequence
- Detected gaps (should be 0)

### 8.2 Alerts

| Condition | Severity |
|-----------|----------|
| Gap detected | CRITICAL |
| Sequence service down | CRITICAL |
| Generation rate > 100k/sec | WARNING (approaching limits) |

## 9. Performance

### 9.1 Throughput

**Target**: 100,000 sequences/sec  
**Bottleneck**: Single atomic counter

**Optimization**: Batch allocation
```rust
// Allocate 100 sequences at once
let start = self.current.fetch_add(100, Ordering::SeqCst);
let end = start + 100;
// Distribute start..end to 100 events
```

### 9.2 Latency

**Target**: < 1μs to generate sequence  
**Actual**: ~50ns (atomic increment on modern CPU)

## 10. Disaster Recovery

### 10.1 Sequence Rebuild

**Scenario**: Sequence service state lost

**Recovery**:
```sql
SELECT MAX(sequence) FROM events;
-- Result: 10,543,234

-- Restart sequence service at 10,543,235
```

### 10.2 Duplicate Prevention

**Problem**: Sequence service fails, restarts, re-issues sequence

**Prevention**:
```
Before restart:
  Read max sequence from ALL partitions of event store
  Start from global_max + 1
```

## 11. Versioning

**Current Version**: v1.0.0  
**Breaking Change**: Switching from u64 to u128 (if needed in distant future)  
**Backward Compatibility**: Old events keep their sequences
