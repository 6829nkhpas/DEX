# Liquidation Process Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the liquidation process for underwater positions, including trigger conditions, execution mechanics, insurance fund usage, and auto-deleveraging.

## 2. Liquidation Trigger

### 2.1 Trigger Condition

**Primary Trigger**:
```
margin_ratio = account_equity / maintenance_margin < 1.1
```

**Where**:
```
account_equity = total_balance + unrealized_pnl
maintenance_margin = Σ (position_value × mm_rate)
```

### 2.2 Continuous Monitoring

**Frequency**: Every mark price update (typically 1 second)  
**Latency Target**: Liquidation initiated within 100ms of trigger  
**Priority**: Real-time highest-priority task

### 2.3 Grace Period

**Standard**: None (immediate liquidation when margin_ratio < 1.1)  
**Exception**: If 1.1 <= margin_ratio < 1.2, user has 15 minutes to add margin  
**Override**: System can disable grace period during high volatility

## 3. Liquidation Types

### 3.1 Partial Liquidation

**When**: Account has multiple positions  
**Strategy**: Close smallest position first  
**Goal**: Restore margin_ratio >= 1.5

**Algorithm**:
```
WHILE margin_ratio < 1.5 AND positions.count > 0:
  position = find_smallest_position()
  liquidate(position)
  recalculate_margin_ratio()
```

**Advantage**: Preserves larger positions, minimizes impact

### 3.2 Full Liquidation

**When**: 
- Margin ratio < 1.05 (critical level)
- Single position account
- Partial liquidation insufficient

**Action**: Close all positions immediately  
**Priority**: Speed over price optimization

## 4. Liquidation Execution

### 4.1 Phase 1: Position Acquisition

**Action**: Exchange takes over position  
**Mechanism**:
```
1. Mark position as "IN_LIQUIDATION"
2. Transfer ownership to liquidation engine
3. Cancel all open orders for this account
4. Lock account from new orders
```

**Timestamp**: `liquidation_started_at = now()`  
**Atomicity**: All steps succeed or rollback

### 4.2 Phase 2: Market Order Execution

**Strategy**: Place market order to close position  
**Order Type**: Market (immediate execution)  
**Direction**: Opposite of position (LONG position → SELL order)

**Execution Parameters**:
```
quantity = position.size
side = opposite(position.side)
order_type = MARKET
time_in_force = IOC (Immediate or Cancel)
```

### 4.3 Phase 3: Price Realization

**Scenarios**:

#### Scenario A: Profitable Liquidation
```
liquidation_price > bankruptcy_price

Result: Remaining equity returned to user + insurance fund contribution
```

#### Scenario B: Breakeven Liquidation
```
liquidation_price = bankruptcy_price

Result: No equity returned, no insurance fund usage
```

#### Scenario C: Underwater Liquidation
```
liquidation_price < bankruptcy_price

Result: Insurance fund covers loss
```

**Bankruptcy Price**:
```
LONG: bankruptcy_price = entry_price - (initial_margin / position_size)
SHORT: bankruptcy_price = entry_price + (initial_margin / position_size)
```

### 4.4 Phase 4: Settlement

**Actions**:
1. Calculate actual PnL from liquidation
2. Deduct liquidation fee (see Section 7)
3. Update account balance
4. Transfer deficit to insurance fund (if any)
5. Emit `PositionLiquidated` event

## 5. Liquidation Pricing

### 5.1 Execution Method

**Primary**: Market order against order book  
**Fallback**: If insufficient liquidity, use ADL (Auto-Deleveraging)

### 5.2 Price Improvement

**Incentive**: Liquidation engine tries to get better than bankruptcy price  
**Time Limit**: 30 seconds maximum  
**Timeout Action**: If not filled, proceed to ADL

**Algorithm**:
```
1. Place market order
2. Wait up to 30s for fill
3. If partially filled: reduce order, continue
4. If no fill or timeout: trigger ADL
```

## 6. Insurance Fund

### 6.1 Purpose

Cover losses when liquidation price is worse than bankruptcy price

### 6.2 Funding Sources

1. **Profitable Liquidations**: Surplus added to fund
2. **Liquidation Fees**: Percentage goes to insurance fund
3. **Trading Fees**: 10% of all trading fees
4. **Initial Capitalization**: Exchange contribution

### 6.3 Usage

