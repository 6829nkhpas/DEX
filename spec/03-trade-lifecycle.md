# Trade Lifecycle Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the complete lifecycle of a trade (match execution) in the distributed exchange, from matching event to final settlement. A trade is created when two orders match.

## 2. Trade Definition

**Trade**: An atomic exchange of assets between two parties (maker and taker) at a specific price and quantity.

### 2.1 Trade Components

```
Trade {
  trade_id: UUID,           // Unique trade identifier
  sequence: u64,            // Global trade sequence number
  symbol: String,           // Trading pair (e.g., "BTC/USDT")
  
  maker_order_id: UUID,     // Passive order (provided liquidity)
  taker_order_id: UUID,     // Aggressive order (took liquidity)
  
  maker_account_id: UUID,   // Maker account
  taker_account_id: UUID,   // Taker account
  
  side: TradeSide,          // BUY or SELL (from taker perspective)
  price: Decimal,           // Execution price
  quantity: Decimal,        // Matched quantity
  
  maker_fee: Decimal,       // Fee charged to maker
  taker_fee: Decimal,       // Fee charged to taker
  
  executed_at: i64,         // Execution timestamp (nanos)
  settled_at: i64,          // Settlement timestamp (nanos)
  
  state: TradeState,        // Current trade state
}
```

## 3. Lifecycle Phases

### 3.1 Phase 1: Matching

**Trigger**: Matching engine finds compatible orders  
**Duration**: < 100μs  
**Output**: TradeExecuted event

#### Matching Conditions
1. Price compatibility: `taker_price >= maker_price` (for buy)
2. Quantity availability: Both orders have sufficient remaining quantity
3. Self-trade check: `maker_account != taker_account`
4. Symbol match: Both orders for same trading pair

