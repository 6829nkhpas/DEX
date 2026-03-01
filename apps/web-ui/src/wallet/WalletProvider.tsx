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
  /** Request wallet connection (MetaMask popup). */
  connect: () => Promise<void>;
  /** Disconnect (client-side clear). */
  disconnect: () => void;
  /** Sign an arbitrary message via personal_sign. */
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
        setAddress(null);
      } else {
        setAddress(accs[0]);
      }
    };

    provider.on("accountsChanged", handleAccountsChanged);
    return () => {
      provider.removeListener("accountsChanged", handleAccountsChanged);
    };
  }, []);

  const connect = useCallback(async () => {
    const provider = window.ethereum;
    if (!provider) {
      throw new Error("No EIP-1193 wallet detected. Please install MetaMask.");
    }
    setIsConnecting(true);
    try {
      const accounts = (await provider.request({
        method: "eth_requestAccounts",
      })) as string[];
      if (accounts.length > 0) {
        setAddress(accounts[0]);
      }
    } finally {
      setIsConnecting(false);
    }
  }, []);

  const disconnect = useCallback(() => {
    setAddress(null);
    setAccountId(null);
  }, []);

  const signMessage = useCallback(
    async (message: string): Promise<string> => {
      const provider = window.ethereum;
      if (!provider || !address) {
        throw new Error("Wallet not connected");
      }
      const signature = (await provider.request({
        method: "personal_sign",
        params: [message, address],
      })) as string;
      return signature;
    },
    [address],
  );

  const value = useMemo<WalletContextValue>(
    () => ({
      address,
      accountId,
      isConnecting,
      connect,
      disconnect,
      signMessage,
    }),
    [address, accountId, isConnecting, connect, disconnect, signMessage],
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
