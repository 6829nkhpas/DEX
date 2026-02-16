# Event Taxonomy Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the complete taxonomy of events emitted by the distributed exchange, ensuring consistent event structure and deterministic event ordering.

## 2. Event Structure

### 2.1 Base Event Schema

```
Event {
  event_id: UUID,           // Unique event identifier (UUID v7)
  event_type: String,       // Event type (see taxonomy below)
  sequence: u64,            // Global monotonic sequence number
  timestamp: i64,           // Unix nanoseconds (exchange time)
  source: String,           // Emitting service (matching-engine, settlement, etc.)
  
  payload: JSON,            // Event-specific data
  metadata: {
    version: String,        // Event schema version
    correlation_id: UUID,   // Request correlation ID
    causation_id: UUID,     // Causing event ID
  }
}
```

### 2.2 Event Guarantees

1. **Unique ID**: Every event has unique event_id
2. **Monotonic Sequence**: Sequence numbers strictly increasing (no gaps)
3. **Immutable**: Events never modified after creation
4. **Ordered**: Events can be replayed in sequence order
5. **Complete**: All state changes emit events

## 3. Event Categories

### 3.1 Order Events

#### OrderSubmitted
```json
{
  "event_type": "OrderSubmitted",
  "payload": {
    "order_id": "uuid",
    "account_id": "uuid",
    "symbol": "BTC/USDT",
    "side": "BUY",
    "type": "LIMIT",
    "price": "50000.00",
    "quantity": "1.0",
    "time_in_force": "GTC"
  }
}
```

#### OrderAccepted
```json
{
  "event_type": "OrderAccepted",
  "payload": {
    "order_id": "uuid",
    "state": "PENDING",
    "margin_reserved": "5000.00"
  }
}
```

#### OrderRejected
```json
{
  "event_type": "OrderRejected",
  "payload": {
    "order_id": "uuid",
    "reason": "INSUFFICIENT_BALANCE",
    "message": "Available balance: 4000 USDT, required: 5000 USDT"
  }
}
```

#### OrderPartiallyFilled
```json
{
  "event_type": "OrderPartiallyFilled",
  "payload": {
    "order_id": "uuid",
    "filled_quantity": "0.3",
    "remaining_quantity": "0.7",
    "average_price": "50000.00"
  }
}
```

#### OrderFilled
```json
{
  "event_type": "OrderFilled",
  "payload": {
    "order_id": "uuid",
    "filled_quantity": "1.0",
    "average_price": "50000.00",
    "total_value": "50000.00",
    "total_fee": "50.00"
  }
}
```

#### OrderCanceled
```json
{
  "event_type": "OrderCanceled",
  "payload": {
    "order_id": "uuid",
    "canceled_by": "USER" | "SYSTEM",
    "reason": "USER_REQUESTED",
    "filled_quantity": "0.3",
    "unfilled_quantity": "0.7"
  }
}
```

#### OrderExpired
```json
{
  "event_type": "OrderExpired",
  "payload": {
    "order_id": "uuid",
    "expiry_time": 1708123456789000000,
    "filled_quantity": "0.0"
  }
}
```

### 3.2 Trade Events

#### TradeExecuted
```json
{
  "event_type": "TradeExecuted",
  "payload": {
    "trade_id": "uuid",
    "symbol": "BTC/USDT",
    "maker_order_id": "uuid",
    "taker_order_id": "uuid",
    "maker_account_id": "uuid",
    "taker_account_id": "uuid",
    "price": "50000.00",
    "quantity": "0.5",
    "side": "BUY",
    "executed_at": 1708123456789000000
  }
}
```

#### TradeSettled
```json
{
  "event_type": "TradeSettled",
  "payload": {
    "trade_id": "uuid",
    "maker_fee": "2.50",
    "taker_fee": "25.00",
    "settled_at": 1708123456790000000
  }
}
```

### 3.3 Account Events

