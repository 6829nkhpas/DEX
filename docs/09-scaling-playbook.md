# Scaling Playbook

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This playbook describes how to scale the decentralized exchange in response to increased market volatility and predictable usage spikes, based roughly on limits defined in `INFRA-BASELINE-v1.0.md`.

## 2. Automated Scaling (HPA)

The Kubernetes Horizontal Pod Autoscaler automatically scales all services horizontally, *except* the active Matching Engine writer for a specific symbol (which must be a singleton per symbol for determinism).

**HPA Triggers**:
- **CPU**: Scales up if average CPU utilization exceeds 70%.
- **Memory**: Scales up if memory exceeds 80%.

## 3. Manual Compute Scaling

### 3.1 Pre-Scaling for Known Events
If a major token listing is occurring, pre-scale the environment 1 hour prior:

1. **Gateway & Order Service**:
   ```bash
   kubectl scale deployment gateway --replicas=10 -n dex-production
   kubectl scale deployment order-service --replicas=10 -n dex-production
   ```

2. **Matching Engine (Vertical)**:
   Since ME is single-threaded per symbol pair, it must be scaled vertically if its CPU core is pegged.
   - Create a specific node pool with highly-clocked Compute-Optimized instances (e.g., AWS `c6id`).
   - Use node selectors/tolerations to move the overloaded ME pod to this specific node.

### 3.2 Database Scaling
- For Read-heavy loads (Market Data fetching, reporting): Add more read replicas.
- For Write-heavy loads (insane trade volume): Ensure the DB writer is vertically scaled (e.g., scaling Aurora primary instance size up). *Requires a brief failover window (~30-60s).*

## 4. Tuning Limits

If clients are hitting Rate Limits too often (429 errors) during a legitimate traffic spike, adjust the Gateway environment limits:

```bash
# Update config map
kubectl edit configmap gateway-config -n dex-production
# Values to tune:
# RATE_LIMIT_ORDERS_PER_SEC=100
# RATE_LIMIT_READS_PER_SEC=1000
```
Note: Ensure downstream DBs have capacity before lifting rate limits.
