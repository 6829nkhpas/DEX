# Order Lifecycle Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the complete lifecycle of an order in the distributed exchange, from creation to terminal state. All implementations MUST follow this specification exactly.

## 2. Lifecycle Phases

### 2.1 Order Creation Phase

**Entry Condition**: Client submits order request  
**Duration**: Single atomic operation  
**Outcomes**: ACCEPTED or REJECTED

#### Validation Steps (Deterministic Order)
1. Schema validation (structure, field types)
2. Business rule validation (price increment, min size)
3. Account balance check (available margin)
4. Risk limits verification (position limits, order count)
5. Sequence number assignment
6. Timestamp assignment (exchange time)

**Success Criteria**: All validations pass  
**Failure Action**: Reject with specific error code, NO state mutation

### 2.2 Order Acceptance Phase

**Entry Condition**: Validation passed  
**State Transition**: null → PENDING  
**Actions**:
- Reserve margin/collateral
- Assign unique OrderID (UUID v7 for time-sortable)
- Persist to order book journal
- Emit `OrderAccepted` event

**Atomicity**: All actions MUST succeed or rollback completely

### 2.3 Order Matching Phase

**Entry Condition**: Order in PENDING or PARTIAL state  
**Execution Model**: Continuous matching against order book  
**Possible Outcomes**:
- Full match → FILLED
- Partial match → PARTIAL
- No match → remains PENDING
- Canceled during matching → CANCELED

**Matching Rules**:
- Price-time priority (better price first, then earlier timestamp)
- Pro-rata at same price level (if configured)
- Self-trade prevention (cancel aggressive order)

### 2.4 Order Modification Phase

**Allowed Operations**:
- Cancel (any non-terminal state)
- Reduce quantity (PENDING or PARTIAL only)
- Price modification (PROHIBITED - must cancel and replace)

**Cancel Process**:
1. Locate order by OrderID
2. Verify ownership (AccountID match)
3. Release reserved margin
4. Remove from order book
5. State → CANCELED
6. Emit `OrderCanceled` event

**Atomicity**: Must prevent race with matching engine

### 2.5 Settlement Phase

**Trigger**: Order reaches FILLED state  
**Actions**:
1. Calculate fees (maker/taker)
2. Update account balances (settled margin)
3. Update positions
4. Mark order as SETTLED
5. Emit `TradeFilled` and `OrderSettled` events

**Settlement Guarantee**: T+0 (immediate settlement)

### 2.6 Terminal States

Orders reach one of these final states:

| State | Description | Reversible |
|-------|-------------|------------|
| FILLED | 100% matched and settled | No |
| CANCELED | User or system canceled | No |
| REJECTED | Failed validation | No |
| EXPIRED | Time-in-force expired | No |

**Terminal Guarantee**: Once terminal, state NEVER changes

## 3. Event Emissions

Every phase transition MUST emit corresponding event:

```
Creation → OrderSubmitted
Validation Pass → OrderAccepted
Match → TradeExecuted
Partial Fill → OrderPartiallyFilled
Full Fill → OrderFilled
Cancel → OrderCanceled
Expiry → OrderExpired
Reject → OrderRejected
```

## 4. Failure Recovery

### 4.1 Crash During Creation
- **Before persistence**: Order never existed, client retries
- **After persistence, before event**: Replay from journal

### 4.2 Crash During Matching
- Recover order book state from journal
- Reapply matching algorithm deterministically
- Emit missed events in sequence order

### 4.3 Crash During Settlement
- Idempotent settlement using OrderID + SequenceNumber
- Prevent double-settlement via settlement journal

## 5. Time-in-Force Policies

| TIF Type | Behavior | Expiry |
|----------|----------|--------|
| GTC (Good-Till-Cancel) | Remains until filled or canceled | None |
| IOC (Immediate-or-Cancel) | Match immediately, cancel remainder | Instant |
| FOK (Fill-or-Kill) | Full match or reject entirely | Instant |
| GTD (Good-Till-Date) | Expire at specified timestamp | UTC timestamp |

**Expiry Check**: Performed BEFORE matching attempt each cycle

## 6. Determinism Requirements

1. **Order Processing**: Same input → same output (no random jitter)
2. **Timestamp Source**: Exchange monotonic clock only
3. **Matching Order**: Strict price-time priority, no ties
4. **Event Sequence**: Strictly increasing sequence numbers
5. **Replay**: Journal replay produces identical state

## 7. Performance Targets

- Order validation: < 100μs (p99)
- Matching latency: < 500μs (p99)
- Settlement finality: < 1ms (p99)
- Throughput: 100,000 orders/sec per symbol

## 8. Invariants

1. Order state transitions are acyclic (no loops)
2. Total reserved margin = sum of all open orders' margin
3. Filled quantity ≤ original quantity
4. Every filled order has corresponding trades
5. Terminal states are immutable

## 9. Interface Contract

Implementations MUST provide:

```
CreateOrder(request: OrderRequest) → Result<OrderID, ErrorCode>
CancelOrder(orderId: OrderID, accountId: AccountID) → Result<void, ErrorCode>
GetOrder(orderId: OrderID) → Result<Order, ErrorCode>
GetOrdersByAccount(accountId: AccountID) → Result<Order[], ErrorCode>
```

## 10. Versioning

**Current Version**: v1.0.0  
**Breaking Changes**: Require new major version  
**Backward Compatibility**: Maintained for 1 major version
