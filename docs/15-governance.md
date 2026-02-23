# Governance Document

**Phase**: Launch & Operational Docs  
**Component**: Compliance & Operations

## 1. Governance Overview

The DEX employs a centralized but transparent governance model (v1.0.0) with strict multi-signature requirements for critical operational and financial parameter changes. This ensures rapid response capability while preventing unilateral malicious actions.

## 2. Role Hierarchy & Authority

| Role | Total Members | Responsibilities |
|------|---------------|------------------|
| **SuperAdmin** | 3 | Full system control, Smart Contract upgrades. |
| **RiskManager** | 5 | Margin configurations, forced liquidations, leverage limits. |
| **SupportAdmin** | 10 | KYC reviews, account suspensions, user recovery. |

## 3. Executive Actions & Multi-Sig Requirements

All governance actions are logged in the immutable Governance Event Log. No single individual can execute a critical financial parameter change.

### 3.1 SuperAdmin Actions
- **Update Fee Tiers**: Requires 2 of 3 SuperAdmins. (30-day user notice required).
- **Halt Trading (Emergency)**: Requires 2 of 3 SuperAdmins OR 1 RiskManager (temporary 1-hr halt).
- **Modify Oracle Source**: Requires 3 of 3 SuperAdmins (7-day shadow period required).

### 3.2 RiskManager Actions
- **Adjust Leverage Limits**: Requires 2 of 5 RiskManagers.
- **Force Close/Liquidate Position**: Requires 2 of 5 RiskManagers (used only if position threatens system solvency).

### 3.3 SupportAdmin Actions
- **Suspend Account**: Requires 1 SupportAdmin (Legal or AML reasons).
- **Asset Delisting**: Requires 30-day notice, halting new deposits.

## 4. Emergency Governance Powers
In the event of a catastrophic exploit or data corruption, a single RiskManager may trigger an **Emergency Pause** lasting exactly 1 hour. This drops the API Gateway into a maintenance mode where only query and withdrawal operations are permitted. Extensions require SuperAdmin quorum.

## 5. Auditability
All actions executed by any Admin role are broadcast onto the `/v1/governance/actions` public endpoint to ensure transparency (excluding actions citing specific PII/Account Suspensions for legal reasons).
