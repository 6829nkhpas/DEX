# Determinism Rules Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the determinism rules for the distributed exchange, ensuring that all operations produce identical outputs given identical inputs.

## 2. Core Principle

**Golden Rule**: Same inputs → Same outputs (always, everywhere, every time)

**Why Critical**:
- Event replay correctness
- Multi-node consensus
- Audit verification
- Bug reproducibility

## 3. Forbidden Operations

### 3.1 System Time

**PROHIBITED**:
```rust
SystemTime::now()           // WRONG
Instant::now()              // WRONG  
chrono::Utc::now()          // WRONG
time.time() (Python)        // WRONG
Date.now() (JavaScript)     // WRONG
```

**ALLOWED**:
```rust
event.timestamp             // From event payload
request.timestamp           // From client request (validated)
exchange_clock.current()    // From centralized time service
```

### 3.2 Random Number Generation

**PROHIBITED**:
```rust
rand::random()              // WRONG
Math.random()               // WRONG
random.random() (Python)    // WRONG
```

**ALLOWED**:
```rust
seeded_rng(event.sequence)  // Deterministic PRNG with event seq as seed
uuid_v7(timestamp)          // Time-based UUID (using event timestamp)
hash(order_id + symbol)     // Deterministic hash function
```

### 3.3 External API Calls

**PROHIBITED** (during state transitions):
```rust
http_client.get("https://api.binance.com/price")  // WRONG
database.query_random_row()  // WRONG
```

**ALLOWED**:
```rust
event.payload.external_price  // Price stored in event
snapshot.market_data          // Data from snapshot
```

**Note**: External calls OK for event *generation*, but NOT for event *processing*

### 3.4 File System State

**PROHIBITED**:
```rust
fs::read_dir("/tmp")         // WRONG (non-deterministic order)
Path::exists("/some/file")   // WRONG (changes over time)
```

**ALLOWED**:
```rust
embedded_config[key]         // Config embedded in binary
event.payload.config         // Config in event
```

### 3.5 Iteration Order

**PROHIBITED**:
```rust
HashMap::iter()              // WRONG (insertion order not guaranteed)
HashSet::iter()              // WRONG
```

**ALLOWED**:
```rust
BTreeMap::iter()             // Sorted iteration
Vec::iter()                  // Insertion order preserved
```

### 3.6 Floating-Point Arithmetic

**PROHIBITED**:
```rust
let result = price *0.001 + fee;    // WRONG (f64 rounding varies)
```

**ALLOWED**:
```rust
let result = Decimal::from(price) * Decimal::from_str("0.001")? + fee;
```

**Reason**: Floating-point arithmetic NOT deterministic across architectures

### 3.7 Concurrency Races

**PROHIBITED**:
```rust
tokio::select! {
    a = future_a => process(a),  // WRONG (race condition)
    b = future_b => process(b),
}
```

**ALLOWED**:
```rust
let results = futures::join!(future_a, future_b);  // Deterministic join
process(results.0);
process(results.1);
```

## 4. Mandatory Practices

### 4.1 Fixed-Point Arithmetic

**Requirement**: Use `Decimal` type for all financial calculations

**Example**:
```rust
use rust_decimal::Decimal;

let quantity = Decimal::from_str("1.5")?;
let price = Decimal::from_str("50000.00")?;
let value = quantity * price;  // Exact, deterministic
```

**Precision**: 18 decimal places internally

### 4.2 Deterministic Rounding

**Rule**: Always use `ROUND_HALF_UP` (round ties away from zero)

**Example**:
```rust
let fee = Decimal::from_str("12.345")?;
let rounded = fee.round_dp_with_strategy(2, RoundingStrategy::RoundHalfUp);
// 12.345 → 12.35
// 12.344 → 12.34
// 12.335 → 12.34
```

