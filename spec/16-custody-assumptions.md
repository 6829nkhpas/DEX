# Custody Assumptions Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the custody model for the distributed exchange, establishing who controls assets and how they are secured.

## 2. Custody Model: Non-Custodial with Hot/Cold Wallet

### 2.1 Model Type

**Classification**: Custodial (exchange controls private keys)

**Rationale**:
- Enables instant trading (no blockchain confirmations)
- Supports margin/leverage trading
- Provides liquidity for market making

**Alternative**: Non-custodial DEX (users keep keys, slower trading)

## 3. Wallet Architecture

### 3.1 Hot Wallet

**Purpose**: Facilitate withdrawals and trading operations  
**Holdings**: 5-10% of total assets  
**Security**: Multi-signature (3-of-5)  
**Location**: Online servers

**Access Control**:
- Automated withdrawals: < $10,000
- Manual approval: $10,000 - $100,000
- Multi-party approval: > $100,000

### 3.2 Cold Wallet

**Purpose**: Secure long-term storage  
**Holdings**: 90-95% of total assets  
**Security**: Air-gapped, multi-signature (5-of-7)  
**Location**: Hardware security modules (HSMs) + offline storage

**Access**: Manual process, requires physical presence

### 3.3 Warm Wallet

**Purpose**: Intermediate storage for faster cold→hot transfers  
**Holdings**: 0-5% during rebalancing  
**Security**: Online but with additional approval layers

## 4. Asset Segregation

### 4.1 Customer vs Exchange Assets

**Principle**: Customer funds NEVER commingled with exchange funds

**Implementation**:
```
Customer wallets: Address pool 1-1,000,000
Exchange treasury: Address pool 2,000,001-2,001,000
Fee collection: Address pool 3,000,001-3,001,000
Insurance fund: Address pool 4,000,001-4,001,000
```

**Audit**: Daily proof-of-reserves matching user balances

### 4.2 Per-User Accounting

**Database**:
```sql
CREATE TABLE balances (
  account_id UUID,
  asset VARCHAR(10),
  available DECIMAL(36,18),
  locked DECIMAL(36,18)
);
```

**Blockchain**: Pooled wallet (not per-user addresses)

**Reconciliation**: DB balance ≤ total wallet balance

## 5. Deposit Flow

### 5.1 Process

```
1. User requests deposit address → generates unique address
2. User sends crypto to address
3. Exchange monitors blockchain for incoming tx
4. After N confirmations: Credit user account
5. Funds remain in hot wallet or swept to cold wallet
```

### 5.2 Confirmation Requirements

| Asset | Confirmations | Time |
|-------|--------------|------|
| BTC | 3 | ~30 min |
| ETH | 12 | ~3 min |
| USDT (TRC20) | 19 | ~1 min |
| USDT (ERC20) | 12 | ~3 min |

**Rationale**: Balance security vs user convenience

### 5.3 Deposit Limits

**Per Transaction**: Unlimited  
**Daily**: $1M (KYC tier 1), $10M (KYC tier 2), Unlimited (KYC tier 3)

## 6. Withdrawal Flow

### 6.1 Process

```
1. User submits withdrawal request
2. System validates:
   - Sufficient available balance
   - KYC/AML checks
   - Address whitelist (if enabled)
3. Deduct from user balance (instant)
4. Queue withdrawal transaction
5. Sign transaction (multi-sig)
6. Broadcast to blockchain
7. Monitor confirmation
8. Mark as complete
```

### 6.2 Withdrawal Limits

| Tier | Daily Limit | Approval |
|------|------------|----------|
| 1 | $10,000 | Automated |
| 2 | $100,000 | Automated |
| 3 | $1,000,000 | Automated |
| > $1M | Custom | Manual review |

### 6.3 Withdrawal Delay

**Security Feature**: 24-hour delay for first withdrawal to new address

**Purpose**: Prevent account takeover withdrawals

**Override**: User can disable (reduces security)

## 7. Private Key Management

### 7.1 Key Generation

