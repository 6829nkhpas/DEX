# Order States Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the complete state space for orders in the distributed exchange. Every order MUST exist in exactly one state at any point in time.

## 2. State Enumeration

### 2.1 Core States

| State ID | State Name | Category | Terminal |
|----------|------------|----------|----------|
| 0 | PENDING | Active | No |
| 1 | PARTIAL | Active | No |
| 2 | FILLED | Terminal | Yes |
| 3 | CANCELED | Terminal | Yes |
| 4 | REJECTED | Terminal | Yes |
| 5 | EXPIRED | Terminal | Yes |

### 2.2 State Definitions

#### PENDING (State 0)
**Description**: Order accepted and awaiting matching  
**Entry Condition**: Passed all validations  
**Characteristics**:
- Full quantity unfilled
- Actively matchable
- Margin fully reserved
- Visible in order book

**Substates**: None

#### PARTIAL (State 1)
**Description**: Order partially matched  
**Entry Condition**: At least one fill, quantity remaining  
**Characteristics**:
- 0 < filled_quantity < total_quantity
- Remaining quantity actively matchable
- Margin reserved for remaining quantity
- Visible in order book with updated quantity

**Invariants**:
- filled_quantity > 0
- remaining_quantity > 0
- filled_quantity + remaining_quantity = total_quantity

#### FILLED (State 2)
**Description**: Order completely matched (terminal)  
**Entry Condition**: filled_quantity = total_quantity  
**Characteristics**:
- 100% execution achieved
- No longer in order book
- All margin released or settled
- Settlement complete

**Finality**: Irreversible

#### CANCELED (State 3)
**Description**: Order canceled by user or system (terminal)  
**Entry Condition**: Cancel request processed  
**Characteristics**:
- Removed from order book
- Reserved margin released
- Unfilled quantity voided
- Partial fills remain valid

**Cancel Reasons**:
- `USER_REQUESTED`: Explicit user cancellation
- `SELF_TRADE`: Would match own order
- `POST_ONLY_REJECT`: Would execute as taker
- `INSUFFICIENT_MARGIN`: Margin fell below requirement
- `RISK_LIMIT_BREACH`: Position limit exceeded
- `ADMIN_CANCEL`: System administrator action

#### REJECTED (State 4)
**Description**: Order failed validation (terminal)  
**Entry Condition**: Validation failure during creation  
**Characteristics**:
- Never entered order book
- No state mutations occurred
- No margin reserved
- Order never assigned OrderID (in some implementations)

**Reject Reasons**:
- `INVALID_SCHEMA`: Malformed request
- `INVALID_PRICE`: Price outside tick size or bounds
- `INVALID_QUANTITY`: Quantity outside min/max
- `INSUFFICIENT_BALANCE`: Inadequate collateral
- `SYMBOL_NOT_FOUND`: Invalid trading pair
- `ACCOUNT_SUSPENDED`: Account restrictions
- `RATE_LIMITED`: Too many requests

#### EXPIRED (State 5)
**Description**: Time-in-force deadline reached (terminal)  
**Entry Condition**: Current time > expiry time  
**Characteristics**:
- Removed from order book
- Unfilled quantity voided
- Reserved margin released
- Partial fills remain valid

**Expiry Types**:
- GTD expiry (explicit timestamp)
- IOC expiry (immediate)

## 3. State Transition Matrix

| From → To | PENDING | PARTIAL | FILLED | CANCELED | REJECTED | EXPIRED |
|-----------|---------|---------|--------|----------|----------|---------|
| **null** (creation) | ✓ | ✗ | ✗ | ✗ | ✓ | ✗ |
| **PENDING** | — | ✓ | ✓ | ✓ | ✗ | ✓ |
| **PARTIAL** | ✗ | — | ✓ | ✓ | ✗ | ✓ |
| **FILLED** | ✗ | ✗ | — | ✗ | ✗ | ✗ |
| **CANCELED** | ✗ | ✗ | ✗ | — | ✗ | ✗ |
| **REJECTED** | ✗ | ✗ | ✗ | ✗ | — | ✗ |
| **EXPIRED** | ✗ | ✗ | ✗ | ✗ | ✗ | — |

**Legend**: ✓ = Valid transition, ✗ = Invalid transition, — = Same state (no-op)

## 4. State Transition Rules

### 4.1 Acyclic Guarantee
**Rule**: State transitions form a Directed Acyclic Graph (DAG)  
**Enforcement**: Once transitioned, previous state is unreachable  
**Exception**: None

### 4.2 Terminal State Immutability
**Rule**: Terminal states (FILLED, CANCELED, REJECTED, EXPIRED) are immutable  
**Enforcement**: Write-once semantics in storage layer  
**Violation**: Critical system error requiring incident response

