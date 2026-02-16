# Governance Hooks Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines governance hooks that allow controlled admin intervention in the distributed exchange while maintaining auditability and safety.

## 2. Governance Principles

### 2.1 Core Tenets

1. **Minimize Intervention**: Automated systems preferred over manual actions
2. **Auditability**: All governance actions logged immutably
3. **Transparency**: Users notified of significant changes
4. **Reversibility**: Most actions reversible (except irreversible operations)
5. **Multi-Party Approval**: Critical actions require multiple approvers

## 3. Governance Roles

### 3.1 Role Hierarchy

| Role | Authority Level | Count |
|------|----------------|-------|
| SuperAdmin | Full system control | 3 |
| RiskManager | Risk parameters, liquidations | 5 |
| SupportAdmin| Account-level actions | 10 |
| Auditor | Read-only access | Unlimited |

### 3.2 Multi-Signature Requirements

| Action | Signatures Required |
|--------|-------------------|
| Update fee rates | 2 of 3 SuperAdmins |
| Halt trading | 2 of 3 SuperAdmins OR 1 RiskManager (emergency) |
| Adjust margin rates | 2 of 5 RiskManagers |
| Suspend account | 1 SupportAdmin |
| Force liquidate | 1 RiskManager |
| Modify smart contract | 3 of 3 SuperAdmins |

## 4. Governance Actions

### 4.1 Trading Controls

#### Halt Trading (Circuit Breaker)

**Trigger**: Manual or automated

**Conditions**:
```
- Market manipulation detected
- Technical issue discovered
- Regulatory requirement
- >10% of accounts in liquidation
```

**Effect**:
```
- Stop accepting new orders
- Existing orders remain in book
- Cancel-only mode
- Withdrawals still allowed
```

**Resumption**: Requires 2-of-3 SuperAdmin approval

#### Pause Symbol

**Purpose**: Disable specific trading pair

**Use Cases**:
- Chain fork/reorg
- Oracle price failure
- Low liquidity

**Effect**: Symbol marked as PAUSED, no new orders

#### Emergency Stop

**Purpose**: Immediate full system stop

**Trigger**: Critical security threat

**Effect**: ALL operations halt (except withdrawals)

### 4.2 Fee Adjustments

#### Update Fee Tiers

**Proposal**:
```json
{
  "action": "UpdateFeeTiers",
  "proposed_by": "admin_alice",
  "tiers": [
    { "volume": 0, "maker": 0.02, "taker": 0.10 },
    { "volume": 100000, "maker": 0.01, "taker": 0.08 }
  ],
  "effective_date": "2024-03-01T00:00:00Z"
}
```

**Approval**: 2-of-3 SuperAdmins

**Notice**: 30-day advance notice to users

#### Temporary Fee Waiver

**Use Case**: Promotional campaign

**Approval**: 1 SuperAdmin

**Duration**: Max 30 days

### 4.3 Risk Parameters

#### Adjust Leverage Limits

**Action**:
```
Symbol: BTC/USDT
Old max leverage: 125x
New max leverage: 100x
Reason: Increased volatility
```

**Approval**: 2-of-5 RiskManagers

**Grandfathering**: Existing positions keep old leverage

#### Update Maintenance Margin Rates

**Action**:
```
Old rate: 0.5%
New rate: 1.0%
Effective in: 24 hours
```

**Approval**: 2-of-5 RiskManagers

**Notice**: 24-hour warning

### 4.4 Account Actions

#### Suspend Account

**Reasons**:
- AML/KYC concerns
- Suspicious activity
- Legal requirement
- User request (security)

**Effect**:
```
- Block new orders
- Block withdrawals
- Allow order cancellation
- Existing positions remain
```

**Approval**: 1 SupportAdmin

**Appeal Process**: User can appeal via support

#### Force Close Position

**Reasons**:
- Regulatory requirement
- Position too large (risk to system)
- Emergency liquidation

**Approval**: 2-of-5 RiskManagers

**Compensation**: If not user fault, compensate slippage

#### Account Recovery

**Use Case**: User lost 2FA access

**Process**:
```
1. User submits recovery request
2. KYC reverification
3. Video call verification
4. 7-day waiting period
5. SupportAdmin approves recovery
6. 2FA reset, new credentials issued
```

### 4.5 System Parameters

#### Update Oracle Source

**Action**: Change price feed provider

**Example**:
```
Old: Binance + Coinbase + Kraken
New: Binance + Coinbase + Huobi
```

**Approval**: 3-of-3 SuperAdmins

**Testing**: Shadow mode for 7 days before switch

#### Adjust Risk Engine Parameters

**Parameters**:
- Position limits
- Order size limits
- Daily volume limits

