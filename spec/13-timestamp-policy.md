# Timestamp Policy Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the timestamp policy for the distributed exchange, establishing a single source of truth for time across all services.

## 2. Core Principles

1. **Single Clock**: Exchange maintains one logical clock
2. **Monotonic**: Time never goes backwards
3. **Deterministic**: Same events → same timestamps (on replay)
4. **Precise**: Nanosecond resolution

## 3. Timestamp Format

### 3.1 Standard Format

**Type**: Unix timestamp in nanoseconds (i64)  
**Range**: -292 billion years to +292 billion years  
**Resolution**: 1 nanosecond

**Example**:
```
1708123456789012345  (nanoseconds since Unix epoch)
= 2024-02-16 23:30:56.789012345 UTC
```

### 3.2 Why Nanoseconds?

- **Precision**: Match high-frequency trading requirements
- **Ordering**: Distinguish events in same microsecond
- **Standard**: Widely used in financial systems

## 4. Time Sources

### 4.1 Exchange Clock Service

**Authority**: Single centralized time service  
**Implementation**: NTP-synchronized primary + backup servers  
**Accuracy**: ±1ms from UTC

**API**:
```protobuf
service ClockService {
  rpc GetTime() returns (Timestamp);
}

message Timestamp {
  int64 nanos = 1;  // Unix nanoseconds
}
```

### 4.2 Forbidden Sources

**NEVER USE**:
- Client-provided timestamps (untrusted, skewed)
- Server local system clocks (drift, skew between nodes)
- External API timestamps (unreliable, non-deterministic)

## 5. Timestamp Usage

### 5.1 Event Generation

**When**: Creating new events (OrderSubmitted, TradeExecuted, etc.)

**Process**:
```rust
let timestamp = clock_service.get_time().await?;
let event = Event {
    event_id: uuid::Uuid::new_v7(),
    timestamp,
    event_type: "OrderAccepted",
    ...
};
```

### 5.2 State Transitions

**When**: Recording when state changed

**Process**:
```rust
order.updated_at = event.timestamp;  // Use event timestamp (deterministic)
// NOT: order.updated_at = SystemTime::now()  // Non-deterministic!
```

### 5.3 Order Matching

**Priority**: Price-time priority requires precise timestamps

**Matching**:
```
Orders at same price level:
  Sort by timestamp ASC (earlier orders first)
```

## 6. MonotonicGuarantee

### 6.1 Strictly Increasing

**Rule**: Each event timestamp > previous event timestamp

**Implementation**:
```rust
struct MonotonicClock {
    last_timestamp: AtomicI64,
}

impl MonotonicClock {
    fn get_time(&self) -> i64 {
        loop {
            let now = self.fetch_from_ntp();
            let last = self.last_timestamp.load(Ordering::SeqCst);
            
            let next = std::cmp::max(now, last + 1);
            
            if self.last_timestamp
                .compare_exchange(last, next, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return next;
            }
        }
    }
}
```

**Guarantee**: No two events have same timestamp

### 6.2 Leap Seconds

**Handling**: Smeared over 24 hours (Google-style)

**Example**:
```
Leap second occurs at midnight →
Slow down clock by 1/86400 seconds each second
Over 24 hours, accumulates to 1 second adjustment
```

**Result**: Monotonicity preserved, no sudden jumps

## 7. Clock Synchronization

### 7.1 NTP Configuration

**Primary**: pool.ntp.org (public NTP pool)  
**Backup**: time.google.com (Google Public NTP)  
**Frequency**: Sync every 64 seconds

**Acceptable Drift**: ±10ms before resync

### 7.2 Clock Skew Detection

**Monitor**: Difference between exchange clock and NTP

**Alerts**:
- Skew > 100ms: WARNING
- Skew > 1s: CRITICAL (halt trading)

**Correction**:
- Skew < 100ms: Gradual adjustment (1ms/sec)
- Skew > 100ms: Immediate resync (during maintenance window)

## 8. Timestamp Precision

### 8.1 Rounding

**Rule**: Never round timestamps

**Reason**: Rounding breaks ordering guarantees

**Example**:
```rust
// WRONG
let timestamp_ms = timestamp_nanos / 1_000_000;  // Loses precision

// CORRECT
let timestamp_nanos = event.timestamp;  // Keep full precision
```

### 8.2 Display Format

**For Users**: ISO 8601 with microseconds
```
2024-02-16T23:30:56.789012Z
```

**For Logs**: Unix nanoseconds + ISO 8601
```
1708123456789012345 (2024-02-16T23:30:56.789012Z)
```

## 9. Timestamp in Data Structures

### 9.1 Database Schema

```sql
CREATE TABLE orders (
  order_id UUID PRIMARY KEY,
  created_at BIGINT NOT NULL,  -- Unix nanoseconds
  updated_at BIGINT NOT NULL,
  ...
);

CREATE INDEX idx_created_at ON orders(created_at);
```

