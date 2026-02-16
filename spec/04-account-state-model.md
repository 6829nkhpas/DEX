# Account State Model Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the account state model for the distributed exchange, including balance tracking, margin management, and position accounting.

## 2. Account Structure

### 2.1 Core Account Model

```
Account {
  account_id: UUID,              // Unique account identifier
  external_id: String,           // User-facing account number
  account_type: AccountType,     // SPOT, MARGIN, or FUTURES
  status: AccountStatus,         // ACTIVE, SUSPENDED, CLOSED
  
  created_at: i64,               // Account creation timestamp
  updated_at: i64,               // Last update timestamp
  version: u64,                  // Optimistic locking version
  
  balances: Map<Asset, Balance>, // Multi-currency balances
  positions: Map<Symbol, Position>, // Open positions
  metadata: Map<String, String>, // Extensible metadata
}
```

### 2.2 Account Types

| Type | Leverage | Borrowing | Derivatives |
|------|----------|-----------|-------------|
| SPOT | 1x | No | No |
| MARGIN | Up to 10x | Yes | No |
| FUTURES | Up to 125x | No | Yes |

**Isolation**: Each account type has  isolated state (no cross-contamination)

### 2.3 Account Status

| Status | Can Trade | Can Withdraw | Can Deposit |
|--------|-----------|--------------|-------------|
| ACTIVE | Yes | Yes | Yes |
| SUSPENDED | No | No | Yes |
| CLOSED | No | No | No |

**Status Transitions**:
- ACTIVE ↔ SUSPENDED (manual or automated)
- ACTIVE → CLOSED (irreversible)
- SUSPENDED → CLOSED (irreversible)

## 3. Balance Model

### 3.1 Balance Structure

```
Balance {
  asset: String,           // Asset symbol (BTC, USDT, ETH, etc.)
  
  // Core balance components
  total: Decimal,          // Total balance
  available: Decimal,      // Available for trading/withdrawal
  locked: Decimal,         // Reserved for open orders
  
  // Derived values
  equity: Decimal,         // Total + unrealized PnL
  margin_used: Decimal,    // Margin reserved for positions
  margin_available: Decimal, // Available margin for new positions
  
  updated_at: i64,         // Last update timestamp
  update_seq: u64,         // Update sequence number
}
```

### 3.2 Balance Invariants

**Fundamental Invariant**:
```
total = available + locked
```

**Must Hold Always**:
1. `available >= 0` (no negative available balance)
2. `locked >= 0` (no negative locked balance)
3. `total >= 0` (no negative total balance)
4. `available + locked = total` (conservation)

**Violation**: Critical error requiring immediate halt

### 3.3 Balance Types

#### 3.3.1 Spot Balance
```
available: Free for orders and withdrawals
locked: Reserved for open orders
margin_used: 0 (not applicable)
margin_available: 0 (not applicable)
```

#### 3.3.2 Margin Balance
```
available: Free collateral
locked: Reserved for orders + positions
margin_used: Calculated from open positions
margin_available: total × leverage - margin_used - locked
```

#### 3.3.3 Futures Balance
```
available: Free collateral
locked: Initial margin for positions
margin_used: Maintenance margin requirement
margin_available: (equity - margin_used) / initial_margin_rate
```

## 4. Position Model

### 4.1 Position Structure

```
Position {
  position_id: UUID,
  account_id: UUID,
  symbol: String,              // Trading pair (BTC/USDT)
  
  // Position state
  side: PositionSide,          // LONG or SHORT
  size: Decimal,               // Absolute position size
  entry_price: Decimal,        // Average entry price
  mark_price: Decimal,         // Current mark price (for PnL)
  liquidation_price: Decimal,  // Liquidation trigger price
  
  // PnL tracking
  realized_pnl: Decimal,       // Closed PnL
  unrealized_pnl: Decimal,     // Open PnL
  
  // Margin
  initial_margin: Decimal,     // Margin at position open
  maintenance_margin: Decimal, // Minimum to avoid liquidation
  
  // Timestamps
  opened_at: i64,
  updated_at: i64,
  
  // Metadata
  leverage: u8,                // 1x to 125x
  version: u64,                // Optimistic locking
}
```

