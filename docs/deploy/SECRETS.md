# Deployment Secrets & Configuration

Complete reference for all secrets, environment variables, and configuration
required to deploy the DEX Web UI to staging and production.

---

## Required Secrets

| Secret Name         | Used By   | Description                                                               |
| ------------------- | --------- | ------------------------------------------------------------------------- |
| `REGISTRY`          | CI, Helm  | Container registry URL (e.g. `ghcr.io/org`)                               |
| `REGISTRY_USERNAME` | CI        | Registry authentication username                                          |
| `REGISTRY_PASSWORD` | CI        | Registry authentication password or token                                 |
| `KUBE_CONFIG`       | CI        | Base64-encoded kubeconfig for the target cluster                          |
| `STAGING_URL`       | CI        | Staging endpoint for smoke tests (e.g. `https://staging-dex.example.com`) |
| `PROD_URL`          | CI Canary | Production endpoint for health checks                                     |
| `SLACK_WEBHOOK`     | CI Canary | Slack webhook URL for deployment notifications (optional)                 |

### GitHub Actions Secrets

Set these in **Settings → Secrets and variables → Actions**:

```
REGISTRY=ghcr.io/your-org
REGISTRY_USERNAME=your-username
REGISTRY_PASSWORD=ghp_xxxxxxxxxxxxx
KUBE_CONFIG=<base64 of ~/.kube/config>
STAGING_URL=https://staging-dex.example.com
PROD_URL=https://dex.example.com
SLACK_WEBHOOK=https://hooks.slack.com/services/xxx/yyy/zzz
```

---

## Kubernetes Secrets

### Registry Credentials

```bash
kubectl create secret docker-registry registry-credentials \
  --docker-server=$REGISTRY \
  --docker-username=$REGISTRY_USERNAME \
  --docker-password=$REGISTRY_PASSWORD \
  --namespace=staging
```

### TLS Certificate

If using cert-manager (recommended), the TLS secret is automatically created
by the `ClusterIssuer`. Otherwise, create manually:

```bash
kubectl create secret tls dex-ui-tls \
  --cert=path/to/tls.crt \
  --key=path/to/tls.key \
  --namespace=production
```

### Application Secrets

```bash
kubectl create secret generic dex-ui-secrets \
  --from-literal=VITE_AUTH_TOKEN=your-auth-token \
  --from-literal=VITE_WS_TOKEN=your-ws-token \
  --namespace=production
```

---

## Environment Variables (Build-time)

These are injected via Helm `ConfigMap` and consumed by Vite at build time:

| Variable                     | Default                     | Description                       |
| ---------------------------- | --------------------------- | --------------------------------- |
| `VITE_API_BASE_URL`          | `/v1`                       | REST API base path                |
| `VITE_WS_URL`                | `ws://localhost:8080/v1/ws` | WebSocket endpoint                |
| `VITE_TELEMETRY_ENDPOINT`    | `/telemetry`                | Telemetry collector URL           |
| `VITE_TELEMETRY_SAMPLE_RATE` | `0.01`                      | Telemetry sampling rate (0–1)     |
| `VITE_TELEMETRY_ENABLED`     | `true`                      | Enable/disable telemetry          |
| `VITE_DEV_MODE`              | `false`                     | Dev mode (console logging)        |
| `VITE_DEBUG_PANEL`           | `false`                     | Show debug panel in UI            |
| `VITE_METRICS_ENABLED`       | `true`                      | Enable metrics collection         |
| `VITE_CB_FAILURE_THRESHOLD`  | `5`                         | Circuit breaker failure threshold |
| `VITE_CB_COOLDOWN_MS`        | `30000`                     | Circuit breaker cooldown (ms)     |
| `VITE_RL_CAPACITY`           | `10`                        | Rate limiter token capacity       |
| `VITE_RL_REFILL_RATE`        | `2`                         | Rate limiter refill rate/sec      |
| `VITE_WS_TOKEN`              | _(none)_                    | WebSocket auth token              |
| `VITE_AUTH_TOKEN`            | _(none)_                    | REST API auth token               |

---

## Helm Values Overrides

### Staging

Key overrides in `values-staging.yaml`:

- `replicaCount: 1` (single replica)
- `autoscaling.enabled: false`
- `image.pullPolicy: Always`
- `config.VITE_DEBUG_PANEL: "true"`

### Production

Key overrides in `values-prod.yaml`:

- `replicaCount: 3` (HA)
- `autoscaling: min 3, max 20, CPU target 60%`
- `image.pullPolicy: IfNotPresent`
- `ingress: rate-limit 100 req/s, TLS enforced`
- `config.VITE_TELEMETRY_SAMPLE_RATE: "0.001"`

---

## Alertmanager / PagerDuty / Slack

The Helm chart accepts `alertmanager.endpoint` in `values.yaml`:

```yaml
alertmanager:
  endpoint: "https://alertmanager.example.com"
```

For PagerDuty or Slack integration, configure Alertmanager receivers:

```yaml
# alertmanager.yml
receivers:
  - name: dex-slack
    slack_configs:
      - api_url: "https://hooks.slack.com/services/xxx/yyy/zzz"
        channel: "#dex-alerts"
        title: "{{ .GroupLabels.alertname }}"
  - name: dex-pagerduty
    pagerduty_configs:
      - service_key: "your-pagerduty-key"
```

---

## Optional: Vault Integration

If using HashiCorp Vault for secret injection:

```bash
export VAULT_ADDR=https://vault.example.com
export VAULT_TOKEN=your-vault-token

# Store secrets
vault kv put secret/dex-ui \
  registry_password=$REGISTRY_PASSWORD \
  auth_token=$AUTH_TOKEN \
  ws_token=$WS_TOKEN

# In Helm, use vault-agent-injector annotations
```

---

## Checklist

Before first deployment, verify:

- [ ] Container registry credentials configured
- [ ] Kubernetes cluster access (kubeconfig) available
- [ ] TLS certificate or cert-manager issuer ready
- [ ] Application secrets created in target namespace
- [ ] Alertmanager endpoint configured
- [ ] Prometheus scraping enabled for the namespace
- [ ] Grafana dashboards imported