#### Atomic Actions
1. Assign unique TradeID (UUID v7)
2. Assign global sequence number (monotonic counter)
3. Calculate matched quantity (min of both remaining quantities)
4. Lock execution price (maker's order price)
5. Emit `TradeExecuted` event
6. State → MATCHED

**Failure**: If any action fails, rollback entirely (no partial state)

### 3.2 Phase 2: Fee Calculation

**Trigger**: Immediately after matching  
**Duration**: < 50μs  
**Determinism**: Pure function of trade parameters

#### Fee Formula
```
maker_fee = quantity × price × maker_fee_rate
taker_fee = quantity × price × taker_fee_rate
```

**Fee Rates** (from fee system spec):
- Maker: -0.01% (rebate) to 0.10%
- Taker: 0.02% to 0.30%

#### Fee Precision
- Calculate with full precision (18 decimal places)
- Round HALF_UP to 8 decimal places for settlement
- Never round down (prevents fee evasion)

### 3.3 Phase 3: Order Update

**Trigger**: After fee calculation  
**Duration**: < 100μs  
**Atomicity**: Must update both orders or rollback

#### Updates for Both Orders
1. Increment `filled_quantity` by matched quantity
2. Decrement `remaining_quantity` by matched quantity
3. Update order state if necessary:
   - If `remaining_quantity = 0` → FILLED
   - Else if `filled_quantity > 0` → PARTIAL
4. Release margin for filled portion
5. Append trade_id to order's `trades[]` array

**Consistency Check**: 
```
filled_quantity + remaining_quantity = total_quantity
```

### 3.4 Phase 4: Position Update

**Trigger**: After order update  
**Duration**: < 200μs  
**Atomicity**: All position updates must succeed

#### Maker Position Update
```
If maker side = SELL:
  base_balance -= quantity
  quote_balance += quantity × price - maker_fee
  
If maker side = BUY:
  base_balance += quantity
  quote_balance -= quantity × price + maker_fee
```

#### Taker Position Update
```
If taker side = BUY:
  base_balance += quantity
  quote_balance -= quantity × price + taker_fee
  
If taker side = SELL:
  base_balance -= quantity
  quote_balance += quantity × price - taker_fee
```

**Invariant**: Sum of all account balances remains constant (minus fees)

### 3.5 Phase 5: Settlement

**Trigger**: After position update  
**Duration**: < 100μs  
**Finality**: T+0 (immediate settlement)

#### Actions
1. Persist position changes to account ledger
2. Record fee collection
3. Update `settled_at` timestamp
4. State → SETTLED
5. Emit `TradeSettled` event

#### Settlement Guarantee
- No reversals or chargebacks
- Final and irreversible
- Byzantine fault tolerant (if using consensus)

### 3.6 Phase 6: Reporting

**Trigger**: After settlement  
**Duration**: Asynchronous (non-blocking)

#### Actions
1. Update market data (last price, volume, OHLCV)
2. Emit `MarketDataUpdate` event
3. Update account transaction history
4. Trigger risk engine recalculation (if applicable)

**Failure Handling**: Retry indefinitely (eventually consistent)

## 4. Trade States

| State | Description | Terminal |
|-------|-------------|----------|
| MATCHED | Trade created, pending settlement | No |
| SETTLED | Fully settled to accounts | Yes |
| FAILED | Settlement failed (rare) | Yes |

**Note**: FAILED state only occurs during catastrophic system failure

## 5. State Transitions

```
null → MATCHED (matching completed)
MATCHED → SETTLED (settlement completed)
MATCHED → FAILED (settlement error - requires manual intervention)
```

**Normal Path**: null → MATCHED → SETTLED (< 1ms total)

## 6. Event Emissions

### 6.1 TradeExecuted Event
```json
{
  "event_type": "TradeExecuted",
  "trade_id": "01939d7f-8e4a-7890-a123-456789abcdef",
  "sequence": 123456789,
  "symbol": "BTC/USDT",
  "maker_order_id": "...",
  "taker_order_id": "...",
  "price": "50000.00",
  "quantity": "0.5",
  "side": "BUY",
  "executed_at": 1708123456789000000,
  "maker_account": "...",
  "taker_account": "..."
}
```

### 6.2 TradeSettled Event
```json
{
  "event_type": "TradeSettled",
  "trade_id": "01939d7f-8e4a-7890-a123-456789abcdef",
  "sequence": 123456790,
  "maker_fee": "1.25",
  "taker_fee": "12.50",
  "settled_at": 1708123456790000000
}
```

## 7. Determinism Requirements

### 7.1 Matching Determinism
- Same order book state → same trades
- Price-time priority strictly enforced
- No randomness in matching algorithm
- Sequence numbers strictly increasing

### 7.2 Fee Calculation Determinism
- Pure mathematical function
- No floating-point arithmetic (use fixed-point decimals)
- Identical fee rates across all nodes
- Rounding mode: HALF_UP (deterministic)

### 7.3 Replay Guarantee
Given identical:
1. Order book state
2. Timestamp source
3. Sequence counter

Replay produces identical trades with identical:
- TradeIDs (if using deterministic UUID generation)
- Prices
- Quantities
- Fees
- Sequence numbers

## 8. Failure Recovery

### 8.1 Crash During Matching
**Recovery**: 
- Matching engine replays order book from journal
- Recomputes matches deterministically
- Emits missed TradeExecuted events

**Idempotency**: TradeID prevents duplicate matches

### 8.2 Crash During Settlement
**Recovery**:
- Query trade state from database
- If MATCHED, retry settlement
- Settlement is idempotent (use trade_id as idempotency key)

**Double-Settlement Prevention**: 
```sql
UPDATE accounts 
SET balance = balance + delta
WHERE account_id = ? AND NOT EXISTS (
  SELECT 1 FROM settlements WHERE trade_id = ?
)
```

### 8.3 Crash During Reporting
**Recovery**:
- Reporting is asynchronous and eventually consistent
- Retry failed reports from event log
- Non-critical path (doesn't block trading)

## 9. Performance Targets

- Matching to settlement: < 1ms (p99)
- Trade throughput: 100,000 trades/sec per symbol
- Event emission latency: < 100μs (p99)
- Settlement finality: T+0 (immediate)

## 10. Invariants

1. **Conservation of Value**: Total value in system constant (minus fees)
2. **Trade-Order Consistency**: Every trade references valid orders
3. **Sequence Uniqueness**: Each trade has unique sequence number
4. **Price Validity**: Trade price = maker order price (price-time priority)
5. **Quantity Validity**: Trade quantity ≤ min(maker_remaining, taker_remaining)
6. **No Self-Trades**: maker_account ≠ taker_account
7. **State Progression**: Trades always progress from MATCHED → SETTLED

## 11. Matching Algorithm

### 11.1 Price-Time Priority
1. Sort orders by price (best first)
2. Within same price level, sort by timestamp (earliest first)
3. Match incoming order against best counterparty
4. Repeat until incoming order filled or no matches

### 11.2 Pro-Rata Matching (Alternative)
- At same price level, allocate proportionally
- Minimum allocation: 1 base unit
- Remainder to earliest order (time priority tiebreaker)

**Default**: Price-Time Priority (most exchanges use this)

## 12. Trade History

### 12.1 Storage
- Infinite retention (never delete trades)
- Indexed by: trade_id, maker_order_id, taker_order_id, symbol, executed_at
- Partitioned by time for performance

### 12.2 Queries
```
GetTrade(trade_id) → Trade
GetTradesByOrder(order_id) → Trade[]
GetTradesByAccount(account_id, start_time, end_time) → Trade[]
GetTradesBySymbol(symbol, start_time, end_time) → Trade[]
GetRecentTrades(symbol, limit) → Trade[]  // For market data
```

## 13. Audit Trail

Every trade MUST be:
1. Logged to immutable journal
2. Cryptographically signed (optional but recommended)
3. Timestamped with exchange time
4. Linked to source orders
5. Reconstructible from event log

## 14. Settlement Cycle

**Type**: Real-Time Gross Settlement (RTGS)  
**Delay**: None (T+0)  
**Batching**: None (each trade settles individually)  
**Finality**: Immediate and irreversible

**Alternative** (for high-frequency systems):
- Micro-batching: Settle every 100ms
- Netting: Aggregate offsetting trades
- Trade-off: Latency vs throughput

## 15. Versioning

**Current Version**: v1.0.0  
**Breaking Changes**: New major version required  
**Field Additions**: Append-only (maintain backward compatibility)  
**Deprecated Fields**: Mark deprecated, never remove
