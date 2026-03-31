# Phase 21 — Production Hardening Changelog

**Date**: 2026-03-30  
**Scope**: DEX Web UI — Final production-hardening pass  
**Branch target**: `release/v1.0.0`

---

## Summary

Phase 21 is an additive hardening pass over the Phases 19–20 auth and wallet layers. All changes are backward-compatible and do not alter existing public APIs.

---

## Changes

### 1. `WalletProvider.tsx` — Wallet Service Hardening

| Change                            | Detail                                                                                                                                                                                 |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `isReconnecting: boolean`         | New context field; `true` during the brief window after `accountsChanged` fires and before the new `accountId` finishes deriving. Allows UI to show a non-disruptive transition state. |
| `connectionError: string \| null` | New context field; surfaces the last wallet-layer error (no provider, connect rejected, etc.) without triggering a full sign-out.                                                      |
| In-flight sign guard              | `signingInFlightRef` (`useRef<boolean>`) prevents concurrent `personal_sign` calls. Second parallel call throws with a clear message.                                                  |
| Double-connect protection         | `connect()` is a no-op if `isConnecting` is already `true`.                                                                                                                            |
| Reconnect tracking                | `accountsChanged` handler sets `isReconnecting = true`; auto-clears once new `accountId` is ready.                                                                                     |
| Error surface on disconnect       | `disconnect()` clears `connectionError` and resets the sign guard.                                                                                                                     |

---

### 2. `rate-limiter.ts` — Rate Limiting Hardening

| Addition                     | Detail                                                                                                                                                   |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RateLimitError`             | Typed `Error` subclass carrying `action: string` and `waitMs: number`. Consumers can display accurate retry-in feedback.                                 |
| `RateLimiterRegistry`        | Named registry (`getOrCreate(name, config?)`) ensuring each action shares a single limiter instance per tab. Includes `remove()`, `resetAll()`, `has()`. |
| `defaultRateLimiterRegistry` | Singleton — import and share across auth, order entry, and cancel paths.                                                                                 |

---

### 3. `AuthProvider.tsx` — Auth Rate Limiter Integration

- Auth sign-in is now gated by a token-bucket limiter: **5 attempts per 60 seconds**.
- Rate-limited attempts set `error` to a friendly message with the retry-in time (in seconds).
- Status stays `"connected"` (not `"rejected"`) on rate-limit hit — this is a throttle, not a wallet rejection.
- `logger.info/warn` calls added for sign-in start, success, rate-limit, and failure paths.
- "Already in progress" wallet error from `WalletProvider` now correctly classified as a rejection.

---

### 4. `useProtectedAction.ts` — Security Invariant Hook (NEW)

Composable hook enforcing two invariants before any sensitive action:

1. `authStatus === "authenticated"` — otherwise `isDisabled = true` and `execute()` is a no-op.
2. Named rate-limiter has a token — otherwise `execute()` captures a `RateLimitError` in `rateLimitedError`.

Concurrent calls to `execute()` are serialised via `inFlightRef`. Returns `{ execute, isDisabled, rateLimitedError, lastError }`.

---

### 5. `GovernanceContext.tsx` — Governance Controls (NEW)

| Export                      | Purpose                                                                                   |
| --------------------------- | ----------------------------------------------------------------------------------------- |
| `GovernanceProvider`        | Derives `adminRole` from `session.accountId` against a static allowlist.                  |
| `GovernanceGuard`           | Renders children only if `adminRole` meets `requiredRole`. Falls back to `fallback` prop. |
| `useGovernance()`           | Exposes `adminRole` and `logAction(action)`.                                              |
| `useGovernanceAudit()`      | Exposes read-only `auditLog` array.                                                       |
| `hasRole(actual, required)` | Pure helper — `"super" > "risk" > "support" > "none"`.                                    |

> **Security note**: Frontend role checks are UX-only. All sensitive operations must enforce roles server-side.

---

### 6. `logger.ts` — Structured Logging (NEW)

Singleton `logger` with `info`, `warn`, `error` methods. Prefixes output with `[DEX][LEVEL] <timestamp>`. Silent in production when `VITE_LOG_LEVEL=silent`.

---

### 7. `phase21-hardening.test.ts` — Expanded Test Coverage (NEW)

11 new test suites (50+ assertions):

| Suite                         | Focus                                                                  |
| ----------------------------- | ---------------------------------------------------------------------- |
| Wallet reconnect / disconnect | Mock provider `accountsChanged` → address clears / switches            |
| Parallel sign blocked         | In-flight guard throws on concurrent `signMessage`                     |
| Auth rate limiter exhaustion  | 5 allowed, 6th blocked; `estimatedWaitMs > 0`; tokens refill           |
| `RateLimitError` shape        | `action`, `waitMs`, `instanceof`, message content                      |
| Named registry `getOrCreate`  | Same instance returned, different names = different instances          |
| `resetAll()`                  | All exhausted limiters restore to full capacity                        |
| Protected action guard logic  | `isDisabled` invariants, `RateLimitError` thrown path                  |
| `hasRole()` ordering          | Full matrix: `super > risk > support > none`                           |
| GovernanceGuard logic         | Role pass / block conditions                                           |
| Audit log append              | Entry shape (timestamp, action, accountId, role)                       |
| Error session recovery        | Expired session cleared; valid session survives; account switch purges |

---

## Verification

```bash
cd apps/web-ui
npm test          # all 7 test files must pass
npm run typecheck # 0 TypeScript errors
```
