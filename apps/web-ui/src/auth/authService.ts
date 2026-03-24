// ---------------------------------------------------------------------------
// authService.ts — pure auth/session logic (no React dependencies)
// ---------------------------------------------------------------------------
//
// Responsibilities:
//   - Generate cryptographic nonces for sign-in challenges
//   - Build the deterministic human-readable sign-in message
//   - Create, validate, persist, load, and clear AuthSession objects
//
// Session storage: uses sessionStorage so sessions are scoped to a tab and
// automatically discarded when the tab closes. Max TTL is 24 hours.
//
// Security notes:
//   - Nonces are one-time-use; a new nonce is generated for every sign-in.
//   - Sessions carry the nonce so the backend can reject replays.
//   - The frontend never grants access based solely on its own auth check;
//     the session token must also be verified server-side on sensitive calls.
// ---------------------------------------------------------------------------

export interface AuthSession {
  /** Checksummed (original case) wallet address. */
  address: string;
  /** hex-encoded ECDSA signature from personal_sign. */
  signature: string;
  /** 64-char hex nonce — prevents replay within the session lifetime. */
  nonce: string;
  /** ISO 8601 timestamp when the session was created. */
  issuedAt: string;
  /** ISO 8601 timestamp when the session expires (issuedAt + 24 h). */
  expiresAt: string;
  /** Derived account ID (UUID-shaped, from WalletProvider.deriveAccountId). */
  accountId: string;
}

// ---------------------------------------------------------------------------
// Session TTL
// ---------------------------------------------------------------------------

const SESSION_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours
const STORAGE_KEY = "dex_auth_session_v1";

// ---------------------------------------------------------------------------
// Nonce generation
// ---------------------------------------------------------------------------

/**
 * Generate a cryptographically random 32-byte (64 hex char) nonce.
 * Falls back to Math.random-based generation in environments without WebCrypto.
 */
export function generateNonce(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.getRandomValues === "function"
  ) {
    const bytes = new Uint8Array(32);
    globalThis.crypto.getRandomValues(bytes);
    return Array.from(bytes)
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }
  // Fallback for test / non-browser environments
  let hex = "";
  for (let i = 0; i < 64; i++) {
    hex += Math.floor(Math.random() * 16).toString(16);
  }
  return hex;
}

// ---------------------------------------------------------------------------
// Sign-in message builder
// ---------------------------------------------------------------------------

/**
 * Build the deterministic, human-readable EIP-4361-style message the user
 * must sign to authenticate. Same inputs always produce the same output.
 */
export function buildLoginMessage(
  address: string,
  nonce: string,
  issuedAt: string,
): string {
  return [
    "DEX Authentication Request",
    "",
    `Address: ${address}`,
    `Nonce: ${nonce}`,
    `Issued At: ${issuedAt}`,
    "",
    "By signing this message you confirm you own this wallet.",
    "This request does not cost gas or send any transaction.",
  ].join("\n");
}

// ---------------------------------------------------------------------------
// Session creation
// ---------------------------------------------------------------------------

/**
 * Create an AuthSession from the completed sign-in data.
 * Does NOT perform signature cryptographic verification — that is the
 * responsibility of the backend. The frontend stores the session to track
 * the authenticated state and pass the token on API calls.
 */
export function createSession(
  address: string,
  signature: string,
  nonce: string,
  issuedAt: string,
  accountId: string,
): AuthSession {
  const issuedMs = new Date(issuedAt).getTime();
  const expiresAt = new Date(issuedMs + SESSION_TTL_MS).toISOString();

  return {
    address,
    signature,
    nonce,
    issuedAt,
    expiresAt,
    accountId,
  };
}

// ---------------------------------------------------------------------------
// Session validation
// ---------------------------------------------------------------------------

/**
 * Returns true if the session is valid:
 *   1. Not expired (current time < expiresAt)
 *   2. Address matches the currently connected wallet (case-insensitive)
 */
export function isSessionValid(
  session: AuthSession,
  currentAddress: string,
): boolean {
  const now = Date.now();
  const expiresMs = new Date(session.expiresAt).getTime();
  if (now >= expiresMs) return false;
  if (session.address.toLowerCase() !== currentAddress.toLowerCase()) return false;
  return true;
}

// ---------------------------------------------------------------------------
// SessionStorage persistence
// ---------------------------------------------------------------------------

/**
 * Persist a session to sessionStorage. Silently ignores storage errors
 * (e.g. private browsing with storage disabled).
 */
export function persistSession(session: AuthSession): void {
  try {
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(session));
  } catch {
    // Storage unavailable — session is in-memory only for this page load.
  }
}

/**
 * Load a previously persisted session from sessionStorage.
 * Returns null if no session exists, the data is corrupt, or any other error.
 */
export function loadSession(): AuthSession | null {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<AuthSession>;
    // Minimal shape guard
    if (
      typeof parsed.address === "string" &&
      typeof parsed.signature === "string" &&
      typeof parsed.nonce === "string" &&
      typeof parsed.issuedAt === "string" &&
      typeof parsed.expiresAt === "string" &&
      typeof parsed.accountId === "string"
    ) {
      return parsed as AuthSession;
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Remove the session from sessionStorage. Idempotent.
 */
export function clearSession(): void {
  try {
    sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore
  }
}
