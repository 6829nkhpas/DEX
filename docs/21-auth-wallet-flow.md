# Auth & Wallet Flow — Production Reference

**Phase**: v1.0 Release  
**Frozen**: 2026-03-26

## 1. Architecture Overview

```
WalletProvider (EIP-1193)
  └─ AuthProvider (nonce-based session)
       ├─ AuthStatusBadge (header widget)
       ├─ AuthGate / AuthGatePanel (protection wrappers)
       └─ Pages (MarketPage, RiskPage, ...)
```

`WalletProvider` **must** be the outer ancestor. `AuthProvider` consumes `useWallet()` internally.

---

## 2. Auth State Machine

```
disconnected ──[connect wallet]──► connecting ──[success]──► connected
                                                └[fail]────► disconnected

connected ──[signIn()]──► signing ──[signature]──► authenticated
                                 └[rejected]─────► rejected ──[retry]──► signing
                                 └[error]─────────► connected

authenticated ──[signOut()]──────► connected
authenticated ──[address change]─► connected  (session cleared)
authenticated ──[chain change]───► connected  (session cleared)
authenticated ──[session TTL]────► expired ──[signIn()]──► signing

expired ──[signIn()]─► signing
```

### State Descriptions

| Status          | Wallet    | Session        | UI Action                   |
| --------------- | --------- | -------------- | --------------------------- |
| `disconnected`  | ✗         | None           | "Connect Wallet" button     |
| `connecting`    | In-flight | None           | Pulsing spinner             |
| `connected`     | ✓         | None / cleared | "Sign In" button            |
| `signing`       | ✓         | None           | "Awaiting signature…"       |
| `authenticated` | ✓         | Valid          | "Sign Out" button           |
| `expired`       | ✓         | Expired        | "Session expired — Sign In" |
| `rejected`      | ✓         | None           | "Rejected — Retry" button   |

---

## 3. Session Lifecycle

- **Storage**: `sessionStorage` under key `dex_auth_session_v1`
- **TTL**: 24 hours from `issuedAt`
- **Scope**: Single browser tab (sessionStorage is tab-scoped)
- **Invalidation triggers**:
  - Explicit `signOut()` call
  - EIP-1193 `accountsChanged` event (address change)
  - EIP-1193 `chainChanged` event
  - Proactive expiry timer (polls every 60 s)
  - Page refresh with expired session (checked on mount)

### Session Shape (`AuthSession`)

```typescript
{
  address: string; // checksummed wallet address
  signature: string; // hex ECDSA signature from personal_sign
  nonce: string; // 64-char hex, one-time-use, replay prevention
  issuedAt: string; // ISO 8601 UTC
  expiresAt: string; // ISO 8601 UTC (issuedAt + 24h)
  accountId: string; // UUID derived from address via SHA-256
}
```

---

## 4. Sign-In Message Format (EIP-4361-style)

```
DEX Authentication Request

Address: <checksummed wallet address>
Nonce: <64-char hex nonce>
Issued At: <ISO 8601 timestamp>

By signing this message you confirm you own this wallet.
This request does not cost gas or send any transaction.
```

- **Deterministic**: identical inputs → identical output
- **Human-readable**: visible in MetaMask UI before signing
- **No gas**: `personal_sign`, not a transaction

---

## 5. Protected Actions

The following actions require `authStatus === "authenticated"`:

| Action                    | Guard                                                |
| ------------------------- | ---------------------------------------------------- |
| Order submit              | `OrderEntry` — `isSubmitDisabled = !isAuthenticated` |
| Order cancel              | `OpenOrders` — disabled span if not authenticated    |
| Account balances panel    | `AuthGatePanel` in `MarketPage`                      |
| Open orders panel         | `AuthGatePanel` in `MarketPage`                      |
| Positions panel           | `AuthGatePanel` in `MarketPage`                      |
| Deposit / Withdraw modals | `AccountPanel` — buttons gated                       |

Public market data (orderbook, trade tape, ticker) is always accessible — no auth required.

---

## 6. Wallet Integration

- **Provider**: EIP-1193 (`window.ethereum`) — MetaMask and compatible
- **Connection**: `eth_requestAccounts` prompt
- **Signing**: `personal_sign` prompt
- **Account ID**: Derived via SHA-256 hash of lowercased address, formatted as UUID

### Account ID Derivation

```typescript
SHA-256(address.toLowerCase()) → first 16 bytes → UUID v4 format
```

Fallback for non-browser environments: FNV-1a hash (used in tests).

---

## 7. Production Assumptions

> [!IMPORTANT]
> These assumptions are **frozen** at v1.0. Changing them is a breaking change.

1. **One active session per tab** — sessionStorage is tab-scoped.
2. **Frontend does NOT verify the ECDSA signature** — verification is the backend's responsibility. Frontend stores the session to track UI state and pass `signature` as Bearer token on API calls.
3. **Session invalidation is always optimistic** — any wallet event (account/chain change) immediately clears auth state without a backend call.
4. **Nonce is single-use per sign-in** — a new 32-byte nonce is generated for every `signIn()` call.
5. **No concurrent sign-in** — `authStatus === "signing"` prevents a second sign-in flow from starting.

---

## 8. What Must Not Change Post-Launch

- `AuthSession` field names and types
- Session storage key: `dex_auth_session_v1`
- Sign-in message format (deterministic)
- SessionTTL: 24 hours
- Protected action list (see §5)
- Provider hierarchy: `WalletProvider` → `AuthProvider` → App
