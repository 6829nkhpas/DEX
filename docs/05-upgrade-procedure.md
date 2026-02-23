# Upgrade Procedure

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This document outlines the standard process for upgrading components of the distributed exchange. Upgrades must follow this strict sequence to maintain the system's "always-on" guarantee and adhere to the 99.99% availability SLA.

## 2. Upgrade Prerequisites

Before any production upgrade, the following conditions must be met:
1. The new version must be tagged (e.g., `v1.1.0`) and built via the `ci-build.yml` pipeline.
2. The release build must pass all end-to-end integration and chaos tests in the Staging environment.
3. Database migrations (if any) must be reviewed by the DBA team.
4. An approved Change Request (CR) must exist.

## 3. Migration and State Upgrades

### 3.1 Stateless Services
Stateless services (API Gateway, Settlement Service, Market Data Service) can be upgraded seamlessly.
- **Method**: Kubernetes Rolling Update
- **Strategy**: `maxSurge: 25%`, `maxUnavailable: 0`

### 3.2 Stateful Services
Stateful services (Matching Engine, Order Service, Risk Engine) require careful handling of their in-memory/in-flight states.
- **Matching Engine**: Requires a snapshot to be taken, state persisted, and the new pods to hydrate from the journal cleanly.
- **Drain process**: Ensure `SIGTERM` signals are trapped allowing the component to flush its current journal buffer before shutting down.

### 3.3 Database Migrations
If the upgrade includes a schema change:
1. Ensure the schema change is purely additive (Backward Compatible).
2. Run the migration script *before* the application upgrade.
   ```bash
   # Run against production DB via secure bastion
   flyway -configFiles=conf/flyway.conf migrate
   ```

## 4. Upgrade Execution Phase

1. **Notify Stakeholders**: Post to the official communication channels 30 minutes prior.
2. **Apply Manifests**: 
   ```bash
   kubectl apply -f infra/k8s/latest-release/
   ```
   Or using standard deployment scripts for specific tags:
   ```bash
   ./infra/scripts/deploy-production.sh --tag v1.1.0
   ```
3. **Monitor Rollout**:
   ```bash
   kubectl rollout status deployment/api-gateway -n dex-production
   ```

## 5. Validation

During the upgrade, actively monitor:
- **Error Rates**: Ensure they do not exceed 0.05% during the deployment window.
- **Latency**: Ensure the new version does not regress the p99 latency SLAs.
- **Logs**: Monitor Loki for `WARN` or `ERROR` messages referencing unmatched protocol versions or serialization errors.

## 6. Post-Upgrade

1. Verify the release version reflects correctly in the application metrics (`build_info` metric).
2. Close the Change Request (CR).
3. Notify users of the completed upgrade if maintenance mode was engaged.
