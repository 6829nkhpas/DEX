# Launch Checklist

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This checklist ensures all technical, operational, and compliance requirements are met before the v1.0.0 exchange is exposed to public traffic.

## 2. Technical Readiness

- [ ] **Infrastructure Baseline**: Production VPC, Subnets, and Security Groups are deployed per `INFRA-BASELINE-v1.0.md`.
- [ ] **Data Persistence**: RDS/Aurora is running in Multi-AZ configuration with 0 Replication Lag.
- [ ] **Service Deployments**: Gateway, Order Service, Matching Engine, Settlement Service, and Risk Service are running the `v1.0.0` tagged image.
- [ ] **Load Testing**: The system successfully handled 100,000 orders/sec per symbol in the Staging environment for 1 hour without breaching the p99 latency SLA.
- [ ] **Chaos Engineering**: The DR drill was successfully executed, confirming an RTO of < 5 minutes.

## 3. Operations & Observability

- [ ] **Monitoring Stack**: Prometheus, Grafana, and Loki are actively ingesting metrics and logs from the production cluster.
- [ ] **Alerts Configured**: P0, P1, P2 alerts are verified to route correctly to PagerDuty and the `#incident-active` Slack channel.
- [ ] **On-Call Roster**: 24/7 on-call rotation is published and staffed.
- [ ] **Runbooks**: All operator manuals (`04-operator-manual.md`), rollback procedures, and incident response guides are published and reviewed by the team.

## 4. Compliance & Security

- [ ] **Smart Contract Audit**: External audit report is signed off with 0 Critical or High severity issues remaining.
- [ ] **Security Invariants**: Adherence to Spec 19 is continuously verified in the CI/CD pipeline.
- [ ] **Penetration Test**: External penetration test of the API Gateway completed and remediated.
- [ ] **Governance Hooks**: Multi-sig wallets for the SuperAdmin and RiskManager roles are generated, distributed, and tested.
- [ ] **KYC Integration**: The third-party identity verification provider is connected and testing successful.

## 5. Go-Live Sequence

The exact sequence of events for the Day 1 launch:

1. **T-Minus 24H**: Final "Go/No-Go" meeting with all stakeholders.
2. **T-Minus 12H**: Production environment scale-up (HPA pre-warming).
3. **T-Minus 2H**: Open deposits for listed assets.
4. **T-Minus 15M**: Engage DMMs (Designated Market Makers) in 'Post-Only' mode to seed the order book.
5. **T-Zero**: Publicly enable Order Creation and Matching via the API Gateway.
6. **T-Plus 1H**: Monitor closely for threshold stability in the `#launch-control` command center.
