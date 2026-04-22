# DEX Auth + Access Control Model

> Phase 19 reference document — describes the wallet-based authentication,
> session lifecycle, and access-control gating layer.

---

## Identity Layers

The DEX uses three distinct identity layers, each narrower than the last:

```
┌──────────────────────────────────────────────────────┐
│  1. Connected Wallet                                 │
│  ─ EIP-1193 provider detected, address known         │
│  ─ Can view market data, read balances               │
│  ─ Cannot trade, cancel, or withdraw                 │
│                                                      │
│  ┌──────────────────────────────────────────────────┐ │
│  │  2. Authenticated Session                       │ │
│  │  ─ personal_sign verified, session created       │ │
│  │  ─ Session stored in sessionStorage (tab-scoped) │ │
│  │  ─ 24-hour TTL, validated every 30 seconds       │ │
│  │                                                  │ │
│  │  ┌──────────────────────────────────────────────┐│ │
│  │  │  3. Protected Trading Access               ││ │
│  │  │  ─ Auth-gated actions: place, cancel,      ││ │
│  │  │    withdraw, admin operations              ││ │
│  │  │  ─ Rate-limited, serialized, double-checked││ │
│  │  └──────────────────────────────────────────────┘│ │
│  └──────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

---

## Session Lifecycle

### Creation

1. User connects wallet → `WalletProvider` stores `address` + `accountId`
2. User clicks "Sign In" → `AuthProvider.signIn()` is called
3. A cryptographic nonce (64-char hex) is generated via `WebCrypto.getRandomValues`
4. An EIP-4361-style message is built: `buildLoginMessage(address, nonce, issuedAt)`
5. The wallet signs via `personal_sign`
6. The nonce is consumed (replay prevention)
7. A session object is created and persisted to `sessionStorage`

### Restoration

On mount, `AuthProvider` calls `loadSession()` which:
- Parses JSON from sessionStorage
- Applies structural validation (`isSessionStructurallyValid`)
- Verifies nonce format (64-char lowercase hex)
- Validates timestamps (issuedAt < expiresAt, both valid ISO 8601)
- Checks field presence (address, signature, accountId non-empty)

### Expiry

- Sessions have a **24-hour TTL** (`SESSION_TTL_MS = 86,400,000 ms`)
- `AuthProvider` polls every **30 seconds** for expiry
- On tab re-focus (`visibilitychange`), session is checked immediately
- Expired sessions transition to `authStatus = "expired"`

### Invalidation Triggers

| Trigger | Handler | Auth Status After |
|---------|---------|------------------|
| Session expired | Expiry poll / visibility check | `expired` |
| Wallet disconnected | `accountsChanged([])` | `disconnected` |
| Account changed | `accountsChanged([newAddr])` | `connected` |
| Chain changed | `chainChanged` | `connected` |
| User signs out | `signOut()` button | `connected` |
| Signature rejected | `personal_sign` throw | `rejected` |
| Tab closed | sessionStorage cleared | N/A (new tab) |

---

## Nonce / Replay Protection

- Nonces are 64-character lowercase hex strings from `WebCrypto.getRandomValues(32)`
- An in-memory `Set<string>` tracks consumed nonces within the page lifecycle
- `consumeNonce(nonce)` returns `false` if the nonce was already used
- The set is cleared on `clearSession()` (sign-out / disconnect)
- The nonce is embedded in the signed message, binding it to a specific auth attempt

---

## Protected Action Gating

### `useProtectedAction(actionName, fn, config?)`

A composable hook that wraps any sensitive async action with:

1. **Auth status check**: Blocks unless `authStatus === "authenticated"`
2. **Real-time session validity**: Calls `isSessionValid(session, address)` at execute-time
3. **Rate limiting**: Per-action token-bucket limiter (configurable capacity/refill)
4. **Serialization**: Prevents concurrent executions of the same action

### `AuthGate`

React component that conditionally renders children:
```tsx
<AuthGate fallback={<SignInPrompt />}>
  <OrderForm />
</AuthGate>
```

### UI Components Using Auth Gating

| Component | Gated Actions | Gate Mechanism |
|-----------|--------------|----------------|
| OrderEntry | Place order | `useProtectedAction` + `AuthGate` |
| OpenOrders | Cancel order | Disabled span when unauthenticated |
| AccountPanel | Withdraw | `disabled={!isAuthenticated}` |
| WithdrawModal | Submit withdrawal | `useProtectedAction` |

---

## REST Client Token Assertion

All `DexApiClient` methods call `assertToken(token)` before sending requests:

- **Dev mode**: All tokens allowed (preserves dev workflow)
- **Production mode**: Rejects empty strings and known dev fallbacks (`"dev-token"`, `"dev-token-123"`)
- Throws `AuthRequiredError` on failure

---

## Security Notes

> **Frontend auth is UX-only gating.** The Rust gateway independently validates
> all credentials (JWT, signatures, nonces) on every protected request.
> Frontend checks prevent unnecessary network traffic and provide immediate
> user feedback, but they are NOT a security boundary.

### Key Principles

1. **Rust core is authoritative** — no frontend-only auth bypass can affect state
2. **Session is tab-scoped** — `sessionStorage` clears on tab close
3. **Deterministic account ID** — derived via SHA-256 hash of lowercase address
4. **No secret storage** — signatures are passed per-request, not cached
5. **Graceful degradation** — storage failures result in `null` session, not errors

---

## Auth Status State Machine

```
                    ┌──────────────┐
                    │ disconnected │
                    └──────┬───────┘
                           │ connect()
                    ┌──────▼───────┐
                    │  connecting  │
                    └──────┬───────┘
                           │ success
                    ┌──────▼───────┐
         ┌──────────│  connected   │◄───────────┐
         │          └──────┬───────┘            │
         │                 │ signIn()           │
         │          ┌──────▼───────┐            │
         │          │   signing    │            │
         │          └──┬───────┬───┘            │
         │    rejected │       │ success        │
    ┌────▼────┐        │ ┌─────▼──────────┐     │
    │rejected │◄───────┘ │ authenticated  │     │
    └─────────┘          └──┬──────┬──────┘     │
                   expired  │      │ signOut()  │
              ┌─────────────┘      └────────────┘
              │
        ┌─────▼───┐
        │ expired  │
        └─────────┘
```
