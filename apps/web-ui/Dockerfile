# ──────────────────────────────────────────────────────
# Dockerfile: DEX Web UI
# Multi-stage build — builder → production runtime
# ──────────────────────────────────────────────────────

# ── Stage 1: Build ───────────────────────────────────
FROM node:20-slim AS builder

WORKDIR /build

# Copy dependency manifests first (layer cache)
COPY package.json package-lock.json ./

# Install ALL dependencies (dev + prod) for build
RUN npm ci --ignore-scripts

# Copy source and config
COPY index.html tsconfig.json vite.config.ts ./
COPY src/ src/
COPY perf/ perf/
COPY tools/ tools/

# Copy shared types (needed by tsconfig paths)
COPY ../../types/ /types/

# TypeScript check
RUN npx tsc --noEmit

# Build production bundle
RUN npx vite build

# ── Stage 2: Production Runtime ──────────────────────
FROM node:20-slim AS runtime

# Security: create non-root user
RUN groupadd --gid 1001 dex && \
    useradd --uid 1001 --gid dex --shell /bin/false --create-home dex

WORKDIR /app

# Copy built assets
COPY --from=builder /build/dist ./dist

# Copy observability server (serves /healthz, /readyz, /metrics)
COPY --from=builder /build/node_modules ./node_modules
COPY ../../ops/observability-server.ts ./observability-server.ts

# Install only what's needed for the observability sidecar
# The static assets are served by nginx/ingress in production;
# the Node process here serves health + metrics endpoints only.
COPY package.json ./

# Switch to non-root user
USER dex

# Expose ports:
#   3000 — static file server (if using serve/vite preview)
#   9091 — observability endpoints (/healthz, /readyz, /metrics)
EXPOSE 3000 9091

# Health check against the observability server
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD node -e "const http=require('http');const r=http.get('http://localhost:9091/healthz',res=>{process.exit(res.statusCode===200?0:1)});r.on('error',()=>process.exit(1))"

# Start: serve static files + observability server
# In production, the static files are typically served by an ingress/CDN.
# This entrypoint serves as a fallback and runs the metrics sidecar.
CMD ["node", "-e", "\
  const {execSync}=require('child_process');\
  const {createServer}=require('http');\
  const fs=require('fs');\
  const path=require('path');\
  const MIME={'.html':'text/html','.js':'application/javascript','.css':'text/css','.json':'application/json','.svg':'image/svg+xml','.png':'image/png','.ico':'image/x-icon'};\
  const dist=path.join(__dirname,'dist');\
  createServer((req,res)=>{\
    let fp=path.join(dist,req.url==='/'?'index.html':req.url);\
    if(!fs.existsSync(fp))fp=path.join(dist,'index.html');\
    const ext=path.extname(fp);\
    res.writeHead(200,{'Content-Type':MIME[ext]||'application/octet-stream'});\
    fs.createReadStream(fp).pipe(res);\
  }).listen(3000,()=>console.log('[web-ui] Serving on :3000'));\
"]
