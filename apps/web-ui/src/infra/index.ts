// ---------------------------------------------------------------------------
// Barrel export — infrastructure modules for production hardening
// ---------------------------------------------------------------------------

export { CircuitBreaker, CircuitBreakerOpenError } from "./circuit-breaker";
export type { BreakerState, CircuitBreakerConfig, CircuitBreakerSnapshot } from "./circuit-breaker";

export { RateLimiter } from "./rate-limiter";
export type { RateLimiterConfig, RateLimiterSnapshot } from "./rate-limiter";

export { TelemetryClient, getTelemetryClient, resetTelemetryClient } from "./telemetry";
export type { TelemetryEventType, TelemetryEvent, TelemetryConfig } from "./telemetry";

export { escapeHtml, sanitizeSymbol, sanitizeId, safeDecimalDisplay, truncateDisplay } from "./safe-display";

export { TokenManager } from "./token-manager";
export type { TokenPair, TokenRefreshConfig } from "./token-manager";

export { loadConfig, getConfig, resetConfig } from "./config";
export type { AppConfig } from "./config";