**Process**:
- Generate on air-gapped machine
- Use hardware RNG (Random Number Generator)
- BIP39 mnemonic backup
- Split into shards (Shamir's Secret Sharing)

**Storage**: Each shard in separate geographic location

### 7.2 Key Usage

**Hot Wallet**:
- Private keys in HSM
- API access with rate limiting
- Auto-signing for withdrawals < threshold

**Cold Wallet**:
- Private keys offline
- Require physical ceremony to sign
- 5-of-7 signatories needed

### 7.3 Key Rotation

**Frequency**: Annually (or after staff changes)

**Process**:
```
1. Generate new key pair
2. Transfer assets to new address
3. Destroy old private keys
4. Update address mapping
```

## 8. Proof of Reserves

### 8.1 Merkle Tree Proof

**Schedule**: Weekly

**Process**:
```
1. Snapshot all user balances
2. Build Merkle tree
3. Publish root hash + total assets
4. Users verify their balance inclusion
```

**Transparency**: On-chain commitment to reserves

### 8.2 Third-Party Audit

**Frequency**: Quarterly

**Auditor**: Independent accounting firm

**Scope**:
- Verify blockchain holdings
- Match against user balances
- Confirm: assets ≥ liabilities

## 9. Insurance Coverage

### 9.1 Insurance Fund

**Source**: 20% of trading fees

**Purpose**: Cover losses from:
- Hacks
- Technical failures
- Liquidation shortfalls

**Target Size**: 10% of total assets under custody

### 9.2 External Insurance

**Policy**: $100M coverage

**Provider**: Crypto-specialized insurer

**Coverage**: Hot wallet compromise

## 10. Operational Security

### 10.1 Access Control

**Principle**: Least privilege

**Roles**:
- Engineers: Read-only access to wallets
- Ops team: Hot wallet signing (with limits)
- Executives: Cold wallet signing (multi-party)
- Auditors: Read-only blockchain verification

### 10.2 Multi-Signature Schemes

**Hot Wallet**: 3-of-5 multi-sig  
**Cold Wallet**: 5-of-7 multi-sig  
**Recovery Wallet**: 7-of-9 multi-sig (backup)

**Geographic Distribution**: Signers in 3+ countries

### 10.3 Monitoring

**Real-Time Alerts**:
- Unauthorized withdrawal attempt
- Balance discrepancy (DB vs blockchain)
- Large outflow (> $1M/hour)
- Multi-sig threshold change

## 11. Disaster Recovery

### 11.1 Key Loss

**Scenario**: Cold wallet keys lost

**Recovery**:
```
1. Use recovery wallet (7-of-9 multi-sig)
2. Transfer assets to new cold wallet
3. Decommission lost keys
```

### 11.2 Database Loss

**Scenario**: User balance database corrupted

**Recovery**:
```
1. Restore from daily backup
2. Replay transaction log to current time
3. Reconcile with blockchain withdrawals/deposits
```

**RPO**: 1 hour (hourly backups)

### 11.3 Hack/Compromise

**Immediate Actions**:
1. Pause all withdrawals
2. Transfer hot wallet to new address
3. Investigate breach
4. Notify users
5. File insurance claim (if applicable)

## 12. Regulatory Compliance

### 12.1 AML/KYC

**Deposit**: No KYC required (receive-only)  
**Trading**: Basic KYC (ID verification)  
**Withdrawal**: Full KYC + source of funds

**Threshold**: $10,000 lifetime withdrawals

### 12.2 Reporting

**FinCEN (US)**: Suspicious activity reports (SARs)  
**Local Regulators**: Transaction reporting > $10k

### 12.3 Jurisdiction

**Assets Domiciled**: Multiple jurisdictions  
**Legal Structure**: Trust structure for customer assets

## 13. Invariants

1. **Solvency**: Total blockchain holdings ≥ sum of user balances
2. **Segregation**: Customer funds separate from exchange treasury
3. **Auditability**: All transactions traceable on-chain
4. **Recoverability**: Private keys recoverable via multi-party ceremony

## 14. Versioning

**Current Version**: v1.0.0  
**Custody Model Change**: Major version + user notification + migration period  
**Security Policy Updates**: Immediate (priority: security)
