// ---------------------------------------------------------------------------
// useProtectedAction — composable guard for auth + rate-limit enforcement
// ---------------------------------------------------------------------------
//
// Usage:
//   const { execute, isDisabled, rateLimitedError } = useProtectedAction(
//     "cancelOrder",
//     async () => { await client.cancelOrder(id); },
//     { capacity: 5, refillRate: 1 },
//   );
//
//   <button disabled={isDisabled} onClick={execute}>Cancel</button>
//   {rateLimitedError && <p>Rate limited: retry in {rateLimitedError.waitMs}ms</p>}
//
// Invariants enforced:
//   1. authStatus must be "authenticated" — otherwise isDisabled=true and execute() is a no-op
//   2. The named rate limiter must have a token available — otherwise execute() throws RateLimitError
//   3. Concurrent calls to execute() are serialised (one at a time via inFlightRef)
// ---------------------------------------------------------------------------

import { useCallback, useRef, useState } from "react";
import { useAuth } from "./AuthProvider";
import { useWallet } from "../wallet/WalletProvider";
import { isSessionValid } from "./authService";
import {
  defaultRateLimiterRegistry,
  RateLimitError,
  type RateLimiterConfig,
} from "../infra/rate-limiter";
import { logger } from "../infra/logger";

export interface UseProtectedActionResult {
  /** Execute the protected action. No-op if auth or rate-limit check fails. */
  execute: () => Promise<void>;
  /** True when the action cannot be executed (not authenticated, or rate limited). */
  isDisabled: boolean;
  /** Non-null when the last execute() call was blocked by the rate limiter. */
  rateLimitedError: RateLimitError | null;
  /** Non-null when the last execute() call threw an unexpected error. */
  lastError: string | null;
}

const DEFAULT_ACTION_CONFIG: Partial<RateLimiterConfig> = {
  capacity: 10,
  refillRate: 2,
};

/**
 * Wraps an async action with auth and rate-limit guards.
 *
 * @param actionName  Used as the rate-limiter registry key and log tag.
 * @param fn          The protected async function to execute.
 * @param limiterConfig  Optional rate-limiter config for this action.
 */
export function useProtectedAction(
  actionName: string,
  fn: () => Promise<void>,
  limiterConfig?: Partial<RateLimiterConfig>,
): UseProtectedActionResult {
  const { authStatus, session } = useAuth();
  const { address } = useWallet();
  const inFlightRef = useRef(false);

  const [rateLimitedError, setRateLimitedError] = useState<RateLimitError | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);

  const isAuthenticated = authStatus === "authenticated";

  // Get (or create on first use) the per-action rate limiter
  const limiter = defaultRateLimiterRegistry.getOrCreate(
    actionName,
    limiterConfig ?? DEFAULT_ACTION_CONFIG,
  );

  const isDisabled = !isAuthenticated || !limiter.canConsume();

  const execute = useCallback(async () => {
    // Belt-and-suspenders auth check at call time
    if (authStatus !== "authenticated") {
      logger.warn(`useProtectedAction: blocked — not authenticated`, { action: actionName });
      return;
    }

    // Real-time session validity check (catches expiry between poll ticks)
    if (session && address && !isSessionValid(session, address)) {
      logger.warn(`useProtectedAction: blocked — session expired or address mismatch`, { action: actionName });
      return;
    }

    // Prevent concurrent executions of the same protected action
    if (inFlightRef.current) {
      logger.warn(`useProtectedAction: blocked — already in-flight`, { action: actionName });
      return;
    }

    // Rate limit check
    if (!limiter.tryConsume()) {
      const waitMs = limiter.estimatedWaitMs();
      const err = new RateLimitError(actionName, waitMs);
      setRateLimitedError(err);
      logger.warn(`useProtectedAction: rate limited`, { action: actionName, waitMs });
      return;
    }

    // Clear previous errors
    setRateLimitedError(null);
    setLastError(null);
    inFlightRef.current = true;

    try {
      await fn();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      logger.error(`useProtectedAction: action threw`, { action: actionName, error: msg });
      setLastError(msg);
    } finally {
      inFlightRef.current = false;
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authStatus, session, address, actionName, limiter, fn]);

  return { execute, isDisabled, rateLimitedError, lastError };
}
