# Security Checklist

**Phase**: Launch & Operational Docs  
**Component**: Operations / Security

## 1. Overview

This checklist is defined directly from Spec 19: Security Invariants. It provides the daily/weekly operational checks and deployment gates required to prove the exchange is secure.

## 2. Pre-Deployment Validation

All deployments (represented by PRs and K8s manifests) MUST pass these checks:

- [ ] **Test Coverage Gate**: Global code coverage > 80%, critical paths (matching, settlement, risk) > 95%.
- [ ] **Integration Suites**: Chaos tests, determinism replays, and network partition simulations must pass in CI.
- [ ] **Network Policies**: Review any changes to `infra/k8s/network-policy.yml`. Ensure the internal network prevents direct ingress access.
- [ ] **Non-root Containers**: Ensure all Dockerfiles run under an unprivileged user.
- [ ] **Dependencies**: `cargo audit` reports no known vulnerabilities.

## 3. Operational Integrity Audits

### 3.1 Daily Reviews
- [ ] **Balance Conservation**: The automated script confirms that `Σ(balances) + Σ(fees) = Σ(deposits) - Σ(withdrawals)`.
- [ ] **Authentication Middleware**: Confirm no `/v1/orders` endpoints are bypassable.
- [ ] **Admin Actions**: Review the immutable audit log for suspicious internal tooling actions or configuration overrides.
- [ ] **Data Sequences**: Confirm there are no gaps or jumps in the global event sequences indicating missed messages.
- [ ] **Account Margin**: Real-time risk engine reports 0 accounts violating maintenance margin (or actively being liquidated).

### 3.2 Weekly Reviews
- [ ] **State Determinism Test**: A scheduled job has pulled 1 hr of historic journal events, replayed them on a sandbox ME, and confirmed the output state hash matches production perfectly.
- [ ] **Sanction DB SYNC**: Confirm that the OFAC/Sanctions list synced within the last 7 days and blocked accounts remain frozen.
- [ ] **Log Access**: Verify that production logs (Loki) do NOT contain PII, API Keys in plaintext, or JWT payloads beyond the UUID.

### 3.3 Access & Key Management
- [ ] **API Keys**: Stored ONLY as bcrypt hashes. Never logged.
- [ ] **TLS Settings**: Ensure ingress handles TLS termination exclusively with modern cipher suites (e.g., TLS 1.3 only or 1.2 strict).
- [ ] **Passwords/Keys**: Validate Kubernetes Sealed Secrets are rotating properly.

## 4. Emergency Action Procedure

If ANY of the Daily Review checks fail unexpectedly (specifically Balance Conservation):

1. Escalate to P0 Incident response.
2. Freeze Withdrawals (Kill switch via Admin CLI).
3. Halt matching and engage Maintenance Mode.
4. Notify the security audit team immediately.