**When**: Liquidation results in deficit  
**Amount**: `max(0, bankruptcy_price - actual_liquidation_price) × position_size`

**Calculation Example**:
```
Position: LONG 10 BTC
Entry: $50,000
Initial Margin: $5,000 (10x leverage)
Bankruptcy Price: $50,000 - $500 = $49,500
Actual Liquidation: $49,200
Loss: ($49,500 - $49,200) × 10 = $3,000
Insurance Fund Covers: $3,000
```

### 6.4 Insurance Fund Depletion

**If Balance < 0**: Trigger Auto-Deleveraging (ADL)  
**Alert**: If balance < 10% of 30-day average, alert risk team  
**Circuit Breaker**: If balance < 5% of open interest, pause new positions

## 7. Liquidation Fees

### 7.1 Fee Structure

| Margin Ratio at Liquidation | Liquidation Fee |
|-----------------------------|-----------------|
| 1.05 - 1.10 | 0.50% |
| 0.50 - 1.05 | 1.00% |
| < 0.50 | 2.00% |

**Calculation**: `fee = position_value × fee_rate`

### 7.2 Fee Allocation

- 50% to insurance fund
- 30% to liquidation engine (operational costs)
- 20% to exchange revenue

### 7.3 Fee Cap

**Maximum**: 5% of position value  
**Rationale**: Prevent excessive fees on highly leveraged positions

## 8. Auto-Deleveraging (ADL)

### 8.1 Trigger Conditions

ADL activates when:
1. Insurance fund balance < 0, OR
2. Liquidation cannot execute in order book (insufficient liquidity), OR
3. Position too large to liquidate without excessive slippage

### 8.2 ADL Queue

**Ranking System**:
```
adl_score = profit_percentage × leverage

profit_percentage = unrealized_pnl / initial_margin
```

**Queue Order**: Highest ADL score deleveraged first

**Example**:
```
User A: +200% profit, 100x leverage → score = 200
User B: +50% profit, 50x leverage → score = 25  
User C: +100% profit, 10x leverage → score = 10

ADL Order: A, then B, then C
```

### 8.3 ADL Execution

**Process**:
1. Identify counterparty from ADL queue (highest score)
2. Force-close their position at bankruptcy price
3. Use proceeds to settle liquidated position
4. Repeat until deficit covered

**Notification**: 
- Users in ADL queue top 20% see warning indicator
- Email notification if selected for ADL
- On-chain verification (if applicable)

### 8.4 ADL Compensation

**None**: ADL is part of exchange risk model  
**Price**: Counterparty closes at current mark price (fair)  
**Alternative Proposed**: Small fee compensation (0.02% of closed value)

## 9. Liquidation States

### 9.1 State Machine

```
NORMAL → LIQUIDATION_WARNING (margin_ratio < 1.5)
LIQUIDATION_WARNING → MARGIN_CALL (margin_ratio < 1.2)
MARGIN_CALL → IN_LIQUIDATION (margin_ratio < 1.1)
IN_LIQUIDATION → LIQUIDATED (position closed)
```

**Terminal States**: LIQUIDATED, ADL_DELEVERAGED

### 9.2 State-Specific Actions

| State | Orders Allowed | Position Changes | Withdrawals |
|-------|----------------|------------------|-------------|
| NORMAL | Yes | Yes | Yes |
| LIQUIDATION_WARNING | Yes | Yes | Limited |
| MARGIN_CALL | Only reduce | Only reduce | No |
| IN_LIQUIDATION | No | Only liquidation engine | No |
| LIQUIDATED | After settlement | After settlement | After settlement |

## 10. Edge Cases

### 10.1 Flash Crashes

**Problem**: Price spikes cause mass liquidations  
**Mitigation**:
- Circuit breakers (pause trading if price moves > 10% in 1 min)
- Mark price uses index of 3+ exchanges (resist manipulation)
- Liquidation delay optional (5 second delay during extreme volatility)

### 10.2 Negative Account Balance

**Should Never Happen**: Liquidation should trigger before this  
**If Occurs**:
1. Write off to insurance fund
2. Account flagged for review
3. Log incident for post-mortem

**Prevention**: Conservative maintenance margin rates

### 10.3 Cascading Liquidations

**Problem**: One liquidation triggers more liquidations  
**Detection**: > 5% of open interest liquidated in 5 minutes  
**Response**:
- Pause new position opening
- Increase maintenance margin requirements temporarily
- Alert risk team

