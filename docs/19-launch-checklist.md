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

## 6. Web UI — Auth & Wallet Readiness

These checks validate the Web UI auth/wallet layer before public traffic is enabled.

### 6.1 Wallet Connect Flow

- [ ] **Connect Wallet**: "Connect Wallet" button in header triggers MetaMask/EIP-1193 prompt.
- [ ] **No Wallet**: Error shown if no EIP-1193 provider is detected (not a crash).
- [ ] **Connecting State**: Header badge shows pulsing indicator during connection.

### 6.2 Sign-In Flow

- [ ] **Sign In**: Connected wallet shows "Sign In" button; clicking triggers `personal_sign` prompt.
- [ ] **Signing State**: Header badge shows "Awaiting signature…" during signing.
- [ ] **Success**: On signature, status transitions to `authenticated`; "Sign Out" button appears.
- [ ] **Rejection**: If user rejects, status transitions to `rejected`; "Retry" button appears.

### 6.3 Session Restore

- [ ] **Refresh with valid session**: After page refresh, valid session is restored from `sessionStorage`; status returns to `authenticated` without re-signing.
- [ ] **Refresh with expired session**: Expired session cleared; status is `expired`; "Session expired — Sign In" shown.
- [ ] **Refresh after disconnect**: No session; status is `disconnected`.

### 6.4 Logout

- [ ] **Sign Out**: Clicking "Sign Out" clears session, status transitions to `connected`.
- [ ] **Idempotent**: Clicking "Sign Out" twice is safe.

### 6.5 Protected Action Gating

- [ ] **Order Entry**: "Sign in to place orders" shown when not authenticated; form hidden.
- [ ] **Open Orders**: "Sign in to view open orders" gate shown when not authenticated.
- [ ] **Positions**: "Sign in to view positions" gate shown when not authenticated.
- [ ] **Account Panel**: "Sign in to view account balances" gate shown when not authenticated.
- [ ] **Cancel Button**: Rendered as non-interactive span when not authenticated.

### 6.6 Invalidation

- [ ] **Chain Change**: Switching network in MetaMask clears auth; status returns to `connected`.
- [ ] **Account Change**: Switching account in MetaMask clears auth; status returns to `connected`.
- [ ] **Session Expiry Timer**: Proactive 60s timer — session transitions to `expired` without page reload.

### 6.7 Public Market Access

- [ ] **Orderbook**: Visible to all users regardless of auth state.
- [ ] **Trade Tape**: Visible to all users regardless of auth state.
- [ ] **Ticker**: Visible to all users regardless of auth state.
- [ ] **ConnectionBanner**: Shown when WebSocket is `disconnected` or `connecting`; hidden when connected.

### 6.8 Tests

- [ ] `npm test` from `apps/web-ui` — all 3 test suites pass (auth-session, wallet-account, launch-readiness).
- [ ] `npm run typecheck` from `apps/web-ui` — 0 TypeScript errors.
