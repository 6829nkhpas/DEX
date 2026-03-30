// ---------------------------------------------------------------------------
// WalletProvider — MetaMask / EIP-1193 wallet integration
// ---------------------------------------------------------------------------
//
// Exposes:
//   address        — connected wallet address (null if disconnected)
//   accountId      — deterministic account_id derived from wallet address
//   connect()      — request MetaMask / EIP-1193 connection
//   disconnect()   — clear wallet state (client-side only)
//   signMessage()  — request a personal_sign from the wallet
//   isConnecting   — true while connection is in-flight
//
// Account ID derivation:
//   We derive a deterministic UUIDv7-like account_id from the wallet
//   address using a simple SHA-256 hash formatted as a UUID string.
//   In production this would be replaced by a backend-provided mapping.
// ---------------------------------------------------------------------------

import React, {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useMemo,
} from "react";

// ---------------------------------------------------------------------------
// EIP-1193 provider type (minimal)
// ---------------------------------------------------------------------------

interface EIP1193Provider {
  request(args: { method: string; params?: unknown[] }): Promise<unknown>;
  on(event: string, handler: (...args: unknown[]) => void): void;
  removeListener(event: string, handler: (...args: unknown[]) => void): void;
}

declare global {
  interface Window {
    ethereum?: EIP1193Provider;
  }
}

// ---------------------------------------------------------------------------
// Context type
// ---------------------------------------------------------------------------

export interface WalletContextValue {
  /** Connected wallet address (checksummed), or null. */
  address: string | null;
  /** Deterministic account_id derived from address, or null. */
  accountId: string | null;
  /** True while a connect() call is in-flight. */
  isConnecting: boolean;
  /**
   * True when the wallet is recovering from a transient disconnect
   * (e.g. chain switch or provider reload) but was previously connected.
   * Distinct from isConnecting which only covers initial connect().
   */
  isReconnecting: boolean;
  /**
   * Last wallet-layer error message, or null.
   * Set on connection failure, no-provider, or sign rejection at the
   * provider level (not auth-layer rejection).
   */
  connectionError: string | null;
  /** Request wallet connection (MetaMask popup). */
  connect: () => Promise<void>;
  /** Disconnect (client-side clear). */
  disconnect: () => void;
  /** Sign an arbitrary message via personal_sign. Throws if another sign is already in-flight. */
  signMessage: (message: string) => Promise<string>;
}

const WalletContext = createContext<WalletContextValue | null>(null);

// ---------------------------------------------------------------------------
// Deterministic account ID derivation
// ---------------------------------------------------------------------------

/**
 * Derive a deterministic UUID-shaped account_id from a wallet address.
 *
 * Uses Web Crypto SHA-256, falling back to a simple hash when crypto is
 * unavailable (e.g. in tests).
 */
export async function deriveAccountId(address: string): Promise<string> {
  const normalized = address.toLowerCase();

  if (typeof globalThis.crypto !== "undefined" && globalThis.crypto.subtle) {
    const data = new TextEncoder().encode(normalized);
    const hashBuf = await crypto.subtle.digest("SHA-256", data);
    const bytes = new Uint8Array(hashBuf);
    return formatAsUuid(bytes);
  }

  // Fallback: simple deterministic hash for test / non-browser environments
  return fallbackDeriveId(normalized);
}

/** Format the first 16 bytes of a Uint8Array as a UUID v4-style string. */
function formatAsUuid(bytes: Uint8Array): string {
  const hex = Array.from(bytes.slice(0, 16))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  // Format: 8-4-4-4-12
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join("-");
}

/** Deterministic fallback hash for environments without Web Crypto. */
function fallbackDeriveId(input: string): string {
  let h = 0x811c9dc5; // FNV offset basis
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 0x01000193); // FNV prime
  }
  const hex = (h >>> 0).toString(16).padStart(8, "0");
  // Build a UUID-shaped string by repeating the hash
  const full = (hex + hex + hex + hex).slice(0, 32);
  return [
    full.slice(0, 8),
    full.slice(8, 12),
    full.slice(12, 16),
    full.slice(16, 20),
    full.slice(20, 32),
  ].join("-");
}

