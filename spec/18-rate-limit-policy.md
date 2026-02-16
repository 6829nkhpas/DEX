# Rate Limit Policy Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines rate limiting policies for the distributed exchange to prevent abuse, ensure fair access, and maintain system stability.

## 2. Rate Limit Dimensions

### 2.1 Request Rate Limits

**Scope**: API requests per time window

### 2.2 Trading Volume Limits

**Scope**: Order/trade volume per time window

### 2.3 Concurrent Connection Limits

**Scope**: Simultaneous WebSocket connections

## 3. API Rate Limits

### 3.1 REST API Limits

| Endpoint Category | Limit | Window | Account Tier |
|------------------|-------|--------|--------------|
| Market Data (public) | 100 req/min | 1 min | All |
| Account Queries | 60 req/min | 1 min | Standard |
| Order Placement | 20 req/sec | 1 sec | Standard |
| Order Placement | 100 req/sec | 1 sec | VIP |
| Order Placement | 500 req/sec | 1 sec | Institutional |
| Order Cancellation | 50 req/sec | 1 sec | All |
| Withdrawals | 10 req/hour | 1 hour | All |

### 3.2 WebSocket Limits

| Connection Type | Max Connections | Max Subscriptions |
|----------------|-----------------|-------------------|
| Market Data | 10 per IP | 50 symbols |
| Private Streams | 5 per account | 20 channels |

### 3.3 Burst Allowance

**Token Bucket Algorithm**:
```
bucket_capacity = rate_limit × 2
refill_rate = rate_limit per second
```

**Example**:
```
Rate limit: 20 req/sec
Bucket: 40 tokens
User can burst 40 requests instantly
Then throttled to 20 req/sec
```

## 4. Order Rate Limits

### 4.1 Order Placement

| Tier | Orders/Second | Orders/Day |
|------|---------------|------------|
| Retail | 5 | 10,000 |
| Intermediate | 20 | 100,000 |
| Professional | 100 | 1,000,000 |
| Market Maker | 500 | Unlimited |

### 4.2 Order Cancellation

**Limit**: 2x order placement limit (allow fast cancel/replace)

**Cancel-All**: 1 request per 500ms (prevent spam)

### 4.3 Self-Trade Ratio

**Limit**: < 10% of total order volume

**Reason**: Detect wash trading

**Action**: If exceeded, flag account for review

## 5. Trading Volume Limits

### 5.1 Position Size Limits

| Tier | Max Position (USDT value) |
|------|--------------------------|
| Retail | $100,000 |
| Intermediate | $1,000,000 |
| Professional | $10,000,000 |
| Institutional | Custom |

### 5.2 Daily Volume Limits

**Purpose**: Anti-money laundering

| Tier | Daily Withdrawal | Daily Trading Volume |
|------|-----------------|---------------------|
| 1 | $10,000 | $100,000 |
| 2 | $100,000 | $1,000,000 |
| 3 | $1,000,000 | $10,000,000 |
| 4 | Unlimited | Unlimited |

## 6. Rate Limit Implementation

### 6.1 Algorithm: Token Bucket

**Pseudocode**:
```rust
struct RateLimiter {
    capacity: u32,
    tokens: f64,
    refill_rate: f64,
    last_update: Instant,
}

impl RateLimiter {
    fn allow_request(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        
        // Refill tokens
        self.tokens = f64::min(
            self.capacity as f64,
            self.tokens + elapsed * self.refill_rate
        );
        self.last_update = now;
        
        // Consume token
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false  // Rate limited
        }
    }
}
```

### 6.2 Storage

**In-Memory**: Redis with TTL

**Schema**:
```
Key: "ratelimit:{user_id}:{endpoint}"
Value: { tokens:40, last_update:1708123456 }
TTL: 3600 seconds
```

### 6.3 Distributed Rate Limiting

**Challenge**: Multiple API servers

