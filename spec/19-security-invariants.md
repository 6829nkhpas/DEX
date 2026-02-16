# Security Invariants Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the security invariants that MUST hold at all times in the distributed exchange. Violation of these invariants constitutes a critical security breach.

## 2. Financial Invariants

### 2.1 Balance Conservation

**Invariant**:
```
Σ(all_user_balances) + Σ(fees_collected) = Σ(blockchain_deposits) - Σ(blockchain_withdrawals)
```

**Meaning**: Total system balances must equal blockchain movements

**Validation**: Hourly automated check

**Violation Response**: Immediate trading halt + investigation

### 2.2 No Negative Balances

**Invariant**:
```
∀ accounts: balance.available >= 0 AND balance.locked >= 0
```

**Meaning**: No user can have negative balance

**Enforcement**: Database check constraints

**Violation**: Impossible (constraint prevents it)

### 2.3 Margin Sufficiency

**Invariant**:
```
∀ open_positions: account.equity >= position.maintenance_margin
```

**Meaning**: All positions adequately margined (or in liquidation)

**Enforcement**: Real-time monitoring (every mark price update)

**Violation Response**: Trigger liquidation

### 2.4 Order Funding

**Invariant**:
```
∀ open_orders: account.available >= order.margin_requirement
```

**Meaning**: Every order has locked margin

**Enforcement**: Checked at order placement

**Violation**: Reject order (should never happen post-acceptance)

### 2.5 Trade Settlement Completeness

**Invariant**:
```
∀ trades: trade.state = SETTLED IMPLIES (
  maker_balance_updated AND taker_balance_updated
)
```

**Meaning**: Settled trades always update both parties

**Enforcement**: Atomic database transactions

**Violation**: Rollback transaction

## 3. Authentication & Authorization Invariants

### 3.1 Authentication Required

**Invariant**:
```
∀ protected_endpoints: request.has_valid_jwt_token()
```

**Meaning**: All protected endpoints require authentication

**Enforcement**: API gateway middleware

**Violation**: 401 Unauthorized

### 3.2 Authorization Scoped

**Invariant**:
```
∀ account_operations: request.user_id = account.owner_id
```

**Meaning**: Users can only access their own accounts

**Enforcement**: Authorization middleware

**Violation**: 403 Forbidden

### 3.3 Session Validity

**Invariant**:
```
∀ sessions: session.expires_at > now() AND session.not_revoked
```

**Meaning**: Only valid, non-revoked sessions accepted

**Enforcement**: Session validation middleware

**Violation**: 401 Unauthorized (re-login required)

### 3.4 Admin Audit Trail

**Invariant**:
```
∀ admin_actions: action logged in immutable audit log
```

**Meaning**: All admin actions auditable

**Enforcement**: Audit logger wrapper

**Violation**: Action rejected (logging failure = security issue)

## 4. Data Integrity Invariants

### 4.1 Order State Consistency

**Invariant**:
```
∀ orders: order.filled + order.remaining = order.total
```

**Meaning**: Order quantities add up

**Enforcement**: Runtime validation after each update

**Violation**: PANIC (data corruption)

### 4.2 Trade-Order Link

**Invariant**:
```
∀ trades: 
  trade.maker_order EXISTS AND 
  trade.taker_order EXISTS AND
  trade.quantity <= min(maker.remaining, taker.remaining)
```

**Meaning**: Trades reference valid orders with sufficient quantity

**Enforcement**: Foreign key constraints + validation

**Violation**: Reject trade creation

### 4.3 Event Sequence Continuity

**Invariant**:
```
∀ events: event[i].sequence = event[i-1].sequence + 1
```

**Meaning**: No gaps in event sequence

**Enforcement**: Sequence generator guarantees

**Violation**: CRITICAL (event loss)

### 4.4 Timestamp Monotonicity

**Invariant**:
```
∀ events: event[i].timestamp > event[i-1].timestamp
```

**Meaning**: Time never goes backwards

**Enforcement**: Monotonic clock service

**Violation**: CRITICAL (clock issue)

## 5. Operational Invariants

### 5.1 Service Health

**Invariant**:
```
∀ critical_services: service.health_check() = HEALTHY OR degraded_mode_active
```

**Meaning**: Critical services operational or system in degraded mode

**Enforcement**: Health check monitoring

**Violation**: Automatic failover or circuit breaker

### 5.2 Database Replication Lag

**Invariant**:
```
∀ replicas: replica.lag < 10 seconds
```

**Meaning**: Replicas reasonably up-to-date

**Enforcement**: Monitoring

**Violation**: Remove replica from pool (serve from others)

### 5.3 Event Processing Lag

**Invariant**:
```
last_processed_event.sequence >= global_sequence - 1000
```

**Meaning**: Event processors keeping up (within 1000 events)

**Enforcement**: Monitoring

**Violation**: Alert on-call (investigate backlog)

## 6. Cryptographic Invariants

### 6.1 Password Hashing

**Invariant**:
```
∀ user_passwords: stored_password = bcrypt(password, cost=12)
```

