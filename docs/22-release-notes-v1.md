# v1.0 Release Notes

**Release date**: 2026-03-26  
**Scope**: DEX Web UI — Final production release

---

## What's Included

### Auth & Wallet Layer (Phase 19–20)

- **Wallet connect** via MetaMask / EIP-1193. `WalletProvider` handles connection, disconnect, and account-change events.
- **Nonce-based sign-in** — EIP-4361-style `personal_sign` challenge. Session valid for 24 hours.
- **Session persistence** in `sessionStorage` (tab-scoped). Restored on reload if still valid.
- **Auto-invalidation** on address change, chain change, or TTL expiry (proactive 60-second timer).
- **Auth state machine** with 7 distinct states: `disconnected`, `connecting`, `connected`, `signing`, `authenticated`, `expired`, `rejected`.

### UX & Protected Actions

- `AuthStatusBadge` in the header — adapts to every auth state with distinct UX for each.
- `AuthGatePanel` on `AccountPanel`, `OpenOrders`, and `Positions` — shows "Sign in to view" with a one-click sign-in prompt when unauthenticated.
- `OrderEntry` — internal auth gate; shows lock message when not authenticated.
- `OpenOrders` — cancel button rendered as non-interactive span when not authenticated.
- `ConnectionBanner` — full-width slim banner when WebSocket is `disconnected` or `connecting`. Disappears when market data is live.

### Market Data (Phases 14–17)

- Real-time orderbook, trade tape, and ticker via WebSocket.
- Delta buffering, gap detection, and snapshot atomicity.
- Public — no authentication required.

### Order Management (Phase 18)

- `OrderEntry`: limit/market orders, GTD support, decimal validation via `decimal.js`, rate-limit handling.
- `OpenOrders`: live order table, per-row cancel with error toasts. No optimistic removal.
- `Positions`: live position table with unrealized PnL via `decimal.js`.

---

## Known Limitations

| Limitation                           | Notes                                                                   |
| ------------------------------------ | ----------------------------------------------------------------------- |
| MetaMask-only                        | Requires EIP-1193 provider. WalletConnect not yet integrated.           |
| Single tab session                   | sessionStorage is tab-scoped; each new tab requires fresh sign-in.      |
| Mock market data in dev              | `MockEventSimulation` in `App.tsx` simulates events when WS is offline. |
| Signature not verified frontend-side | Backend must verify ECDSA signature on every sensitive API call.        |
| No KYC gate                          | KYC integration is a backend concern; the frontend has no KYC state.    |

---

## Frozen Contracts (must not change post-v1.0)

See [`docs/21-auth-wallet-flow.md §8`](./21-auth-wallet-flow.md) for the full list.

- `AuthSession` shape and storage key
- Sign-in message format
- Session TTL (24 hours)
- Protected action list
- Provider hierarchy: `WalletProvider → AuthProvider → App`
- `AuthStatus` enum values

---

## Test Coverage

| Test file                  | Coverage                                                                                                                              |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `auth-session.test.ts`     | Auth service unit tests, session CRUD, sign-in / rejection paths                                                                      |
| `wallet-account.test.ts`   | `deriveAccountId`, mock provider, account store snapshot/delta                                                                        |
| `launch-readiness.test.ts` | Regression suite — session restore, expiry, protected actions, chain/account invalidation, state machine, trading flow auth stability |
| `risk-models.test.ts`      | Risk model calculations                                                                                                               |

Run all tests:

```bash
cd apps/web-ui
npm test
```