// ---------------------------------------------------------------------------
// Provider component
// ---------------------------------------------------------------------------

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [address, setAddress] = useState<string | null>(null);
  const [accountId, setAccountId] = useState<string | null>(null);
  const [isConnecting, setIsConnecting] = useState(false);
  const [isReconnecting, setIsReconnecting] = useState(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  // In-flight guard: prevents parallel personal_sign calls.
  // If a sign is already in-flight, a second call will throw.
  const signingInFlightRef = React.useRef(false);

  // Derive account ID whenever address changes
  useEffect(() => {
    if (!address) {
      setAccountId(null);
      return;
    }
    let cancelled = false;
    deriveAccountId(address).then((id) => {
      if (!cancelled) setAccountId(id);
    });
    return () => {
      cancelled = true;
    };
  }, [address]);

  // Listen for MetaMask account / chain changes
  useEffect(() => {
    const provider = window.ethereum;
    if (!provider) return;

    const handleAccountsChanged = (accounts: unknown) => {
      const accs = accounts as string[];
      if (accs.length === 0) {
        // Account disconnected from wallet side
        setAddress(null);
        setConnectionError(null);
      } else if (accs[0] !== address) {
        // Account changed — brief reconnecting state
        setIsReconnecting(true);
        setAddress(accs[0]);
        setConnectionError(null);
        // Reconnecting clears after accountId is derived (see useEffect below)
      }
    };

    provider.on("accountsChanged", handleAccountsChanged);
    return () => {
      provider.removeListener("accountsChanged", handleAccountsChanged);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [address]);

  // Clear isReconnecting once accountId is ready after an account switch
  useEffect(() => {
    if (isReconnecting && accountId !== null) {
      setIsReconnecting(false);
    }
  }, [isReconnecting, accountId]);

  const connect = useCallback(async () => {
    // Prevent double-connect while already connecting
    if (isConnecting) return;

    const provider = window.ethereum;
    if (!provider) {
      const msg = "No EIP-1193 wallet detected. Please install MetaMask.";
      setConnectionError(msg);
      throw new Error(msg);
    }
    setIsConnecting(true);
    setConnectionError(null);
    try {
      const accounts = (await provider.request({
        method: "eth_requestAccounts",
      })) as string[];
      if (accounts.length > 0) {
        setAddress(accounts[0]);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setConnectionError(msg);
      throw err;
    } finally {
      setIsConnecting(false);
    }
  }, [isConnecting]);

  const disconnect = useCallback(() => {
    setAddress(null);
    setAccountId(null);
    setConnectionError(null);
    setIsReconnecting(false);
    // Reset in-flight sign guard
    signingInFlightRef.current = false;
  }, []);

  const signMessage = useCallback(
    async (message: string): Promise<string> => {
      const provider = window.ethereum;
      if (!provider || !address) {
        throw new Error("Wallet not connected");
      }
      // In-flight guard: prevent concurrent personal_sign calls
      if (signingInFlightRef.current) {
        throw new Error(
          "A signature request is already in progress. Please complete or reject it in your wallet.",
        );
      }
      signingInFlightRef.current = true;
      try {
        const signature = (await provider.request({
          method: "personal_sign",
          params: [message, address],
        })) as string;
        return signature;
      } finally {
        signingInFlightRef.current = false;
      }
    },
    [address],
  );

  const value = useMemo<WalletContextValue>(
    () => ({
      address,
      accountId,
      isConnecting,
      isReconnecting,
      connectionError,
      connect,
      disconnect,
      signMessage,
    }),
    [address, accountId, isConnecting, isReconnecting, connectionError, connect, disconnect, signMessage],
  );

  return (
    <WalletContext.Provider value={value}>
      {children}
    </WalletContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useWallet(): WalletContextValue {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used within a <WalletProvider>");
  }
  return ctx;
}
