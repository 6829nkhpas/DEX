# Disaster Recovery Checklist

Complete recovery procedure for the DEX Web UI in case of catastrophic failure
(cluster loss, data corruption, complete outage).

---

## Pre-Requisites for Recovery

- [ ] Access to container registry (images)
- [ ] Access to Helm chart source (this repository)
- [ ] Kubernetes cluster with sufficient capacity
- [ ] Container registry credentials
- [ ] TLS certificates or cert-manager configured
- [ ] DNS access for domain updates
- [ ] Monitoring stack (Prometheus + Grafana) available

## Recovery Priority Order

```
1. Infrastructure (cluster, networking)
2. Backend services (matching engine, persistence, gateway)
3. Frontend (DEX Web UI) ← this checklist
4. Monitoring & alerting
5. Verification & smoke tests
```

> **Note**: The Web UI is a stateless SPA. Recovery is straightforward since
> all state is derived from backend services via WebSocket events and REST
> API calls. No client-side data needs to be restored.

---

## Phase 1: Infrastructure Verification

- [ ] Kubernetes cluster is operational
  ```bash
  kubectl cluster-info
  kubectl get nodes
  ```
- [ ] Container registry is accessible
  ```bash
  docker pull ${REGISTRY}/dex-web-ui:latest
  ```
- [ ] Ingress controller is running
  ```bash
  kubectl -n ingress-nginx get pods
  ```
- [ ] cert-manager / TLS certs available
  ```bash
  kubectl get clusterissuers
  kubectl get certificates -A
  ```

## Phase 2: Namespace & Secrets

- [ ] Create namespace
  ```bash
  kubectl create namespace production
  ```
- [ ] Create registry pull secret
  ```bash
  kubectl create secret docker-registry registry-credentials \
    --docker-server=$REGISTRY \
    --docker-username=$REG_USER \
    --docker-password=$REG_PASS \
    -n production
  ```
- [ ] Create application secrets
  ```bash
  kubectl create secret generic dex-ui-secrets \
    --from-literal=VITE_AUTH_TOKEN=$AUTH_TOKEN \
    --from-literal=VITE_WS_TOKEN=$WS_TOKEN \
    -n production
  ```
- [ ] Create TLS secret (if not using cert-manager)
  ```bash
  kubectl create secret tls dex-ui-tls \
    --cert=tls.crt --key=tls.key -n production
  ```

## Phase 3: Deploy Application

- [ ] Verify backend services are running
  ```bash
  kubectl -n production get pods -l app=gateway
  kubectl -n production get pods -l app=matching-engine
  ```
- [ ] Deploy with Helm
  ```bash
  helm upgrade --install dex-ui deploy/helm/dex-ui \
    -f deploy/helm/dex-ui/values-prod.yaml \
    --namespace production \
    --set image.repository=${REGISTRY}/dex-web-ui \
    --set image.tag=prod-latest \
    --wait --timeout 5m
  ```
- [ ] Verify pods are running
  ```bash
  kubectl -n production get pods -l app.kubernetes.io/name=dex-ui
  kubectl -n production rollout status deployment/dex-ui
  ```

## Phase 4: Health Verification

- [ ] Health endpoint responds
  ```bash
  kubectl -n production port-forward svc/dex-ui 9091:9091 &
  curl -fsS http://localhost:9091/healthz
  ```
- [ ] Readiness check passes
  ```bash
  curl -fsS http://localhost:9091/readyz
  ```
- [ ] Metrics endpoint returns data
  ```bash
  curl -fsS http://localhost:9091/metrics | grep "dex_"
  ```
- [ ] UI is accessible via ingress
  ```bash
  curl -fsS https://dex.example.com/ | head -5
  ```

## Phase 5: Data Integrity

- [ ] WebSocket connection establishes
  ```bash
  # Check via in-app debug panel or browser DevTools
  ```
- [ ] Orderbook data loads (verify against REST API)
  ```bash
  curl -s http://gateway:8080/v1/orderbook/BTC-USDC | jq '.bids[:3]'
  ```
- [ ] Sequence numbers are advancing (no stale state)
  ```bash
  curl -s http://localhost:9091/metrics | grep "dex_last_seq"
  ```
- [ ] No sequence gaps detected
  ```bash
  curl -s http://localhost:9091/metrics | grep "dex_gaps_detected"
  ```

## Phase 6: Monitoring Restoration

- [ ] Prometheus scraping the new pods
  ```bash
  # Check Prometheus targets page
  curl -s http://prometheus:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job=="dex-ui")'
  ```
- [ ] Alert rules loaded
  ```bash
  kubectl apply -f deploy/prometheus/rules.yaml
  ```
- [ ] Grafana dashboards imported
  ```bash
  # Import from deploy/grafana/dashboards/*.json
  ```
- [ ] Alertmanager receiving alerts
  ```bash
  curl -s http://alertmanager:9093/api/v2/status | jq .
  ```

## Phase 7: DNS & External Access

- [ ] DNS records point to new cluster/ingress IP
- [ ] SSL certificate valid and not expired
- [ ] CDN cache purged (if applicable)
- [ ] Rate limiting configured on ingress

## Phase 8: Post-Recovery Validation

- [ ] Run performance benchmark
  ```bash
  cd apps/web-ui
  npx tsx perf/bench-runner.ts --rate 100 --duration 30
  ```
- [ ] Verify all KPIs pass
- [ ] HPA operational (scale up/down test)
  ```bash
  kubectl -n production get hpa
  ```
- [ ] Notify stakeholders of recovery completion

---

## Recovery Time Objectives

| Component          | RTO    | RPO                          |
| ------------------ | ------ | ---------------------------- |
| Web UI (stateless) | 15 min | N/A (no persistent state)    |
| Backend services   | 30 min | Depends on persistence layer |
| Full platform      | 1 hour | Last committed state         |

## Emergency Contacts

| Role                | Contact                  |
| ------------------- | ------------------------ |
| Frontend on-call    | _Configure in PagerDuty_ |
| Backend on-call     | _Configure in PagerDuty_ |
| Infrastructure      | _Configure in PagerDuty_ |
| Engineering Manager | _Configure_              |