**Meaning**: Never store plaintext passwords

**Enforcement**: Password service

**Violation**: Impossible (would require code change)

### 6.2 API Key Secrecy

**Invariant**:
```
∀ api_keys: key stored as hash, only shown once at creation
```

**Meaning**: API keys hashed, not retrievable

**Enforcement**: API key service

**Violation**: Reject (plaintext storage forbidden)

### 6.3 TLS Everywhere

**Invariant**:
```
∀ external_connections: connection.is_tls = true
```

**Meaning**: All external communication encrypted

**Enforcement**: Server configuration

**Violation**: Connection refused

### 6.4 JWT Signature Validation

**Invariant**:
```
∀ jwt_tokens: verify_signature(token, public_key) = true
```

**Meaning**: All JWTs cryptographically verified

**Enforcement**: JWT middleware

**Violation**: 401 Unauthorized

## 7. Rate Limiting Invariants

### 7.1 Per-User Limits

**Invariant**:
```
∀ users: user.requests_in_window() <= user.rate_limit
```

**Meaning**: Rate limits enforced

**Enforcement**: Rate limiter middleware

**Violation**: 429 Too Many Requests

### 7.2 Global Throughput

**Invariant**:
```
system.requests_per_second < max_capacity × 0.8
```

**Meaning**: System not overloaded (20% headroom)

**Enforcement**: Load balancer + monitoring

**Violation**: Circuit breaker (reject excess traffic)

## 8. Compliance Invariants

### 8.1 KYC Enforcement

**Invariant**:
```
∀ withdrawals > $10k: user.kyc_verified = true
```

**Meaning**: Large withdrawals require KYC

**Enforcement**: Withdrawal service

**Violation**: Withdrawal rejected

### 8.2 Sanction Screening

**Invariant**:
```
∀ users: user.address NOT IN sanctions_list
```

**Meaning**: No sanctioned entities

**Enforcement**: Onboarding + periodic checks

**Violation**: Account frozen

### 8.3 Tax Reporting

**Invariant**:
```
∀ transactions > threshold: transaction logged for tax reporting
```

**Meaning**: Tax-reportable events tracked

**Enforcement**: Transaction logger

**Violation**: Compliance violation

## 9. Testing Invariants

### 9.1 Test Coverage

**Invariant**:
```
code_coverage >= 80% (critical paths: 95%)
```

**Meaning**: Adequate test coverage

**Enforcement**: CI/CD gates

**Violation**: Build fails (deploy blocked)

### 9.2 Integration Tests

**Invariant**:
```
∀ deploys: all_integration_tests PASSED
```

**Meaning**: No deployment with failing tests

**Enforcement**: CI/CD pipeline

**Violation**: Deploy blocked

## 10. Monitoring & Alerting

### 10.1 Alert Response

**Invariant**:
```
∀ P0_alerts: acknowledged_within(5 minutes)
```

**Meaning**: Critical alerts acknowledged quickly

**Enforcement**: On-call rotation

**Violation**: Escalation to next tier

### 10.2 Metric Collection

**Invariant**:
```
∀ services: metrics.last_update < 60 seconds ago
```

**Meaning**: Metrics actively collected

**Enforcement**: Monitoring system

**Violation**: Alert (monitoring outage)

## 11. Validation Procedures

### 11.1 Continuous Validation

**Process**:
```
Every minute:
  Check financial invariants
  Check data integrity invariants
  Log results
  If violation: ALERT + halt relevant operations
```

### 11.2 Daily Reconciliation

**Process**:
```
Daily at 00:00 UTC:
  Full balance reconciliation
  Blockchain vs database
  Detect any discrepancies
  Report to compliance team
```

### 11.3 Weekly Audit

**Process**:
```
Weekly:
  Replay events from past week
  Verify final state matches current state
  Check for determinism violations
  Report anomalies
```

## 12. Violation Handling

### 12.1 Severity Levels

| Level | Description | Action |
|-------|-------------|--------|
| P0 - Critical | Financial invariant violation | Immediate halt + page on-call |
| P1 - High | Security invariant violation | Alert + restrict affected operations |
| P2 - Medium | Operational invariant violation | Alert + monitor |
| P3 - Low | Warning condition | Log + review |

### 12.2 Incident Response

**Steps**:
```
1. Detect violation (automated)
2. Halt affected operations
3. Page on-call engineer
4. Investigate root cause
5. Implement fix
6. Verify invariants restored
7. Resume operations
8. Post-mortem report
```

## 13. Invariant Evolution

### 13.1 Adding New Invariants

**Process**:
1. Propose new invariant
2. Review with security team
3. Implement validation
4. Deploy monitoring
5. Document in this spec

### 13.2 Relaxing Invariants

**Requirements**:
- Security review
- Executive approval
- User notification (if affects customers)
- 30-day notice period

## 14. Versioning

**Current Version**: v1.0.0  
**New Invariants**: Minor version bump  
**Removing Invariants**: Major version bump (breaking change)  
**Tightening Invariants**: Minor version bump (always safe)