**Approval**: 2-of-5 RiskManagers

### 4.6 Asset Management

#### Add New Asset

**Process**:
```
1. Technical review (smart contract audit)
2. Risk assessment
3. Liquidity analysis
4. Proposal to governance
5. 3-of-3 SuperAdmin approval
6. Announcement (7-day notice)
7. Asset goes live
```

#### Delist Asset

**Reasons**:
- Low volume
- Security concerns
- Regulatory requirement

**Process**:
```
1. Announcement (30-day notice)
2. Stop new deposits
3. Disable trading after 30 days
4. Allow withdrawals for 90 days
5. Force convert remaining balances to stablecoin
```

## 5. Proposal System

### 5.1 Proposal Lifecycle

```
PROPOSED → UNDER_REVIEW → APPROVED/REJECTED → SCHEDULED → EXECUTED
```

### 5.2 Proposal Schema

```json
{
  "proposal_id": "uuid",
  "type": "FeeUpdate",
  "proposed_by": "admin_alice",
  "proposed_at": "timestamp",
  "description": "Reduce maker fees to incentivize liquidity",
  "changes": { /* specific changes */ },
  "approvals": [
    { "approver": "admin_bob", "approved_at": "timestamp" },
    { "approver": "admin_charlie", "approved_at": "timestamp" }
  ],
  "status": "APPROVED",
  "scheduled_execution": "2024-03-01T00:00:00Z"
}
```

### 5.3 Approval Workflow

**Step 1**: Proposer creates proposal  
**Step 2**: Notify required approvers  
**Step 3**: Approvers review and sign  
**Step 4**: If threshold met → APPROVED  
**Step 5**: Schedule execution  
**Step 6**: Execute at scheduled time

## 6. Emergency Powers

### 6.1 Emergency Pause

**Authority**: Any single RiskManager

**Use Case**: Immediate threat detected

**Duration**: 1 hour max (auto-resume unless extended)

**Extension**: Requires 2-of-3 SuperAdmin approval

### 6.2 Emergency Liquidation

**Authority**: 2-of-5 RiskManagers

**Use Case**: Position threatens system solvency

**Process**: Force close position at market price

**Compensation**: None (user risk)

## 7. Audit Trail

### 7.1 Governance Event Log

**Schema**:
```json
{
  "event_id": "uuid",
  "event_type": "GovernanceAction",
  "action": "UpdateFeeRates",
  "actor": "admin_alice",
  "timestamp": 1708123456789000000,
  "parameters": { /* action details */ },
  "approvals": [ /* signatures */ ],
  "execution_result": "SUCCESS",
  "affected_users": 12345
}
```

**Storage**: Immutable append-only log

**Retention**: Permanent

### 7.2 Transparency

**Public API**:
```
GET /v1/governance/actions?since=<timestamp>
```

**Returns**: All non-sensitive governance actions

**Sensitive Actions**: Logged but not publicly exposed (account suspensions)

## 8. User Notifications

### 8.1 Notification Triggers

| Action | Notice Period | Channels |
|--------|--------------|----------|
| Fee change | 30 days | Email + In-app |
| New asset | 7 days | Email + Announcement |
| Asset delisting | 30 days | Email + In-app + SMS |
| Leverage change | 24 hours | Email + In-app |
| System maintenance | 24 hours | All channels |
| Emergency halt | Immediate | All channels |

### 8.2 Opt-Out

**Policy**: Users cannot opt out of critical notifications  
**Optional**: Marketing/promotional notifications

## 9. Override Restrictions

### 9.1 Forbidden Actions

**Never Allowed** (even with full governance approval):
- Modify past trades
- Retroactive fee changes
- Reverse settled transactions
- Seize customer funds (except legal requirement)
- Disable all withdrawals (except emergency)

### 9.2 Legal Overrides

**Regulatory Compliance**:
- Freeze account (legal order)
- Disclose user data (warrant)
- Block jurisdiction (sanctions)

**Process**: Legal team approval + 1 SuperAdmin

## 10. Decentralization Path

### 10.1 Future: DAO Governance (Optional)

**Phase 1**: Current (centralized governance)  
**Phase 2**: Hybrid (some params via token vote)  
**Phase 3**: Full DAO (all decisions via governance)

**Timeline**: TBD

## 11. Testing

### 11.1 Governance Drills

**Frequency**: Quarterly

**Scenarios**:
- Emergency halt activation
- Multi-sig approval workflow
- Asset delisting process

**Purpose**: Ensure readiness

## 12. Versioning

**Current Version**: v1.0.0  
**Role Changes**: Requires SuperAdmin consensus  
**Power Expansion**: Requires user notification + opt-in period
