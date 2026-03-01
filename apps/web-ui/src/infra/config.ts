// ---------------------------------------------------------------------------
// Environment configuration — reads all config from environment variables
// ---------------------------------------------------------------------------
//
// No secret literals in code. All sensitive values come from env vars.
// Falls back to safe dev defaults when env vars are not set.
// ---------------------------------------------------------------------------

export interface AppConfig {
  // API
  apiBaseUrl: string;
  wsUrl: string;

  // Auth
  wsToken: string;
  authToken: string;

  // Telemetry
  telemetryEndpoint: string;
  telemetrySampleRate: number;
  telemetryEnabled: boolean;

  // Circuit breaker
  cbFailureThreshold: number;
  cbCooldownMs: number;

  // Rate limiter
  rlCapacity: number;
  rlRefillRate: number;

  // Feature flags
  devMode: boolean;
  debugPanelEnabled: boolean;
  metricsEnabled: boolean;
}

function envStr(key: string, fallback: string): string {
  if (typeof process !== "undefined" && process.env && process.env[key]) {
    return process.env[key]!;
  }
  // Vite uses import.meta.env
  if (typeof import.meta !== "undefined" && (import.meta as any).env?.[key]) {
    return (import.meta as any).env[key];
  }
  return fallback;
}

function envNum(key: string, fallback: number): number {
  const raw = envStr(key, "");
  if (!raw) return fallback;
  const n = Number(raw);
  return Number.isFinite(n) ? n : fallback;
}

function envBool(key: string, fallback: boolean): boolean {
  const raw = envStr(key, "").toLowerCase();
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  return fallback;
}

export function loadConfig(): AppConfig {
  return {
    apiBaseUrl: envStr("VITE_API_BASE_URL", "/v1"),
    wsUrl: envStr("VITE_WS_URL", "ws://localhost:8080/v1/ws"),
    wsToken: envStr("VITE_WS_TOKEN", "dev-token"),
    authToken: envStr("VITE_AUTH_TOKEN", "dev-token"),
    telemetryEndpoint: envStr("VITE_TELEMETRY_ENDPOINT", "/telemetry"),
    telemetrySampleRate: envNum("VITE_TELEMETRY_SAMPLE_RATE", 0.01),
    telemetryEnabled: envBool("VITE_TELEMETRY_ENABLED", true),
    cbFailureThreshold: envNum("VITE_CB_FAILURE_THRESHOLD", 5),
    cbCooldownMs: envNum("VITE_CB_COOLDOWN_MS", 30000),
    rlCapacity: envNum("VITE_RL_CAPACITY", 10),
    rlRefillRate: envNum("VITE_RL_REFILL_RATE", 2),
    devMode: envBool("VITE_DEV_MODE", true),
    debugPanelEnabled: envBool("VITE_DEBUG_PANEL", true),
    metricsEnabled: envBool("VITE_METRICS_ENABLED", true),
  };
}

// Singleton instance
let _config: AppConfig | null = null;

export function getConfig(): AppConfig {
  if (!_config) {
    _config = loadConfig();
  }
  return _config;
}

export function resetConfig(): void {
  _config = null;
}
