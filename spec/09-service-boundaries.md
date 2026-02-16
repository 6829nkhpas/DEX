# Service Boundaries Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the service boundaries and responsibilities in the distributed exchange architecture, establishing clear separation of concerns.

## 2. Service Architecture

### 2.1 Core Services

```
┌─────────────────┐
│   API Gateway   │  ← Entry point
└────────┬────────┘
         │
    ┌────┴────┬──────┬────────┬─────────┬───────┐
    │         │      │        │         │       │
┌───▼──┐ ┌───▼───┐ ┌▼─────┐ ┌▼──────┐ ┌▼────┐ ┌▼────────┐
│Order │ │Account│ │Match │ │Settle │ │Risk │ │Liquidity│
│ Svc  │ │  Svc  │ │Engine│ │  Svc  │ │ Svc │ │   Svc   │
└──────┘ └───────┘ └──────┘ └───────┘ └─────┘ └─────────┘
```

## 3. Service Definitions

### 3.1 API Gateway

**責任**:
- HTTP/WebSocket request routing
- Authentication & authorization
- Rate limiting
- Request validation
- Response aggregation

**Does NOT**:
- Business logic
- State management
- Direct database access

**Interfaces**:
```
POST /v1/orders → OrderService.CreateOrder
GET /v1/orders/:id → OrderService.GetOrder
DELETE /v1/orders/:id → OrderService.CancelOrder
GET /v1/accounts/:id → AccountService.GetAccount
```

### 3.2 Order Service

**Responsibilities**:
- Order lifecycle management
- Order validation (schema, business rules)
- Order state persistence
- Order queries

**State Owned**:
- Order book (active orders)
- Order history
- Order state machine

**Interfaces**:
```
CreateOrder(request: OrderRequest) → Result<OrderID, Error>
CancelOrder(orderId: OrderID, accountId: AccountID) → Result<void, Error>
GetOrder(orderId: OrderID) → Result<Order, Error>
ListOrders(accountId: AccountID, filters: Filters) → Result<Order[], Error>
```

**Dependencies**:
- AccountService (balance check)
- RiskService (risk limits)
- MatchingEngine (order submission)

### 3.3 Matching Engine

**Responsibilities**:
- Order book maintenance
- Price-time priority matching
- Trade generation
- Market data calculation

**State Owned**:
- Live order book (in-memory)
- Matching algorithm state
- Recent trades (for market data)

**Interfaces**:
```
SubmitOrder(order: Order) → void
CancelOrder(orderId: OrderID) → void
GetOrderBook(symbol: Symbol, depth: u32) → OrderBook
GetMarketData(symbol: Symbol) → MarketData
```

**Performance Targets**:
- Matching latency: < 500μs (p99)
- Throughput: 100,000 orders/sec per symbol

**Note**: This is the HOT PATH (highest performance requirements)

### 3.4 Settlement Service

**Responsibilities**:
- Trade settlement
- Balance updates
- Position updates
- Fee collection

**State Owned**:
- Settlement journal
- Trade settlement status

**Interfaces**:
```
SettleTrade(trade: Trade) → Result<void, Error>
GetSettlementStatus(tradeId: TradeID) → SettlementStatus
```

**Dependencies**:
- AccountService (balance updates)
- EventStore (settlement events)

**Guarantees**:
- Idempotent settlement (trade_id as key)
- Atomic balance updates
- T+0 settlement

### 3.5 Account Service

**Responsibilities**:
- Account management
- Balance tracking
- Position management
- Collateral calculations

**State Owned**:
- Account balances
- Account positions
- Account metadata

**Interfaces**:
```
GetAccount(accountId: AccountID) → Result<Account, Error>
UpdateBalance(accountId: AccountID, asset: Asset, delta: Decimal) → Result<void, Error>
ReserveMargin(accountId: AccountID, amount: Decimal) → Result<void, Error>
ReleaseMargin(accountId: AccountID, amount: Decimal) → Result<void, Error>
GetPosition(positionId: PositionID) → Result<Position, Error>
UpdatePosition(positionId: PositionID, update: PositionUpdate) → Result<void, Error>
```

**Concurrency**:
- Optimistic locking (version field)
- Retry logic for conflicts

### 3.6 Risk Service

**Responsibilities**:
- Risk limit enforcement
- Margin calculations
- Liquidation monitoring
- Portfolio risk assessment

**State Owned**:
- Risk limits per account
- Margin requirements
- Liquidation queue

**Interfaces**:
```
CheckRiskLimits(accountId: AccountID, order: Order) → Result<void, RiskError>
CalculateMargin(position: Position) → Decimal
MonitorLiquidations() → void
GetMarginRatio(accountId: AccountID) → Decimal
TriggerLiquidation(accountId: AccountID) → Result<void, Error>
```

**Real-Time**: Continuously monitors all accounts

### 3.7 Liquidation Service

**Responsibilities**:
- Liquidation execution
- Position takeover
- Insurance fund management
- Auto-deleveraging

**State Owned**:
- Liquidations in progress
- Insurance fund balance
- ADL queue