### 4.2 Position Sides

- **LONG**: Profit when price increases
- **SHORT**: Profit when price decreases
- **FLAT**: No position (size = 0)

### 4.3 Position PnL Calculation

#### Unrealized PnL
```
LONG: (mark_price - entry_price) × size
SHORT: (entry_price - mark_price) × size
```

#### Realized PnL
Updated on each trade:
```
realized_pnl += (exit_price - avg_entry_price) × quantity_closed
```

### 4.4 Position Lifecycle

```
null → OPEN (first trade)
OPEN → OPEN (additional trades same direction)
OPEN → REDUCED (partial close)
OPEN → CLOSED (full close)
OPEN → REVERSED (close + open opposite direction)
OPEN → LIQUIDATED (forced close)
```

## 5. State Updates

### 5.1 Order Placement

**Effect on Balance**:
```
locked += order_margin_requirement
available -= order_margin_requirement
```

**Margin Requirement**:
- Spot: `quantity × price` (full value)
- Margin: `(quantity × price) / leverage`
- Futures: `(quantity × price) × initial_margin_rate`

**Validation**: `available >= order_margin_requirement`

### 5.2 Order Cancellation

**Effect on Balance**:
```
locked -= order_margin_requirement
available += order_margin_requirement
```

**Atomicity**: Must reverse order placement exactly

### 5.3 Trade Execution

**For Buyer**:
```
locked -= order_margin_used
total += quantity_bought
available += quantity_bought
quote_balance -= quantity × price + fee
```

**For Seller**:
```
locked -= quantity_sold
total -= quantity_sold
quote_balance += quantity × price - fee
```

### 5.4 Position Update

**Opening Position**:
```
size += trade_quantity
entry_price = weighted_average(existing, new_trade)
initial_margin += trade_margin
```

**Closing Position**:
```
size -= trade_quantity
realized_pnl += (exit_price - entry_price) × quantity
If size = 0: position → CLOSED
```

### 5.5 Mark Price Update

**Trigger**: Periodic (e.g., every second) or on significant price move  
**Effect**:
```
mark_price = current_index_price
unrealized_pnl = (mark_price - entry_price) × size
equity = total + unrealized_pnl
```

**Risk Check**: If `equity < maintenance_margin` → trigger liquidation

## 6. Atomic State Transitions

### 6.1 Transaction Boundaries

All state updates MUST be atomic:
1. Order placement: Balance update + Order creation
2. Order cancel: Balance update + Order state update
3. Trade execution: Both accounts + Both orders + Position updates
4. Liquidation: Position close + Balance update + Fee collection

**Rollback**: Any failure rolls back entire transaction

### 6.2 Optimistic Locking

```sql
UPDATE accounts 
SET balance = ?, version = version + 1
WHERE account_id = ? AND version = ?
```

**Conflict Resolution**: Retry with exponential backoff

### 6.3 Sequence Numbers

Every state update assigned monotonic sequence number:
```
AccountUpdate {
  account_id: UUID,
  sequence: u64,           // Strictly increasing
  update_type: UpdateType,
  delta: Decimal,
  balance_after: Decimal,
  timestamp: i64,
}
```

**Guarantee**: No gaps in sequence (enables replay)

## 7. Multi-Asset Support

### 7.1 Asset Registry

```
Asset {
  symbol: String,          // BTC, USDT, ETH
  name: String,            // Bitcoin, Tether USD
  decimals: u8,            // Precision (8 for BTC, 6 for USDT)
  min_withdrawal: Decimal,
  withdrawal_fee: Decimal,
  is_collateral: bool,     // Can use as margin
  collateral_weight: Decimal, // Haircut (0.9 = 90% value)
}
```

### 7.2 Cross-Collateral

For margin/futures accounts:
```
total_collateral_value = Σ (balance[asset] × price[asset] × weight[asset])
```

**Example**:
- 1 BTC @ $50,000 × 0.95 weight = $47,500
- 10,000 USDT @ $1 × 1.0 weight = $10,000
- Total collateral: $57,500

## 8. Risk Calculations

### 8.1 Margin Ratio

```
margin_ratio = equity / maintenance_margin
```

