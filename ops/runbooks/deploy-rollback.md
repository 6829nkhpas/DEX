# Runbook: Deployment Rollback

**Severity**: P2 — service disruption risk  
**Owner**: Frontend Platform / DevOps  
**Last updated**: 2025-07-18

---

## When to Rollback

- New deployment causes errors (500s, pod crash loops, health check failures)
- Performance degradation detected by perf gate (p95 latency > 300ms)
- Users reporting broken UI, missing data, or order failures after deploy
- Canary checks fail during progressive rollout

## Pre-Rollback Assessment

```bash
# Check current deployment status
kubectl -n production rollout status deployment/dex-ui

# Check pod health
kubectl -n production get pods -l app.kubernetes.io/name=dex-ui

# Check recent events
kubectl -n production get events --sort-by='.lastTimestamp' | tail -20

# Check logs for errors
kubectl -n production logs -l app.kubernetes.io/name=dex-ui --tail=50 --since=5m
```

## Immediate Rollback Steps

### Option 1: Helm Rollback (Recommended)

```bash
# List release history
helm history dex-ui -n production

# Rollback to previous revision
helm rollback dex-ui 0 -n production --wait --timeout 5m

# Verify rollback
helm status dex-ui -n production
kubectl rollout status deployment/dex-ui -n production
```

### Option 2: Kubectl Rollback

```bash
# View rollout history
kubectl -n production rollout history deployment/dex-ui

# Rollback to previous version
kubectl -n production rollout undo deployment/dex-ui

# Rollback to specific revision
kubectl -n production rollout undo deployment/dex-ui --to-revision=<N>

# Verify
kubectl -n production rollout status deployment/dex-ui
```

### Option 3: Canary Rollback

If using the canary workflow:

```bash
# Remove canary deployment
helm uninstall dex-ui-canary -n production

# Verify stable deployment is healthy
kubectl -n production get pods -l app.kubernetes.io/name=dex-ui
```

## Post-Rollback Verification

```bash
# Health check
kubectl -n production exec deploy/dex-ui -- \
  node -e "const h=require('http');h.get('http://localhost:9091/healthz',r=>{let d='';r.on('data',c=>d+=c);r.on('end',()=>{console.log(d);process.exit(r.statusCode===200?0:1)})})"

# Metrics check
kubectl -n production port-forward svc/dex-ui 9091:9091 &
curl -s http://localhost:9091/metrics | head -20

# Check client connectivity
curl -s http://localhost:9091/readyz | jq .
```

## Rollback Communication

1. Notify the team in `#dex-deployments` Slack channel
2. Update deployment ticket with rollback reason
3. Create a post-mortem ticket if user impact occurred
4. Revert the git commit or open a fix PR before re-deploying

## Prevention

- Always deploy via canary workflow with health checks
- Monitor perf gate results before promoting to production
- Keep rollback window within 5 minutes of detecting issues
- Maintain at least 3 Helm release revisions for history