**Type**: BIGINT (not TIMESTAMP, which has lower precision)

### 9.2 Event Structure

```json
{
  "event_id": "01939d7f-8e4a-7890-a123-456789abcdef",
  "timestamp": 1708123456789012345,
  "event_type": "OrderAccepted",
  ...
}
```

### 9.3 API Responses

```json
{
  "order_id": "...",
  "created_at": "2024-02-16T23:30:56.789012Z",  // ISO 8601 for clients
  "created_at_nanos": 1708123456789012345       // Unix nanos for precision
}
```

## 10. Time Zones

**Rule**: All times in UTC

**Rationale**:
- No daylight saving time complexity
- Global standard
- Deterministic (no locale dependencies)

**Display**: Convert to user's local timezone in UI only

## 11. Timestamp Validation

### 11.1 Client Timestamps

**If Accepting** (e.g., for cancel-after):
```rust
fn validate_client_timestamp(ts: i64, server_ts: i64) -> Result<()> {
    let max_skew = 60_000_000_000;  // 60 seconds in nanos
    
    if (ts - server_ts).abs() > max_skew {
        return Err("Timestamp skew too large");
    }
    
    Ok(())
}
```

### 11.2 Sanity Checks

**Minimum**: 2020-01-01 (1577836800000000000 nanos)  
**Maximum**: 2100-01-01 (4102444800000000000 nanos)

**Validation**:
```rust
if timestamp < MIN_TIMESTAMP || timestamp > MAX_TIMESTAMP {
    return Err("Timestamp out of valid range");
}
```

## 12. Replay Behavior

### 12.1 Historical Replay

**Timestamp Source**: Use timestamps from events (NOT current time)

**Example**:
```rust
for event in events {
    let state_time = event.timestamp;  // Use historical timestamp
    apply_event(&mut state, &event, state_time);
}
```

### 12.2 Live Replay (Catch-up)

**Timestamp Source**: Events up to current time, then switch to live

**Process**:
```rust
// Catch-up phase
for event in historical_events {
    apply_event(&mut state, &event, event.timestamp);
}

// Live phase
for event in live_event_stream {
    apply_event(&mut state, &event, clock_service.get_time());
}
```

## 13. Performance Considerations

### 13.1 Caching

**Strategy**: Cache clock time for 1ms intervals

**Implementation**:
```rust
struct CachedClock {
    cache: Mutex<(i64, Instant)>,  // (timestamp, system_instant)
}

impl CachedClock {
    fn get_time(&self) -> i64 {
        let mut cache = self.cache.lock().unwrap();
        let elapsed = cache.1.elapsed();
        
        if elapsed < Duration::from_millis(1) {
            return cache.0 + elapsed.as_nanos() as i64;
        }
        
        let new_ts = self.fetch_from_clock_service();
        *cache = (new_ts, Instant::now());
        new_ts
    }
}
```

**Trade-off**: 1ms staleness vs reduced clock service load

### 13.2 Batching

**Strategy**: Fetch timestamp once per batch of events

**Example**:
```rust
let batch_timestamp = clock_service.get_time();

for order in batch {
    let event = Event {
        timestamp: batch_timestamp,
        ...
    };
    events.push(event);
}
```

**Result**: All events in batch have same timestamp (acceptable)

## 14. Monitoring

### 14.1 Metrics

- Clock skew (exchange vs NTP)
- Timestamp monotonicity violations (should be 0)
- Clock service availability
- Time between events (should be < 1s typically)

### 14.2 Alerts

| Condition | Severity |
|-----------|----------|
| Clock skew > 100ms | WARNING |
| Clock skew > 1s | CRITICAL |
| Non-monotonic timestamp | CRITICAL |
| Clock service unavailable | CRITICAL |

## 15. Disaster Recovery

### 15.1 Clock Service Failure

**Scenario**: Clock service becomes unavailable

**Fallback**:
```
1. Switch to backup clock service (hot standby)
2. If both down: Use server system time (with warning)
3. Log all timestamps with "fallback" flag
4. After recovery: Verify no monotonicity violations
```

### 15.2 Clock Rollback

**Scenario**: NTP time goes backwards (rare but possible)

**Detection**:
```rust
if new_timestamp < last_timestamp {
    panic!("Clock rollback detected!");
}
```

**Recovery**: Manual intervention required (investigate NTP issue)

## 16. Invariants

1. **Monotonicity**: timestamp[i+1] > timestamp[i] (strictly increasing)
2. **Accuracy**: |exchange_time - UTC| < 1ms (in steady state)
3. **Precision**: Nanosecond resolution maintained end-to-end
4. **Determinism**: Same events → same timestamp order on replay

## 17. Versioning

**Current Version**: v1.0.0  
**Precision Change**: Would be breaking change (new major version)  
**Format Change**: Maintain backward compatibility with conversion
