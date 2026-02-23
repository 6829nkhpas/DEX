# API Documentation

**Phase**: Launch & Operational Docs  
**Component**: Core Services (API Gateway)

## 1. Overview

The DEX provides a REST API via the API Gateway for external integrations, algorithmic trading, and UI clients. All requests must be authenticated, excluding public market data endpoints.

**Base URL**: `https://api.dex.example.com/v1`

## 2. Authentication

Requests must include a JWT in the Authorization header.
```
Authorization: Bearer <your_jwt_token>
```

## 3. Orders API

### 3.1 Create Order
**POST** `/orders`

Submits a new order to the matching engine. (Synchronous, low latency path).

**Request Body**:
```json
{
  "client_id": "req-12345", 
  "symbol": "BTC/USDT",
  "side": "BUY",
  "type": "LIMIT",
  "price": "50000.00",
  "quantity": "1.0",
  "time_in_force": "GTC"
}
```

**Response** (201 Created):
```json
{
  "order_id": "01939d7f-8e4a-7890-a123-456789abcdef",
  "status": "ACCEPTED"
}
```

### 3.2 Cancel Order
**DELETE** `/orders/{order_id}`

**Response** (200 OK):
```json
{
  "status": "CANCELED"
}
```

### 3.3 Get Active Orders
**GET** `/accounts/{account_id}/orders?status=PENDING`

Returns a list of all active orders for an account.

## 4. Account & Positions API

### 4.1 Get Account Balances
**GET** `/accounts/{account_id}/balances`

**Response**:
```json
{
  "account_id": "...",
  "balances": [
    { "asset": "USDT", "available": "10000.00", "locked": "500.00" },
    { "asset": "BTC", "available": "0.5", "locked": "0.0" }
  ]
}
```

### 4.2 Get Open Positions
**GET** `/accounts/{account_id}/positions`

Returns current unrealized PnL, margin requirements, and liquidation prices.

## 5. Rate Limits
All endpoints are strictly rate limited per IP and per Account.
- Orders: 100 req/sec
- Market Data: 1000 req/sec

If exceeded, the API returns `429 Too Many Requests`.