**Solution**: Centralized Redis
```
1. Request arrives at API server A
2. Server A queries Redis for user's token count
3. If tokens available: decrement, allow request
4. If no tokens: reject with 429 status
```

**Accuracy**: ~95% (eventual consistency acceptable)

## 7. Rate Limit Responses

### 7.1 HTTP Header

```
X-RateLimit-Limit: 20
X-RateLimit-Remaining: 15
X-RateLimit-Reset: 1708123500
```

### 7.2 429 Response

```json
{
  "error": "RateLimitExceeded",
  "message": "Order placement rate limit exceeded",
  "limit": 20,
  "window": "1 second",
  "retry_after": 0.5
}
```

### 7.3 Retry-After Header

```
Retry-After: 1  (seconds)
```

## 8. Exemptions

### 8.1 Whitelisted IPs

**Use Case**: Institutional clients, market makers

**Process**:
1. Request whitelist
2. Compliance approval
3. Add IP to whitelist
4. Rate limits increased 10x

### 8.2 Emergency Override

**Authority**: 1 SuperAdmin

**Use Case**: System recovery, critical user issue

**Duration**: Max 1 hour

## 9. Adaptive Rate Limiting

### 9.1 System Load-Based

**Rule**: If system load > 80%, reduce all rate limits by 50%

**Implementation**:
```rust
fn effective_rate_limit(base_limit: u32, system_load: f64) -> u32 {
    if system_load > 0.8 {
        base_limit / 2
    } else {
        base_limit
    }
}
```

### 9.2 User Behavior-Based

**Good Actors**: Gradually increase limits (up to 2x)  
**Bad Actors**: Gradually decrease limits (down to 0.5x)

**Criteria**:
- Fill rate (orders that execute)
- Cancel rate (< 90% cancel rate is good)
- Self-trade ratio (< 5% is good)

## 10. DDoS Protection

### 10.1 IP-Based Limits

**Global Limit**: 1000 requests/minute per IP

**Motivation**: Prevent DDoS from single source

### 10.2 GeoBlocking

**Blocked Regions**: Configurable (e.g., sanctioned countries)

**Implementation**: Check IP geolocation, reject if in blocklist

### 10.3 Challenge-Response

**Trigger**: Suspicious traffic pattern

**Challenge**: CAPTCHA or proof-of-work

**Bypass**: Pass challenge → temporary whitelist (1 hour)

## 11. Monitoring

### 11.1 Metrics

- Rate limit hit rate (per endpoint, per user tier)
- Top rate-limited users
- Average tokens remaining (headroom)
- 429 error count

### 11.2 Alerts

| Condition | Action |
|-----------|--------|
| Individual user hits limit 100x/hour | Flag for review |
| Global 429 rate > 10% | Increase capacity or investigate |
| Sudden spike in traffic | DDoS alert |

## 12. Compliance

### 12.1 Fair Access

**Principle**: All users same tier get equal treatment

**Enforcement**: Rate limits applied uniformly (no hidden favoritism)

### 12.2 Transparency

**Public Documentation**: Rate limits published in API docs

**Updates**: 7-day notice for limit decreases

## 13. Grace Period

### 13.1 First Violation

**Action**: Warning (in response headers)

**Rate Limit**: Soft limit (allow but warn)

### 13.2 Repeated Violations

**Action**: Hard limit (reject requests)

**Escalation**: After 10 violations in 1 hour, temporary ban (1 hour)

## 14. Testing

### 14.1 Load Testing

**Frequency**: Monthly

**Scenario**: Simulate 10x normal load

**Validation**: Rate limits hold, system stable

### 14.2 User Feedback

**Channel**: Rate limit too restrictive? User can request increase

**Process**: Support ticket → compliance review → approval/denial

## 15. Versioning

**Current Version**: v1.0.0  
**Limit Changes**: 7-day notice for decreases, immediate for increases  
**New Tiers**: Announced with migration path
