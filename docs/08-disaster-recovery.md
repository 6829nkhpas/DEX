# Disaster Recovery Plan

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

The Disaster Recovery (DR) plan is built upon the catastrophic failure strategies defined in Spec 10. The system guarantees an RTO (Recovery Time Objective) of 5 minutes and an RPO (Recovery Point Objective) of 0 (no data loss).

## 2. Disaster Types

1. **Availability Zone (AZ) Loss**: AWS/GCP zone goes down entirely.
2. **Region Loss**: Entire primary region goes offline.
3. **Data Loss / Corruption**: Catastrophic database corruption or malicious actor dropping tables.

## 3. Recovery Procedures

### 3.1 Multi-AZ Failover (Automated)
* **Databases**: Synchronous replication guarantees the standby in another AZ is immediately ready. RDS/Aurora handles the DNS switch.
* **Stateless Compute**: Kubernetes HPA will automatically spin up new pods in surviving AZs based on node pool availability.
* **Intervention Needed**: None. Monitor error rates during the 60-second transition.

### 3.2 Regional DR Failover (Manual)
If `us-east-1` is totally lost, fail over to the passive `us-west-2` environment.

**Execution steps**:
1. Confirm primary region is unrecoverable within SLA.
2. **Promote Replicas**: In DR region, promote the cross-region DB replicas to Primary.
3. **Deploy Compute**: Scale the DR Kubernetes cluster from 0 replicas to production targets:
   ```bash
   ./infra/scripts/failover-region.sh --target us-west-2
   ```
4. **DNS Switch**: Update Route53/Cloudflare to point API Gateway public records to the DR load balancer IP.
5. **Verify**: Run smoke tests and ensure Matching Engine is rebuilding from the latest journal checkpoints.

### 3.3 Data Corruption / Ransomware (Manual)
If the primary database is logically corrupted (bad data replicated everywhere):

1. **Halt System**: 
   ```bash
   ./infra/scripts/toggle-maintenance.sh --enable
   ```
2. **Point in Time Recovery (PITR)**: Restore the database to the exact millisecond before the corruption event.
3. **Journal Replay**: Replay events sequentially via Event Sourcing (Spec 10, section 6.1) until the healthy state is restored. Validate via checksums.
4. **Resume**: Disable maintenance mode.

## 4. DR Drills

- Planned Multi-AZ tests happen monthly.
- A full Regional Failover Drill must occur **Quarterly** during a scheduled maintenance window.