### 10.4 Market Closed / No Liquidity

**For Traditional Markets**:
- Use last known price for margin calculation
- Queue liquidations for market open
- Charge higher liquidation fee (increased risk)

**For 24/7 Crypto Markets**: Not applicable (always liquid)

## 11. Determinism Requirements

### 11.1 Liquidation Trigger

**Deterministic Inputs**:
- Mark price (from index calculation)
- Account equity (from deterministic balance calculations)
- Maintenance margin (from deterministic margin formulas)

**No Randomness**: Same inputs → same liquidation decision

### 11.2 Execution Order

**For Multiple Liquidations**:
- Sort by margin_ratio (lowest first = most urgent)
- Tiebreaker: account_id lexicographic order
- Deterministic queue ensures replay consistency

### 11.3 Event Replay

Given:
- Order book state
- Mark price history
- Account state history

Can deterministically replay:
- Liquidation trigger points
- Liquidation execution prices
- Insurance fund usage
- ADL counterparty selection

## 12. Performance Requirements

- Liquidation trigger detection: < 100ms
- Position takeover: < 500ms
- Market order placement: < 100ms
- Full liquidation (trigger to settlement): < 2 seconds (p99)
- ADL queue calculation: < 1 second
- Mass liquidation handling: 10,000 positions/minute

## 13. Monitoring and Alerts

### 13.1 Real-Time Metrics

- Number of accounts in margin call
- Number of liquidations in progress
- Insurance fund balance
- ADL queue depth
- Average liquidation profit/loss

### 13.2 Alert Thresholds

| Condition | Severity | Action |
|-----------|----------|--------|
| Insurance fund < 10% of norm | WARNING | Email risk team |
| Insurance fund < 0 | CRITICAL | Activate ADL + page on-call |
| > 100 liquidations/minute | WARNING | Check for market event |
| > 1000 liquidations/minute | CRITICAL | Circuit breaker consideration |

## 14. User Interface

### 14.1 Pre-Liquidation Warnings

**Margin Ratio Display**:
- Green: > 2.0
- Yellow: 1.5 - 2.0
- Orange: 1.2 - 1.5
- Red: < 1.2

**Notifications**:
- Push notification at margin_ratio < 1.5
- Email + SMS at margin_ratio < 1.2
- In-app modal at margin_ratio < 1.15

### 14.2 Liquidation History

**Display**:
```
Liquidation {
  timestamp: datetime,
  symbol: string,
  position_size: decimal,
  entry_price: decimal,
  liquidation_price: decimal,
  loss: decimal,
  liquidation_fee: decimal,
  method: "Market" | "ADL"
}
```

## 15. Compliance and Auditing

### 15.1 Audit Trail

Every liquidation MUST log:
1. Pre-liquidation account state
2. Trigger condition (margin ratio)
3. Execution method (market order / ADL)
4. Execution prices
5. Post-liquidation account state
6. Insurance fund impact

**Retention**: Permanent (never delete)

### 15.2 Dispute Resolution

**User Claims**:
- "Incorrect mark price used"
- "Liquidation too aggressive"
- "System error"

**Process**:
1. Review audit trail
2. Verify mark price accuracy
3. Check for system anomalies
4. Compensation if exchange error (rare)

## 16. Invariants

1. **No Negative Balances**: Liquidation prevents account going negative
2. **Insurance Fund Integrity**: Always resolves deficits (or uses ADL)
3. **Position Closure**: Liquidated position ALWAYS closed (never partial)
4. **Bankruptcy Price**: Calculated consistently for all positions
5. **ADL Fairness**: Queue based on objective, verifiable criteria

## 17. Testing Requirements

### 17.1 Scenarios to Test

- Normal liquidation (market order fills)
- Underwater liquidation (insurance fund usage)
- ADL trigger and execution
- Partial liquidation (multi-position account)
- Cascading liquidations
- Flash crash simulation

### 17.2 Stress Tests

- 10,000 simultaneous liquidations
- Insurance fund depletion
- Zero liquidity scenario
- Extreme price movements (99th percentile)

## 18. Versioning

**Current Version**: v1.0.0  
**Fee Changes**: 7-day notice required  
**Margin Ratio Changes**: 24-hour notice required  
**Breaking Changes**: New major version + grandfather period
