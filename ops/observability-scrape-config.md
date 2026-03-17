# Observability Scrape Config

Example Prometheus scrape configurations for the DEX Web UI pods.

## Kubernetes Service Discovery (Recommended)

When running in Kubernetes with Prometheus Operator, the pods are auto-discovered
via annotations set in the Helm chart deployment template:

```yaml
prometheus.io/scrape: "true"
prometheus.io/port: "9091"
prometheus.io/path: "/metrics"
```

No additional scrape config is needed if your Prometheus instance is configured
to honor pod annotations (the default for `kube-prometheus-stack`).

## Manual Scrape Config

If you need explicit scrape config, add this to your `prometheus.yml`:

```yaml
scrape_configs:
  # ── DEX UI Metrics ─────────────────────────────────────────
  - job_name: "dex-ui"
    scrape_interval: 15s
    scrape_timeout: 10s
    metrics_path: /metrics

    # Option A: Kubernetes SD (pods in cluster)
    kubernetes_sd_configs:
      - role: pod
        namespaces:
          names:
            - staging
            - production
    relabel_configs:
      # Only scrape pods with the annotation
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
        action: keep
        regex: "true"
      # Use the annotated port
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_port]
        action: replace
        target_label: __address__
        regex: (.+)
        replacement: ${1}:$1
      # Use the annotated path
      - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_path]
        action: replace
        target_label: __metrics_path__
        regex: (.+)
      # Add pod name label
      - source_labels: [__meta_kubernetes_pod_name]
        action: replace
        target_label: pod
      # Add namespace label
      - source_labels: [__meta_kubernetes_namespace]
        action: replace
        target_label: namespace
      # Only match dex-ui pods
      - source_labels: [__meta_kubernetes_pod_label_app_kubernetes_io_name]
        action: keep
        regex: dex-ui

    # Option B: Static targets (for local dev / Docker Compose)
    # Uncomment and adjust if using static targets
    #
    # static_configs:
    #   - targets:
    #       - "localhost:9091"
    #     labels:
    #       environment: "development"
    #       app: "dex-ui"
```

## Exposed Metrics

| Metric                               | Type      | Description                                           |
| ------------------------------------ | --------- | ----------------------------------------------------- |
| `dex_uptime_seconds`                 | gauge     | Time since observability server started               |
| `dex_events_ignored_total`           | counter   | Total duplicate events ignored                        |
| `dex_gaps_detected_total`            | counter   | Total sequence gaps detected                          |
| `dex_buffer_size_total`              | gauge     | Total buffered events across streams                  |
| `dex_connected_clients`              | gauge     | Number of active WebSocket connections                |
| `dex_last_seq_by_stream{stream}`     | gauge     | Last sequence number per data stream                  |
| `dex_ws_last_seq{symbol}`            | gauge     | Last WS sequence per symbol                           |
| `dex_event_to_store_latency_seconds` | histogram | Event dispatch latency                                |
| `dex_circuit_breaker_state`          | gauge     | Circuit breaker state (0=closed, 1=open, 2=half_open) |

## Alert Rules

Alert rules are defined in `deploy/prometheus/rules.yaml`. Import them via:

```bash
# PrometheusRule CRD
kubectl apply -f deploy/prometheus/rules.yaml

# Or add to Prometheus config
rule_files:
  - /etc/prometheus/rules/dex-ui-rules.yaml
```

## Grafana Dashboards

Import the JSON dashboards from `deploy/grafana/dashboards/`:

- `latency-dashboard.json` — End-to-end latency (p50/p95/p99)
- `buffers-sequences.json` — Buffer sizes, sequence tracking
- `resource-monitor.json` — Heap, CPU, pod counts

Import via Grafana UI → Dashboards → Import → Upload JSON file.
