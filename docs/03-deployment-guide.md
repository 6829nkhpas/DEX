# Deployment Guide

**Phase**: Launch & Operational Docs  
**Component**: Operations

## 1. Overview

This guide details the deployment process for the distributed exchange, ensuring it aligns with the infrastructure baselines and specification boundaries for production, staging, and testnet environments.

## 2. Prerequisites

The deployment targets a Kubernetes (K8s) environment based on the `INFRA-BASELINE-v1.0.md`.

*   **Kubernetes Cluster**: Minimum v1.26.
*   **Helm**: v3+.
*   **kubectl**: Configured with cluster admin rights.
*   **OCI Registry**: Configured with access to pull the `rust:1.77-slim-bookworm` toolchain and custom images.
*   **Sealed Secrets**: Controller running in the cluster.

## 3. Deployment Artifacts

All Dockerfiles and K8s manifests are located in the `/infra` directory.

### 3.1 Provided Dockerfiles

*   `infra/docker/Dockerfile.gateway` (Port 8080)
*   `infra/docker/Dockerfile.matching-engine` (Port 8081)
*   `infra/docker/Dockerfile.market-data` (Port 8082)
*   `infra/docker/Dockerfile.risk-engine` (Port 8083)
*   `infra/docker/Dockerfile.persistence` (Port 8084)

## 4. Environment Bootstrapping

Deployments are isolated by Kubernetes namespaces.

### 4.1 Base Infrastructure Setup

1.  **Create Namespace**:
    ```bash
    kubectl apply -f infra/k8s/namespace.yml
    ```
2.  **Apply Network Policies**:
    ```bash
    kubectl apply -f infra/k8s/network-policy.yml
    ```
    *Note: This isolates the DMZ (Gateway) from internal services per Spec 09.*
3.  **Apply Secrets**:
    ```bash
    kubectl apply -f infra/k8s/secrets/sealed-secrets.yml
    ```

### 4.2 Application Deployment

The application deployment is fully automated via CI/CD, but can be manually triggered using the provided scripts.

**To deploy to staging:**
```bash
./infra/scripts/deploy-staging.sh
```

**To deploy to testnet:**
```bash
./infra/scripts/deploy-testnet.sh
```

These scripts perform the following:
1. Validate the runtime config via `infra/config/config-loader.sh`.
2. Apply deployments out of `infra/k8s/<service>/deployment.yml`.
3. Wait for pod readiness.

## 5. Scaling Configuration

The system is configured with Horizontal Pod Autoscalers (HPA) to manage load dynamically based on CPU and Memory targets.

| Service | Min Replicas | Max Replicas | CPU Request |
|---------|-------------|-------------|------------|
| gateway | 3 | 10 | 250m |
| matching-engine | 10 | 20 | 2000m |
| market-data | 3 | 8 | 250m |
| risk-engine | 2 | 6 | 500m |
| persistence | 3 | 8 | 250m |

## 6. Access and Ingress

External traffic is routed exclusively through the Gateway.

1.  Apply the ingress specifications:
    ```bash
    kubectl apply -f infra/k8s/ingress.yml
    ```
2.  Ensure TLS certificates are provisioned (managed via Cert-Manager annotations within the ingress spec).

## 7. Verification

Post-deployment, ensure all services are reporting as healthy.

1.  Check Pod Status:
    ```bash
    kubectl get pods -n dex-production
    ```
2.  Verify Health Endpoints:
    ```bash
    curl https://api.dex.example.com/health
    ```
    Expected output: `{"status": "healthy"}`

## 8. Rollback Command

If a deployment fails, use standard k8s rollback commands:
```bash
kubectl rollout undo deployment/<service-name> -n dex-production
```
*(Further details in the Rollback Procedure documentation)*