#### AccountCreated
```json
{
  "event_type": "AccountCreated",
  "payload": {
    "account_id": "uuid",
    "account_type": "MARGIN",
    "external_id": "ACC-123456"
  }
}
```

#### BalanceUpdated
```json
{
  "event_type": "BalanceUpdated",
  "payload": {
    "account_id": "uuid",
    "asset": "USDT",
    "delta": "1000.00",
    "balance_after": "5000.00",
    "update_reason": "TRADE_SETTLEMENT",
    "reference_id": "trade_uuid"
  }
}
```

#### MarginReserved
```json
{
  "event_type": "MarginReserved",
  "payload": {
    "account_id": "uuid",
    "amount": "5000.00",
    "reason": "ORDER_PLACEMENT",
    "order_id": "uuid"
  }
}
```

#### MarginReleased
```json
{
  "event_type": "MarginReleased",
  "payload": {
    "account_id": "uuid",
    "amount": "5000.00",
    "reason": "ORDER_CANCELED",
    "order_id": "uuid"
  }
}
```

### 3.4 Position Events

#### PositionOpened
```json
{
  "event_type": "PositionOpened",
  "payload": {
    "position_id": "uuid",
    "account_id": "uuid",
    "symbol": "BTC/USDT",
    "side": "LONG",
    "size": "1.0",
    "entry_price": "50000.00",
    "leverage": 10
  }
}
```

#### PositionUpdated
```json
{
  "event_type": "PositionUpdated",
  "payload": {
    "position_id": "uuid",
    "size_delta": "0.5",
    "new_size": "1.5",
    "new_avg_price": "50100.00",
    "unrealized_pnl": "150.00"
  }
}
```

#### PositionClosed
```json
{
  "event_type": "PositionClosed",
  "payload": {
    "position_id": "uuid",
    "close_price": "51000.00",
    "realized_pnl": "1000.00",
    "total_fees": "30.00"
  }
}
```

### 3.5 Liquidation Events

#### LiquidationTriggered
```json
{
  "event_type": "LiquidationTriggered",
  "payload": {
    "account_id": "uuid",
    "margin_ratio": 1.05,
    "equity": "5250.00",
    "maintenance_margin": "5000.00"
  }
}
```

#### PositionLiquidated
```json
{
  "event_type": "PositionLiquidated",
  "payload": {
    "position_id": "uuid",
    "liquidation_price": "49500.00",
    "bankruptcy_price": "49600.00",
    "loss": "100.00",
    "liquidation_fee": "250.00",
    "insurance_fund_contribution": "-100.00"
  }
}
```

#### ADLExecuted
```json
{
  "event_type": "ADLExecuted",
  "payload": {
    "position_id": "uuid",
    "account_id": "uuid",
    "counterparty_account_id": "uuid",
    "close_price": "50000.00",
    "quantity": "0.5"
  }
}
```

### 3.6 System Events

#### MarketDataUpdated
```json
{
  "event_type": "MarketDataUpdated",
  "payload": {
    "symbol": "BTC/USDT",
    "last_price": "50000.00",
    "24h_volume": "1234.56",
    "24h_high": "51000.00",
    "24h_low": "49000.00",
    "mark_price": "50010.00"
  }
}
```

#### MarkPriceUpdated
```json
{
  "event_type": "MarkPriceUpdated",
  "payload": {
    "symbol": "BTC/USDT",
    "mark_price": "50010.00",
    "index_price": "50000.00",
    "funding_rate": "0.0001"
  }
}
```

#### CircuitBreakerTriggered
```json
{
  "event_type": "CircuitBreakerTriggered",
  "payload": {
    "symbol": "BTC/USDT",
    "reason": "PRICE_MOVEMENT_EXCEEDED",
    "price_change": "12.5",
    "duration": 300
  }
}
```

### 3.7 Withdrawal/Deposit Events

