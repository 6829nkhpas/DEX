# Market Data Specification

**Phase**: Launch & Operational Docs  
**Component**: Market Data Service

## 1. Overview

The Market Data Service aggregates internal matches and public states to provide read-optimized representations of the market. It disseminates this data primarily via WebSockets (for real-time updates) and REST (for historical queries).

## 2. Real-Time WebSockets

**WSS Endpoint**: `wss://stream.dex.example.com/v1`

### 2.1 Order Book Snapshot + Deltas
Clients subscribe to `book` to maintain a local L2 order book mirroring the Matching Engine.

**Subscription**:
```json
{
  "method": "SUBSCRIBE",
  "channels": ["book.BTC-USDT"]
}
```

**Message Stream**:
1. Initial full snapshot.
2. Subsequent incremental deltas (`update` type).

```json
{
  "type": "update",
  "symbol": "BTC-USDT",
  "sequence": 1234567,
  "bids": [ ["49900.00", "0.5"] ],
  "asks": [ ["50000.00", "0"] ] // Quantity 0 implies delete depth level
}
```

### 2.2 Trade Feed
Real-time feed of all executed trades (`TradeExecuted` events translated to public schemas).

```json
{
  "type": "trade",
  "symbol": "BTC-USDT",
  "trade_id": "uuid",
  "price": "50000.00",
  "quantity": "0.1",
  "side": "BUY",
  "timestamp": 1708123456789
}
```

## 3. Historical REST Endpoints

### 3.1 OHLCV Candlesticks
**GET** `/v1/market/klines?symbol=BTC-USDT&interval=1h&limit=100`

**Response**:
```json
[
  [
    1708123000000,   // Open time
    "50000.00",      // Open
    "51000.00",      // High
    "49000.00",      // Low
    "50500.00",      // Close
    "120.5"          // Volume
  ]
]
```

### 3.2 Index and Mark Prices
**GET** `/v1/market/mark-price?symbol=BTC-USDT`

Used by clients to determine liquidation proximity. Updates every 1 second.
