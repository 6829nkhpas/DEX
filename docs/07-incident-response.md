# Incident Response

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This document outlines the standard operating procedure for handling severity incidents (P0-P3) in the distributed exchange, aligned with the Failure Recovery Philosophy (Spec 10) and Security Invariants (Spec 19).

## 2. Severity Levels and SLAs

| Severity | Definition | Target Response (Ack) | Target Resolution |
|----------|------------|-----------------------|-------------------|
| **P0** (Critical) | Financial invariant broken, ME crash, total outage. | < 5 minutes | < 1 hour |
| **P1** (High) | Major latency spike (>10x p99), single service down. | < 15 minutes | < 4 hours |
| **P2** (Medium) | Degraded performance, non-critical bugs. | < 4 hours | < 24 hours |
| **P3** (Low) | Cosmic UI glitches, minor monitoring anomalies. | < 24 hours | Next sprint |

## 3. Incident Lifecycle

### 3.1 🚨 Phase 1: Detection & Triage
1. **Detection**: Alertmanager fires to PagerDuty/Slack.
2. **Acknowledge**: On-call engineer claims the incident.
3. **Assessment**: Evaluate if the issue breaches Security Invariants (Section 2 of Spec 19).
   - If **Financial Invariants** are violated (e.g., balance conservation fails): **IMMEDIATELY HALT TRADING**.

### 3.2 🛠️ Phase 2: Containment & Mitigation
1. **Assign Roles**: Incident Commander (IC), Operations Lead, Communications Lead.
2. **Containment Actions**:
   - *Runaway matching bug*: Disable Order Service ingestion.
   - *Database corruption*: Failover to read replica.
   - *Exploit suspected*: Engage Maintenance Mode and revoke active sessions.

### 3.3 🩹 Phase 3: Resolution
- Implement the hotfix, rollback, or failover (refer to `06-rollback-procedure.md` or `08-disaster-recovery.md`).
- Validate system metrics return to baseline.

### 3.4 📝 Phase 4: Post-Mortem
Must be completed within 48 hours for any P0/P1:
- Blameless analysis of root cause.
- Define timeline of events.
- Issue Action Items (Jira tickets) for prevention.

## 4. Communication Protocol

- **Internal**: Use dedicated `#incident-active` Slack channel. IC calls the shots.
- **External**: Status Page updates required every 30 minutes during P0/P1 incidents. Never expose internal stack traces or exact vulnerability mechanics publicly during triage.
