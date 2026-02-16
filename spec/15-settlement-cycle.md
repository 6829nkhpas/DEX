# Settlement Cycle Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the settlement cycle for trades in the distributed exchange, establishing timeframes and processes for finalizing transactions.

## 2. Settlement Model

### 2.1 Type: T+0 (Real-Time Gross Settlement)

**Definition**: Trade settles immediately after execution

**Timeline**:
```
Time 0:     Trade executed (matching engine)
Time +1ms:  Fees calculated
Time +2ms:  Balances updated
Time +5ms:  Positions updated
Time +10ms: Settlement complete (irrevocable)
```

**Finality**: Immediate (no waiting period)

## 3. Settlement Phases

### 3.1 Phase 1: Trade Capture

**Trigger**: Matching engine generates trade  
**Duration**: < 100μs  
**Actions**:
- Assign TradeID
- Record price, quantity, parties
- Emit TradeExecuted event

### 3.2 Phase 2: Validation

**Duration**: < 50μs  
**Checks**:
- Both orders still valid
- Accounts still active
- No duplicate trade_id
- Quantities consistent

### 3.3 Phase 3: Fee Calculation

**Duration**: < 50μs  
**Process**:
```
maker_fee = trade_value × maker_fee_rate
taker_fee = trade_value × taker_fee_rate
total_value = quantity × price
net_maker = total_value - maker_fee
net_taker = total_value + taker_fee
```

### 3.4 Phase 4: Balance Update

**Duration**: < 1ms  
**Atomicity**: Both accounts updated or neither

**Maker (Seller)**:
```
base_asset -= quantity
quote_asset += total_value - maker_fee
```

**Taker (Buyer)**:
```
base_asset += quantity
quote_asset -= total_value + taker_fee
```

### 3.5 Phase 5: Position Update

**Duration**: < 1ms

**For Spot**: No positions (skip)  
**For Margin/Futures**:
```
Update position size
Recalculate average entry price
Update unrealized PnL
Adjust margin requirements
```

### 3.6 Phase 6: Finalization

**Duration**: < 1ms  
**Actions**:
- Mark trade as SETTLED
- Emit TradeSettled event
- Release order margin (if order filled)
- Update market data

**Irreversibility**: After this phase, NO ROLLBACK

## 4. Settlement Guarantees

### 4.1 Atomicity

**Guarantee**: All settlement steps succeed or all fail

**Implementation**: Database transaction
```sql
BEGIN TRANSACTION;
  UPDATE balances SET ... WHERE account_id = maker;
  UPDATE balances SET ... WHERE account_id = taker;
  UPDATE positions SET ... WHERE position_id = maker_pos;
  UPDATE positions SET ... WHERE position_id = taker_pos;
  INSERT INTO settlements VALUES (...);
COMMIT;
```

**Rollback**: Any failure rolls back entire transaction

### 4.2 Finality

**Guarantee**: Settled trades NEVER reversed

**Exceptions**: None (not even admin override)

**Rationale**: Financial integrity requires absolute finality

### 4.3 Idempotency

**Guarantee**: Can attempt settlement multiple times safely

**Implementation**: Unique constraint on trade_id in settlements table
```sql
INSERT INTO settlements (trade_id, ...) 
VALUES (?, ...) 
ON CONFLICT (trade_id) DO NOTHING;
```

**Result**: Duplicate settlement attempts are ignored

## 5. Settlement Cycle Alternatives

### 5.1 T+1 (Next Day Settlement)

**Use Case**: Traditional securities exchanges  
**Not Used**: Our exchange uses T+0

**Process** (if implemented):
```
Day 1: Trade executes, mark as PENDING_SETTLEMENT
Day 2: Batch settle all Day 1 trades at midnight
```

### 5.2 Netting

**Use Case**: Reduce settlement volume  
**Process**: Aggregate offsetting trades

**Example**:
```
User A → B: 1 BTC at $50k
User B → A: 0.5 BTC at $50k
Net: A owes B 0.5 BTC
```

**Not Used**: Our exchange does gross settlement (every trade settles)

### 5.3 Continuous Linked Settlement (CLS)

**Use Case**: FX markets to eliminate settlement risk  
**Not Applicable**: Crypto markets don't have Herstatt risk

## 6. Settlement Risk

### 6.1 Counterparty Risk

**Risk**: Counterparty defaults before settlement  
**Mitigation**: T+0 settlement (immediate)  
**Residual Risk**: ~10ms window (negligible)

