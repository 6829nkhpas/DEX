# Infrastructure Baseline — v1.0.0

**Status**: FROZEN  
**Date**: 2026-02-23  
**Authority**: Infra Phase

---

## 1. Component Inventory

### 1.1 Dockerfiles

| Service | Dockerfile | Base Image | Port |
|---------|-----------|------------|------|
| gateway | `infra/docker/Dockerfile.gateway` | `rust:1.77-slim-bookworm` → `debian:bookworm-slim` | 8080 |
| matching-engine | `infra/docker/Dockerfile.matching-engine` | `rust:1.77-slim-bookworm` → `debian:bookworm-slim` | 8081 |
| market-data | `infra/docker/Dockerfile.market-data` | `rust:1.77-slim-bookworm` → `debian:bookworm-slim` | 8082 |
| risk-engine | `infra/docker/Dockerfile.risk-engine` | `rust:1.77-slim-bookworm` → `debian:bookworm-slim` | 8083 |
| persistence | `infra/docker/Dockerfile.persistence` | `rust:1.77-slim-bookworm` → `debian:bookworm-slim` | 8084 |

### 1.2 Docker Compose

- **File**: `infra/docker-compose.yml`
- **Services**: 5 DEX services + Redis + Prometheus + Grafana + Loki + Promtail
- **Networks**: `dex-dmz` (public), `dex-internal` (private)

### 1.3 Kubernetes Manifests

| Resource | File |
|----------|------|
| Namespace | `infra/k8s/namespace.yml` |
| Gateway Deployment+Service+HPA | `infra/k8s/gateway/deployment.yml` |
| Matching Engine Deployment+Service+HPA | `infra/k8s/matching-engine/deployment.yml` |
| Market Data Deployment+Service+HPA | `infra/k8s/market-data/deployment.yml` |
| Risk Engine Deployment+Service+HPA | `infra/k8s/risk-engine/deployment.yml` |
| Persistence Deployment+Service+HPA | `infra/k8s/persistence/deployment.yml` |
| Ingress (TLS) | `infra/k8s/ingress.yml` |
| Network Policies | `infra/k8s/network-policy.yml` |
| Sealed Secrets | `infra/k8s/secrets/sealed-secrets.yml` |

### 1.4 Scaling Configuration

| Service | Min Replicas | Max Replicas | CPU Request | Memory Request |
|---------|-------------|-------------|------------|----------------|
| gateway | 3 | 10 | 250m | 256Mi |
| matching-engine | 10 | 20 | 2000m | 2Gi |
| market-data | 3 | 8 | 250m | 512Mi |
| risk-engine | 2 | 6 | 500m | 512Mi |
| persistence | 3 | 8 | 250m | 512Mi |

### 1.5 Monitoring

| Component | Configuration |
|-----------|--------------|
| Prometheus | `infra/monitoring/prometheus/prometheus.yml` |
| Alert Rules (P0–P3) | `infra/monitoring/prometheus/alert-rules.yml` |
| Grafana Overview | `infra/monitoring/grafana/dashboards/dex-overview.json` |
| Grafana ME Dashboard | `infra/monitoring/grafana/dashboards/matching-engine-dashboard.json` |
| Grafana Datasources | `infra/monitoring/grafana/provisioning/datasources.yml` |
| Loki | `infra/monitoring/loki/loki-config.yml` |
| Promtail | `infra/monitoring/promtail/promtail-config.yml` |

### 1.6 CI/CD

| Pipeline | File | Trigger |
|----------|------|---------|
| Build | `infra/ci/ci-build.yml` | push/PR to main, develop |
| Test | `infra/ci/ci-test.yml` | push/PR to main, develop |
| Release | `infra/ci/release.yml` | version tag `v*.*.*` |

### 1.7 Configuration

| File | Purpose |
|------|---------|
| `infra/config/env.staging.env` | Staging environment |
| `infra/config/env.testnet.env` | Testnet environment |
| `infra/config/env.production.env` | Production environment |
| `infra/config/config-loader.sh` | Runtime config validation |
| `infra/network/network.yml` | Network topology definition |

### 1.8 Deployment Scripts

| Script | Purpose |
|--------|---------|
| `infra/scripts/deploy-staging.sh` | Deploy to staging K8s |
| `infra/scripts/deploy-testnet.sh` | Deploy to testnet K8s |

---

## 2. Spec Compliance

| Spec | Requirement | Implemented |
|------|------------|-------------|
| §09 Service Boundaries | DMZ/internal separation | Network policies, ingress |
| §09 Service Scaling | Per-service scaling strategy | HPA per service |
| §09 SLAs | p99 latency targets | Alert rules |
| §10 Failure Recovery | Health checks, circuit breakers | Readiness/liveness probes, env config |
| §10 Monitoring | Health indicators | Prometheus + Grafana |
| §10 Auto-remediation | Restart on crash/memory | K8s restart policy, resource limits |
| §18 Rate Limiting | Token bucket config | Environment config |
| §19 Security | TLS, non-root, network isolation | Ingress TLS, securityContext, NetworkPolicy |
| §19 Test Coverage | CI gates | ci-test.yml with clippy/fmt/tests |
| §19 Metric Collection | <60s staleness | 5-15s scrape intervals |

---

## 3. Version Lock

- **Rust builder**: `1.77-slim-bookworm`
- **Runtime**: `debian:bookworm-slim`
- **Prometheus**: `v2.51.0`
- **Grafana**: `10.4.0`
- **Loki**: `2.9.6`
- **Promtail**: `2.9.6`
- **Redis**: `7-alpine`

---

## 4. Frozen

This baseline is **FROZEN** as of v1.0.0. Changes require:
1. Version bump in this document
2. PR review by infra owner
3. All CI checks passing