### 4.3 Atomic Transitions
**Rule**: State changes are atomic with all side effects  
**Side Effects Include**:
- Margin updates
- Order book mutations
- Event emissions
- Journal writes

**Rollback Policy**: All-or-nothing transaction semantics

### 4.4 Monotonic Progression
**Rule**: State transitions always move "forward" in lifecycle  
**Order**: PENDING → PARTIAL → (FILLED | CANCELED | EXPIRED)  
**Backwards Transition**: Prohibited

## 5. State-Specific Behaviors

### 5.1 Margin Reservation

| State | Margin Status |
|-------|---------------|
| PENDING | Full quantity reserved |
| PARTIAL | Remaining quantity reserved |
| FILLED | Margin settled to position |
| CANCELED | Margin released immediately |
| REJECTED | No margin reserved |
| EXPIRED | Margin released immediately |

### 5.2 Order Book Visibility

| State | In Order Book | Matchable |
|-------|---------------|-----------|
| PENDING | Yes | Yes |
| PARTIAL | Yes | Yes (remaining) |
| FILLED | No | No |
| CANCELED | No | No |
| REJECTED | No | No |
| EXPIRED | No | No |

### 5.3 Query Visibility

| State | ListOpenOrders | ListAllOrders | GetOrder |
|-------|----------------|---------------|----------|
| PENDING | Yes | Yes | Yes |
| PARTIAL | Yes | Yes | Yes |
| FILLED | No | Yes | Yes |
| CANCELED | No | Yes | Yes |
| REJECTED | No | No | Yes |
| EXPIRED | No | Yes | Yes |

## 6. Event Correlation

Every state transition MUST emit exactly one event:

```
null → PENDING: OrderAccepted
null → REJECTED: OrderRejected
PENDING → PARTIAL: OrderPartiallyFilled
PENDING → FILLED: OrderFilled
PENDING → CANCELED: OrderCanceled
PENDING → EXPIRED: OrderExpired
PARTIAL → PARTIAL: OrderPartiallyFilled (additional)
PARTIAL → FILLED: OrderFilled
PARTIAL → CANCELED: OrderCanceled
PARTIAL → EXPIRED: OrderExpired
```

## 7. Persistence Format

### 7.1 Storage Schema
```
OrderState {
  order_id: UUID,
  state: u8,  // State ID from enumeration
  prev_state: u8,  // For audit trail
  state_reason: String,  // Cancel/reject reason
  filled_quantity: Decimal,
  remaining_quantity: Decimal,
  state_changed_at: i64,  // Unix nanos
  state_seq: u64,  // Monotonic sequence
}
```

### 7.2 Indexing Requirements
- Primary: `order_id`
- Secondary: `(account_id, state, created_at)` for queries
- Secondary: `(symbol, state, price)` for order book

## 8. Determinism Guarantees

1. **State Derivation**: Given order journal, state is derivable
2. **Transition Function**: Pure function with no side effects
3. **No Randomness**: State changes are deterministic
4. **Replay Consistency**: Replaying events yields identical state
5. **Clock Independence**: State transitions use logical time (sequence numbers)

## 9. Validation Rules

### 9.1 State Invariants
```
PENDING: filled_quantity = 0 AND remaining_quantity = total_quantity
PARTIAL: 0 < filled_quantity < total_quantity
FILLED: filled_quantity = total_quantity AND remaining_quantity = 0
CANCELED: remaining_quantity >= 0 (may be partially filled)
REJECTED: filled_quantity = 0 AND remaining_quantity = 0
EXPIRED: remaining_quantity >= 0 (may be partially filled)
```

### 9.2 Transition Validation
Before any state change:
1. Verify current state matches expected source state
2. Validate transition is allowed per matrix (Section 3)
3. Check invariants for target state
4. Atomically update state + side effects

## 10. Error Handling

### 10.1 Invalid Transition Attempt
**Action**: Reject operation with `INVALID_STATE_TRANSITION` error  
**Logging**: Log as WARNING (may indicate client bug)  
**State**: No change

### 10.2 Concurrent Modification
**Detection**: Compare-and-swap on state field  
**Resolution**: Retry logic in application layer  
**Guarantee**: Last-write-wins prohibited

### 10.3 Inconsistent State
**Detection**: Invariant validation failure  
**Action**: PANIC and halt order processing  
**Recovery**: Manual intervention required

## 11. Performance Characteristics

- State transition: O(1) time complexity
- State lookup: O(1) with proper indexing
- Event emission: O(1) per transition
- Journal write: O(1) append-only

## 12. Versioning

**Current Version**: v1.0.0  
**State ID Stability**: State IDs MUST NOT change  
**New States**: Append to enumeration only (never reuse IDs)  
**Deprecated States**: Mark deprecated, never remove