### 6.2 Operational Risk

**Risk**: System failure during settlement  
**Mitigation**:
- Atomic transactions (all-or-nothing)
- Idempotent settlement
- Automatic retry

### 6.3 Liquidity Risk

**Risk**: Insufficient balance to settle  
**Mitigation**: Pre-settlement balance check (during matching)  
**Impossibility**: Can't match if insufficient funds

## 7. Settlement Monitoring

### 7.1 Metrics

- Settlement latency (p50, p99, p999)
- Settlement failure rate (should be 0%)
- Pending settlements (should be ~0)
- Settlement throughput (trades/sec)

### 7.2 Alerts

| Condition | Severity |
|-----------|----------|
| Settlement latency > 100ms | WARNING |
| Settlement failure | CRITICAL |
| Pending settlements > 100 | WARNING |

## 8. Settlement Journal

### 8.1 Purpose

Immutable record of all settlements

### 8.2 Schema

```sql
CREATE TABLE settlements (
  trade_id UUID PRIMARY KEY,
  sequence BIGINT NOT NULL,
  maker_account UUID NOT NULL,
  taker_account UUID NOT NULL,
  symbol VARCHAR(20) NOT NULL,
  quantity DECIMAL(36,18) NOT NULL,
  price DECIMAL(36,18) NOT NULL,
  maker_fee DECIMAL(36,18) NOT NULL,
  taker_fee DECIMAL(36,18) NOT NULL,
  settled_at BIGINT NOT NULL,
  settlement_status VARCHAR(20) NOT NULL
);
```

### 8.3 Reconciliation

**Daily Process**:
```
1. Sum all settlements: Σ(quantity × price)
2. Sum all balance changes: Σ(balance_delta)
3. Verify: settlements_total = balance_changes_total
```

**Discrepancy**: Immediate investigation (data integrity issue)

## 9. Cross-Asset Settlement

### 9.1 Spot Trading

**Assets Involved**: 2 (base and quote)

**Example** (BTC/USDT):
```
Buyer receives: 1 BTC
Buyer pays: 50,000 USDT + fee
Seller receives: 50,000 USDT - fee
Seller pays: 1 BTC
```

### 9.2 Margin Trading

**Assets Involved**: 2 + collateral asset (if different)

**Example**:
```
User borrows 50,000 USDT
Buys 1 BTC
Collateral (ETH) remains unchanged
Liability increases by 50,000 USDT
```

### 9.3 Futures

**Assets Involved**: 1 (quote asset only, no delivery)

**Example**:
```
Long opens 1 BTC futures contract
Margin reserved: 5,000 USDT
No BTC changes hands (cash-settled)
```

## 10. Settlement Failure Recovery

### 10.1 Detection

**Trigger**: Settlement transaction fails

**Causes**:
- Database deadlock
- Insufficient balance (shouldn't happen)
- System crash

### 10.2 Recovery

**Process**:
```
1. Log failed settlement attempt
2. Rollback database transaction
3. Retry settlement (idempotent)
4. If retry fails 3 times: Alert + manual intervention
```

### 10.3 Manual Intervention

**When**: Automatic retry exhausted

**Process**:
1. Investigate root cause
2. Fix underlying issue
3. Manually trigger settlement
4. Verify account balances correct

## 11. Regulatory Compliance

### 11.1 Audit Trail

**Requirement**: Complete settlement history

**Retention**: Permanent (never delete)

**Format**: Immutable append-only log

### 11.2 Reporting

**Daily Report**:
- Total settled volume
- Number of settlements
- Average settlement time
- Failed settlements (should be 0)

### 11.3 Dispute Resolution

**User Claim**: "Trade settled incorrectly"

**Evidence**:
```
1. Query settlement journal by trade_id
2. Show:
   - Execution price
   - Quantity
   - Fees charged
   - Balance before/after
3. Compare with order details
```

## 12. Performance Targets

- Settlement latency: < 10ms (p99)
- Throughput: 100,000 settlements/sec
- Failure rate: 0%
- Data consistency: 100%

## 13. Invariants

1. **Conservation**: Σ(all_balances) = constant (minus fees collected)
2. **Completeness**: Every trade has corresponding settlement
3. **Uniqueness**: Each trade_id settled exactly once
4. **Finality**: Settled trades immutable

## 14. Versioning

**Current Version**: v1.0.0  
**Settlement Model Change**: Major version bump + migration  
**Timeline Change**: Requires regulatory approval (if moving from T+0)