**Forbidden**: ROUND_HALF_EVEN (banker's rounding, implementation-dependent)

### 4.3 Sorted Iteration

**Rule**: Always iterate in deterministic order

**Example**:
```rust
// WRONG
for (k, v) in hashmap.iter() {  // Unordered
    process(k, v);
}

// CORRECT (Option 1: Use BTreeMap)
let btreemap: BTreeMap<_, _> = hashmap.into_iter().collect();
for (k, v) in btreemap.iter() {  // Sorted by key
    process(k, v);
}

// CORRECT (Option 2: Sort keys)
let mut keys: Vec<_> = hashmap.keys().collect();
keys.sort();
for k in keys {
    process(k, hashmap[k]);
}
```

### 4.4 String Encoding

**Rule**: Always use UTF-8 encoding

**Example**:
```rust
let bytes = string.as_bytes();  // UTF-8 encoded
let hash = sha256(bytes);
```

**Forbidden**: Platform default encoding

### 4.5 Comparison Operators

**Rule**: Use total ordering for all comparisons

**Example**:
```rust
// WRONG (for f64)
if price1 > price2 {  // Partial ordering (NaN issues)

// CORRECT (for Decimal)
if price1 > price2 {  // Total ordering
```

## 5. Deterministic ID Generation

### 5.1 UUID v7 (Time-Ordered)

**Recommended**: UUIDv7 (deterministic given timestamp + sequence)

**Example**:
```rust
let trade_id = UUIDv7::new(event.timestamp, event.sequence);
```

**Properties**:
- Sortable by time
- Deterministic (same timestamp + sequence → same UUID)
- No random component

### 5.2 Sequential IDs

**Alternative**: Simple counter (requires coordination)

**Example**:
```rust
let order_id = sequence_generator.next();  // 1, 2, 3, ...
```

**Pros**: Simpler, guaranteed unique  
**Cons**: Requires single source of truth (bottleneck)

## 6. State Derivation

### 6.1 Pure Functions

**Requirement**: All state transitions are pure functions

**Definition**:
```rust
fn apply_trade(state: State, trade: Trade) -> State {
    // No side effects
    // No external dependencies
    // Same input → same output
    let mut new_state = state.clone();
    new_state.balance += trade.value;
    new_state
}
```

### 6.2 Command Sourcing Pattern

**Process**:
```
1. Command arrives (CreateOrder)
2. Validate command (deterministic validation)
3. Generate event (OrderCreated) with unique ID
4. Store event (append-only log)
5. Apply event to state (deterministic state transition)
```

**Key**: Steps 2 and 5 must be deterministic

## 7. Testing Determinism

### 7.1 Replay Test

**Test**:
```rust
#[test]
fn test_deterministic_replay() {
    let events = load_events();
    
    // Run 1
    let state1 = replay_events(&events);
    
    // Run 2
    let state2 = replay_events(&events);
    
    // Must be identical
    assert_eq!(state1, state2);
}
```

### 7.2 Parallel Replay Test

**Test**:
```rust
#[test]
fn test_parallel_determinism() {
    let events = load_events();
    
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let events = events.clone();
            tokio::spawn(async move { replay_events(&events) })
        })
        .collect();
    
    let states: Vec<_> = futures::future::join_all(handles).await;
    
    // All states must be identical
    let first = &states[0];
    for state in &states[1..] {
        assert_eq!(first, state);
    }
}
```

### 7.3 Property-Based Testing

**Test**:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_add_then_remove_deterministic(
        balance: Decimal,
        amount: Decimal
    ) {
        let state1 = balance + amount - amount;
        let state2 = balance;
        assert_eq!(state1, state2);  // Reversibility
    }
}
```

## 8. Common Pitfalls

### 8.1 JSON Object Ordering

**Problem**: JSON object key order not guaranteed

**Solution**:
```rust
// WRONG
let json = serde_json::to_string(&hashmap)?;

// CORRECT (use BTreeMap)
let btreemap: BTreeMap<_, _> = hashmap.into_iter().collect();
let json = serde_json::to_string(&btreemap)?;
```

### 8.2 Database Query Order

**Problem**: `SELECT * FROM orders` without ORDER BY

**Solution**:
```sql
-- WRONG
SELECT * FROM orders WHERE account_id = ?

-- CORRECT
SELECT * FROM orders WHERE account_id = ? ORDER BY order_id ASC
```

### 8.3 Locale-Dependent Formatting

**Problem**: Number/date formatting varies by locale

**Solution**:
```rust
// WRONG
format!("{}", price)  // Uses system locale

// CORRECT
format!("{:.2}", price)  // Explicit precision
price.to_string()        // Canonical representation
```

## 9. Multi-Node Determinism

### 9.1 Consensus Requirement

**For Production**: All nodes must compute identical state

**Validation**:
```
Node A: state_hash = sha256(serialize(state))
Node B: state_hash = sha256(serialize(state))

If state_hash_A != state_hash_B → CRITICAL ERROR
```

### 9.2 Non-Determinism Detection

**Mechanism**: Periodic state hash comparison

**Process**:
```
Every 1000 events:
  1. Each node computes state hash
  2. Broadcast hash to other nodes
  3. Compare hashes
  4. If mismatch: Halt and investigate
```

## 10. Debugging Non-Determinism

### 10.1 Binary Search

**Problem**: Replay diverges from production at event 5,000,000

**Investigation**:
```
1. Replay 0 to 2,500,000 → Compare state (match/mismatch?)
2. If mismatch: Recurse on 0 to 1,250,000
3. If match: Recurse on 2,500,000 to 5,000,000
4. Continue until found exact diverging event
```

### 10.2 Diff Tool

**Tool**: Compare two states field-by-field

**Example**:
```rust
fn diff_states(state1: &State, state2: &State) {
    for (id, account1) in &state1.accounts {
        let account2 = &state2.accounts[id];
        if account1.balance != account2.balance {
            println!("Account {} balance mismatch: {} vs {}", 
                     id, account1.balance, account2.balance);
        }
    }
}
```

## 11. Performance vs Determinism

### 11.1 Trade-offs

**Determinism Costs**:
- BTreeMap vs HashMap (10-20% slower)
- Decimal vs f64 (5-10% slower)
- Sorted iteration (additional sorting cost)

**Verdict**: Worth it (correctness > performance)

### 11.2 Optimization Opportunities

**Safe Optimizations**:
- Parallel processing of independent entities
- Caching (with deterministic cache keys)
- SIMD for batch calculations (still deterministic)

**Unsafe Optimizations**:
- Lock-free data structures (if ordering matters)
- Speculative execution
- Compiler reordering (use barriers)

## 12. Invariants

1. **Replay Consistency**: Replay produces identical state
2. **Cross-Node Consistency**: All nodes compute same state
3. **Time Independence**: Same today as tomorrow as next year

## 13. Versioning

**Current Version**: v1.0.0  
**Rule Changes**: Extremely rare (fundamental to system)  
**Violations**: Treated as critical bugs (P0 severity)
