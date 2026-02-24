# WebSocket Protocol Summary

**Spec Version**: 1.0.0
**Endpoint**: `wss://api.exchange.com/v1/ws`
**Auth**: JWT Bearer token passed as `?token=<jwt>` query parameter on connect.

---

## 1. Connection Lifecycle

### 1.1 Connect
```
Client → wss://api.exchange.com/v1/ws?token=<jwt>
Server → { "type": "connected", "session_id": "uuid" }
```

### 1.2 Heartbeat
- Server sends `{"type":"ping"}` every 15 seconds.
- Client must reply `{"type":"pong"}` within 5 seconds or be disconnected.

### 1.3 Disconnection & Reconnection (Exponential Backoff)
On unexpected disconnect, clients **MUST** implement exponential backoff:

| Attempt | Delay     |
|---------|-----------|
| 1       | 500ms     |
| 2       | 1s        |
| 3       | 2s        |
| 4       | 4s        |
| 5       | 8s        |
| 6+      | 16s (cap) |

**Jitter**: Add ±20% random jitter to each delay.

After reconnection:
1. Re-authenticate with fresh JWT.
2. Re-subscribe to all channels.
3. Request snapshot-since-seq to recover missed messages (see §5).

---

## 2. Channels

### `market_data`
- **Direction**: Server → Client
- **Content**: Order book updates, trades, mark price, ticker data.

### `account`
- **Direction**: Server → Client
- **Content**: Balance updates, order status changes, position updates.
- **Filtering**: Scoped to authenticated account.

### `trades`
- **Direction**: Server → Client
- **Content**: Real-time trade execution feed for a symbol.

---

## 3. Subscription Format

### Subscribe
```json
{
  "action": "subscribe",
  "channel": "market_data",
  "params": {
    "symbol": "BTC/USDT"
  }
}
```

### Server Acknowledgement
```json
{
  "type": "subscribed",
  "channel": "market_data",
  "params": { "symbol": "BTC/USDT" },
  "snapshot_seq": 1000
}
```

### Unsubscribe
```json
{
  "action": "unsubscribe",
  "channel": "market_data",
  "params": {
    "symbol": "BTC/USDT"
  }
}
```

### Server Acknowledgement
```json
{
  "type": "unsubscribed",
  "channel": "market_data",
  "params": { "symbol": "BTC/USDT" }
}
```

---

## 4. Snapshot / Delta Flow

On subscription, the server sends a **full snapshot** followed by incremental **deltas**:

### Snapshot Message
```json
{
  "type": "snapshot",
  "channel": "market_data",
  "sequence": 1000,
  "timestamp": "1708123456789000000",
  "payload": {
    "symbol": "BTC/USDT",
    "bids": [["50000.00", "1.5"], ["49999.00", "2.0"]],
    "asks": [["50001.00", "1.0"], ["50002.00", "0.5"]]
  }
}
```

### Delta Message
```json
{
  "type": "delta",
  "channel": "market_data",
  "sequence": 1001,
  "timestamp": "1708123456790000000",
  "payload": {
    "symbol": "BTC/USDT",
    "last_price": "50000.00",
    "volume_24h": "1234.56",
    "high_24h": "51000.00",
    "low_24h": "49000.00",
    "mark_price": "50010.00"
  }
}
```

**Client MUST** track `sequence` numbers. If a gap is detected, request a snapshot-since-seq (see §5).

---

## 5. Snapshot-Since-Seq (Gap Recovery)

If the client detects a sequence gap after reconnection:

### Request
```json
{
  "action": "snapshot_since",
  "channel": "market_data",
  "params": {
    "symbol": "BTC/USDT",
    "last_seq": 995
  }
}
```

### Response
Server replays all events from `last_seq + 1` to current, then switches to live deltas:
```json
{
  "type": "snapshot_since_response",
  "channel": "market_data",
  "from_seq": 996,
  "to_seq": 1005,
  "events": [
    { "sequence": 996, "timestamp": "1708123456780000000", "payload": { "..." : "..." } },
    { "sequence": 997, "timestamp": "1708123456781000000", "payload": { "..." : "..." } }
  ]
}
```

---

## 6. Rate Limits

| Resource              | Limit                |
|-----------------------|----------------------|
| Connections per account | 10                 |
| Subscriptions per conn  | 50                 |
| Messages per second     | 100 (client→server)|

Exceeding limits results in:
```json
{
  "type": "error",
  "code": "RATE_LIMIT_EXCEEDED",
  "message": "Too many messages per second"
}
```

---

## 7. Error Messages

```json
{
  "type": "error",
  "code": "INVALID_CHANNEL",
  "message": "Channel 'foo' does not exist"
}
```

Error codes: `RATE_LIMIT_EXCEEDED`, `INVALID_CHANNEL`, `AUTH_FAILED`, `INVALID_ACTION`, `SEQ_TOO_OLD`.