**Interfaces**:
```
LiquidatePosition(positionId: PositionID) → Result<void, Error>
GetInsuranceFundBalance() → Decimal
GetADLQueue(symbol: Symbol) → Account[]
ExecuteADL(positionId: PositionID, counterpartyId: AccountID) → Result<void, Error>
```

### 3.8 Market Data Service

**Responsibilities**:
- Market data aggregation
- OHLCV calculation
- Mark price calculation
- Index price calculation

**State Owned**:
- Recent trades (for OHLCV)
- Mark price history
- Index prices from external sources

**Interfaces**:
```
GetOrderBook(symbol: Symbol, depth: u32) → OrderBook
GetRecentTrades(symbol: Symbol, limit: u32) → Trade[]
GetMarkPrice(symbol: Symbol) → Decimal
GetOHLCV(symbol: Symbol, interval: Interval) → OHLCV[]
```

**Update Frequency**: 
- Order book: Real-time
- Mark price: 1 second
- OHLCV: 1 second (for 1s candles)

### 3.9 Wallet Service

**Responsibilities**:
- Deposit detection
- Withdrawal processing
- Blockchain interaction
- Address generation

**State Owned**:
- Pending deposits
- Pending withdrawals
- Hot/cold wallet balances

**Interfaces**:
```
GenerateDepositAddress(accountId: AccountID, asset: Asset) → Address
ProcessWithdrawal(withdrawalRequest: WithdrawalRequest) → Result<TxID, Error>
GetDepositStatus(address: Address) → DepositStatus
```

## 4. Communication Patterns

### 4.1 Synchronous (gRPC)

**Use Cases**:
- Order placement (needs immediate response)
- Balance checks
- Risk validation

**Timeout**: 5 seconds max

### 4.2 Asynchronous (Events)

**Use Cases**:
- Trade settlement
- Market data updates
- Notifications

**Delivery**: At-least-once (with idempotency)

### 4.3 Request-Reply (RPC)

**Use Cases**:
- Queries (GetOrder, GetAccount)
- Commands requiring acknowledgment

## 5. Data Ownership

| Service | Owns | Read-Only Access |
|---------|------|------------------|
| Order Service | Orders | All |
| Matching Engine | Order Book | All (via API) |
| Account Service | Accounts, Balances, Positions | All |
| Settlement Service | Settlements | All |
| Risk Service | Risk Limits | All |

**Rule**: Only owner can WRITE, others READ via API calls

## 6. Failure Isolation

### 6.1 Circuit Breakers

Each service has circuit breaker for dependencies:
- Open: After 5 consecutive failures
- Half-Open: After 30 seconds
- Closed: After 3 successful calls

### 6.2 Degraded Mode

**Order Service**: Queue orders if matching engine down  
**Settlement Service**: Retry settlement indefinitely  
**Market Data**: Serve stale data if < 10 seconds old

## 7. Service Scaling

| Service | Scaling Strategy | Stateful |
|---------|------------------|----------|
| API Gateway | Horizontal (stateless) | No |
| Order Service | Horizontal + Sharding by symbol | Yes |
| Matching Engine | Vertical per symbol | Yes |
| Account Service | Horizontal + Sharding by account_id | Yes |
| Settlement Service | Horizontal (idempotent) | No |
| Risk Service | Horizontal (read replicas) | Yes |

## 8. Deployment Boundaries

### 8.1 Production

```
Region: us-east-1
  - API Gateway (3 instances)
  - Order Service (5 instances)
  - Matching Engine (10 instances, sharded by symbol)
  - Account Service (5 instances)
  - Settlement Service (3 instances)
  - Risk Service (2 instances)
```

### 8.2 Disaster Recovery

**RTO**: 5 minutes (Recovery Time Objective)  
**RPO**: 0 (Recovery Point Objective - no data loss)

**Strategy**: Active-passive multi-region

## 9. Interface Contracts

### 9.1 Protobuf Definitions

All gRPC services use protobuf:
```protobuf
service OrderService {
  rpc CreateOrder(OrderRequest) returns (OrderResponse);
  rpc CancelOrder(CancelRequest) returns (CancelResponse);
  rpc GetOrder(GetOrderRequest) returns (Order);
}
```

Location: `/spec/proto/`

### 9.2 Versioning

**URL Versioning**: `/v1/orders`, `/v2/orders`  
**Backward Compatibility**: Maintain v(n-1) for 6 months

## 10. Security Boundaries

**DMZ**: API Gateway only  
**Internal Network**: All other services  
**Database**: Private subnet, no internet access

**Authentication**: JWT tokens (verified at gateway)  
**Authorization**: Per-service ACLs

## 11. Monitoring

### 11.1 Health Checks

Each service exposes:
```
GET /health → { "status": "healthy" | "degraded" | "unhealthy" }
GET /metrics → Prometheus metrics
```

### 11.2 SLAs

| Service | Availability | Latency (p99) |
|---------|-------------|---------------|
| API Gateway | 99.99% | 100ms |
| Order Service | 99.99% | 50ms |
| Matching Engine | 99.99% | 500μs |
| Settlement Service | 99.9% | 10ms |
| Account Service | 99.99% | 20ms |

## 12. Versioning

**Current Version**: v1.0.0  
**Service Independence**: Services version independently  
**Breaking Changes**: New major version + deprecation period
