// ---------------------------------------------------------------------------
// logger.ts — structured logger for the DEX web UI
// ---------------------------------------------------------------------------
//
// In dev mode (VITE_LOG_LEVEL !== "silent"): emits prefixed console output
// with structured context. In production (VITE_LOG_LEVEL=silent): no-ops.
//
// Usage:
//   import { logger } from "../infra/logger";
//   logger.info("Wallet connected", { address });
//   logger.warn("Session expired", { accountId, expiresAt });
//   logger.error("Sign-in failed", { reason: err.message });
// ---------------------------------------------------------------------------

type LogContext = Record<string, unknown>;

type LogLevel = "info" | "warn" | "error";

function isSilent(): boolean {
  // In Vite environments this is replaced by define. Falls back to false.
  try {
    const level =
      typeof import.meta !== "undefined" &&
      (import.meta as { env?: { VITE_LOG_LEVEL?: string } }).env?.VITE_LOG_LEVEL;
    return level === "silent";
  } catch {
    return false;
  }
}

function emit(level: LogLevel, msg: string, ctx?: LogContext): void {
  if (isSilent()) return;

  const prefix = `[DEX][${level.toUpperCase()}]`;
  const ts = new Date().toISOString();

  if (level === "error") {
    // eslint-disable-next-line no-console
    console.error(prefix, ts, msg, ctx ?? "");
  } else if (level === "warn") {
    // eslint-disable-next-line no-console
    console.warn(prefix, ts, msg, ctx ?? "");
  } else {
    // eslint-disable-next-line no-console
    console.log(prefix, ts, msg, ctx ?? "");
  }
}

/** Singleton structured logger. Import and use instead of direct console calls. */
export const logger = {
  info(msg: string, ctx?: LogContext): void {
    emit("info", msg, ctx);
  },
  warn(msg: string, ctx?: LogContext): void {
    emit("warn", msg, ctx);
  },
  error(msg: string, ctx?: LogContext): void {
    emit("error", msg, ctx);
  },
} as const;
