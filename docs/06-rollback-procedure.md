# Rollback Procedure

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This document defines the process for reverting an upgrade that has introduced instability or failed to meet performance/consistency targets in production. Following a deployment, a rollback mitigates unexpected impact within the 5-minute RTO.

## 2. Triggering a Rollback

A rollback is mandatory if any of the following conditions are met within the deployment validation window (typically 1 hour post-deployment):
1. **P0 Incident Triggered**: e.g., Matching irregularities, loss of State Determinism.
2. **SLA Violations**: Latency consistently exceeds p99 targets, throughput capacity degrades.
3. **Data Integrity Violation**: Account Balance anomalies detected by the Reconciliation/Risk engine.

## 3. Execution Process

### 3.1 Evaluating Database Changes
*  **Backward-compatible schema**: (Preferred) The previous application version can safely reconnect and operate on the current schema. Proceed immediately to application rollback.
*  **Breaking Schema/Data Corrections**: If the new version mutated records such that rolling back the codebase leaves the system unreadable to the old code:
    1. Enter Emergency Maintenance Mode to halt trading.
    2. Exert `flyway undo` if supported, or restore from the Pre-Deployment Snapshot.
    3. Proceed to application rollback.

### 3.2 Kubernetes Deployment Rollback

Given standard stateless or semi-stateful configurations, execute a Kubernetes undo targeting the failing microservice.

```bash
# General Undo Step
kubectl rollout undo deployment/<service-name> -n dex-production

# Example: Revert the API Gateway
kubectl rollout undo deployment/gateway -n dex-production
```

### 3.3 Verifying Rollback Complete

1. **Watch Rollout**:
   ```bash
   kubectl rollout status deployment/<service-name> -n dex-production
   ```
2. **Review Replica Sets**: Ensure the older `ReplicaSet` has scaled up and is serving traffic, while the newer, problematic one has scaled down to zero.
3. **Health Check**: Test the `/health` and `/metrics` endpoints. Verify the `build_info` metric reflects the prior, stable version.
4. **Error Rates**: Validate that Loki and Prometheus show error rates returning to nominal baselines (<0.01%).

## 4. Post-Rollback Actions

1. Capture a dump of logs from the problematic release before the container instances are completely pruned from the cluster.
2. The Engineering team must analyze the root cause of the incident through a blameless post-mortem.
3. Fix the newly identified issues, augment the `ci-test.yml` chaos suites, and prepare for a subsequent, safer release cycle.