#### DepositDetected
```json
{
  "event_type": "DepositDetected",
  "payload": {
    "account_id": "uuid",
    "asset": "BTC",
    "amount": "1.0",
    "tx_id": "blockchain_tx_hash",
    "confirmations": 1
  }
}
```

#### DepositConfirmed
```json
{
  "event_type": "DepositConfirmed",
  "payload": {
    "account_id": "uuid",
    "asset": "BTC",
    "amount": "1.0",
    "tx_id": "blockchain_tx_hash",
    "confirmations": 6
  }
}
```

#### WithdrawalRequested
```json
{
  "event_type": "WithdrawalRequested",
  "payload": {
    "withdrawal_id": "uuid",
    "account_id": "uuid",
    "asset": "BTC",
    "amount": "0.5",
    "destination": "bc1q..."
  }
}
```

#### WithdrawalCompleted
```json
{
  "event_type": "WithdrawalCompleted",
  "payload": {
    "withdrawal_id": "uuid",
    "tx_id": "blockchain_tx_hash",
    "fee": "0.0005"
  }
}
```

## 4. Event Ordering

### 4.1 Sequence Numbers

**Global Sequence**: Single monotonic counter across all events  
**Per-Entity Sequence**: Each account/order/position has own sequence

**Example**:
```
Global: 1000, 1001, 1002, ...
Account A: 10, 11, 12, ...
Account B: 5, 6, 7 ...
```

### 4.2 Causal Ordering

Events MUST respect causality:
```
OrderSubmitted → OrderAccepted → TradeExecuted → TradeSettled
```

**Violation**: System error requiring investigation

## 5. Event Persistence

### 5.1 Event Store

```sql
CREATE TABLE events (
  event_id UUID PRIMARY KEY,
  sequence BIGINT UNIQUE NOT NULL,
  event_type VARCHAR(50) NOT NULL,
  timestamp BIGINT NOT NULL,
  source VARCHAR(50) NOT NULL,
  payload JSONB NOT NULL,
  metadata JSONB NOT NULL
);

CREATE INDEX idx_sequence ON events(sequence);
CREATE INDEX idx_timestamp ON events(timestamp);
CREATE INDEX idx_type ON events(event_type);
```

### 5.2 Retention

**Policy**: Infinite retention (never delete)  
**Archival**: Move events > 1 year to cold storage  
**Compression**: Use JSONB compression

## 6. Event Replay

### 6.1 Full Replay

**Purpose**: Rebuild system state from scratch  
**Process**:
1. Query all events ORDER BY sequence
2. Apply each event to empty state
3. Verify final state matches current state

**Frequency**: Weekly full replay validation

### 6.2 Partial Replay

**Purpose**: Recover specific entity state  
**Process**:
1. Query events WHERE account_id = ? ORDER BY sequence
2. Apply events to reconstruct account state

## 7. Event Subscriptions

### 7.1 Real-Time Streams

**Protocols**: WebSocket, gRPC streaming  
**Filtering**: By account_id, symbol, event_type  
**Guarantee**: At-least-once delivery

### 7.2 Batch Consumers

**Protocols**: Kafka, RabbitMQ  
**Guarantee**: Exactly-once processing (via idempotency)

## 8. Determinism

1. **Sequence Assignment**: Single source of truth (sequence generator service)
2. **Timestamp Source**: Exchange clock only (not client time)
3. **Event Content**: Derived from deterministic state transitions
4. **Replay**: Same events → same final state

## 9. Schema Evolution

**Backward Compatibility**: Required  
**Version Field**: Present in metadata  
**Deprecated Fields**: Never removed, marked deprecated  
**New Fields**: Add as optional

## 10. Invariants

1. **No Gaps**: sequence numbers continuous (1, 2, 3, ...)
2. **No Duplicates**: Each sequence appears exactly once
3. **Causal Order**: Child events after parent events
4. **Immutability**: Events never modified or deleted

## 11. Versioning

**Current Version**: v1.0.0  
**New Event Types**: Minor version bump  
**Schema Changes**: Major version bump if breaking
