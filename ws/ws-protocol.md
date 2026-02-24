# WebSocket Protocol Summary

## 1. Overview
The exchange provides real-time updates via WebSocket over `wss://api.exchange.com/v1/ws`.

## 2. Channels

### `market_data`
- **Direction**: Server -> Client
- **Description**: Real-time market data updates (order book, trades, mark price).
- **Subscribe Flow**: Client sends `subscribe:market_data` message.
- **Messages**:
  - `MarketDataUpdated`
  - `MarkPriceUpdated`

## 3. Connection Rules
- **Rate Limit**: 10 connections per account ID, 50 subscriptions.
- **Reconnection**: Clients should implement exponential backoff on disconnect.
- **Snapshot/Delta Flow**: Server sends a full snapshot upon initial subscription, followed by delta updates to the order book.

## 4. Example Messages

### Client Subscription Example
```json
{
  "action": "subscribe",
  "channel": "market_data",
  "symbol": "BTC/USDT"
}
```

### Server MarketDataUpdated Example (Delta)
```json
{
  "event_type": "MarketDataUpdated",
  "sequence": 1001,
  "timestamp": 1708123456789000000,
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