**Health Levels**:
- > 2.0: Healthy
- 1.5 - 2.0: Warning
- 1.1 - 1.5: Danger
- < 1.1: Liquidation triggered

### 8.2 Buying Power

```
buying_power = (equity - margin_used) × leverage
```

**Max Order Size**:
```
max_order = min(
  buying_power,
  position_limit - current_position,
  account_tier_limit
)
```

## 9. Account Aggregates

### 9.1 Portfolio Value

```
portfolio_value = Σ (balance[asset] × price[asset]) 
                + Σ (position[symbol].unrealized_pnl)
```

### 9.2 24h PnL

```
pnl_24h = Σ realized_pnl (last 24h) 
        + Σ (unrealized_pnl_now - unrealized_pnl_24h_ago)
```

### 9.3 Lifetime Statistics

```
AccountStats {
  total_trades: u64,
  total_volume: Decimal,
  total_fees_paid: Decimal,
  total_realized_pnl: Decimal,
  win_rate: Decimal,
  sharpe_ratio: Decimal,
}
```

## 10. Persistence Schema

### 10.1 Primary Storage

```sql
CREATE TABLE accounts (
  account_id UUID PRIMARY KEY,
  account_type VARCHAR(20) NOT NULL,
  status VARCHAR(20) NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  version BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE balances (
  account_id UUID,
  asset VARCHAR(10),
  total DECIMAL(36,18) NOT NULL,
  available DECIMAL(36,18) NOT NULL,
  locked DECIMAL(36,18) NOT NULL,
  updated_at BIGINT NOT NULL,
  update_seq BIGINT NOT NULL,
  PRIMARY KEY (account_id, asset),
  CONSTRAINT check_balance CHECK (available + locked = total),
  CONSTRAINT check_non_negative CHECK (available >= 0 AND locked >= 0)
);

CREATE TABLE positions (
  position_id UUID PRIMARY KEY,
  account_id UUID NOT NULL,
  symbol VARCHAR(20) NOT NULL,
  side VARCHAR(10) NOT NULL,
  size DECIMAL(36,18) NOT NULL,
  entry_price DECIMAL(36,18) NOT NULL,
  realized_pnl DECIMAL(36,18) NOT NULL DEFAULT 0,
  opened_at BIGINT NOT NULL,
  version BIGINT NOT NULL DEFAULT 0
);
```

### 10.2 Event Journal

```sql
CREATE TABLE account_events (
  event_id UUID PRIMARY KEY,
  account_id UUID NOT NULL,
  sequence BIGINT NOT NULL,
  event_type VARCHAR(50) NOT NULL,
  payload JSONB NOT NULL,
  timestamp BIGINT NOT NULL,
  UNIQUE (account_id, sequence)
);
```

**Index**: `(account_id, sequence)` for efficient replay

## 11. Deterministic Operations

### 11.1 Balance Updates

**Requirement**: Given same events, same final balance  
**Enforcement**:
- No floating-point math (use fixed-point Decimal)
- Deterministic rounding (HALF_UP)
- Ordered event processing (by sequence number)

### 11.2 Position Calculations

**Average Entry Price**:
```
new_avg = (old_avg × old_size + trade_price × trade_size) / (old_size + trade_size)
```

**Deterministic**: Pure mathematical function

### 11.3 Replay Guarantee

Given account event journal, can reconstruct exact:
- Balance state
- Position state  
- PnL history
- Margin utilization

## 12. Performance Requirements

- Balance query: < 1ms
- Balance update: < 5ms
- Position calculation: < 2ms
- PnL update: < 10ms (all positions)
- Account state snapshot: < 50ms

## 13. Invariants

1. **Balance Conservation**: Σ(all_balances) + fees_collected = constant
2. **Position Consistency**: position.size × entry_price = initial_margin × leverage
3. **Margin Safety**: equity >= maintenance_margin (or liquidate)
4. **No Negative Balances**: available >= 0, locked >= 0, total >= 0
5. **Sequence Continuity**: No gaps in account update sequences

## 14. Versioning

**Current Version**: v1.0.0  
**Schema Changes**: Versioned migrations  
**Field Additions**: Backward compatible  
**Breaking Changes**: New major version + migration path
