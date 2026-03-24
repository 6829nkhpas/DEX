// ---------------------------------------------------------------------------
// AuthProvider.tsx — React context for wallet-based authentication
// ---------------------------------------------------------------------------
//
// Wraps the existing WalletProvider (must be a child of it). Adds:
//   - Signed-message login flow with nonce/challenge
//   - Session persistence via sessionStorage (24h TTL)
//   - Auto-invalidation on address change or chain change
//   - AuthStatus enum for clear UI state mapping
//
// Public API:
//   authStatus  — current auth state (see AuthStatus)
//   session     — AuthSession object (null if not authenticated)
//   signIn()    — begins the sign-in flow
//   signOut()   — clears auth state and session
//   error       — last error message (null if none)
// ---------------------------------------------------------------------------

import React, {
    createContext,
    useContext,
    useState,
    useCallback,
    useEffect,
    useRef,
    useMemo,
} from "react";
import { useWallet } from "../wallet/WalletProvider";
import {
    generateNonce,
    buildLoginMessage,
    createSession,
    isSessionValid,
    persistSession,
    loadSession,
    clearSession,
    type AuthSession,
} from "./authService";

// ---------------------------------------------------------------------------
// AuthStatus — all possible states the auth layer can be in
// ---------------------------------------------------------------------------

export type AuthStatus =
    | "disconnected"   // wallet not connected
    | "connecting"     // wallet connection in-flight (mirrors WalletProvider)
    | "connected"      // wallet connected, NOT yet authenticated
    | "signing"        // waiting for user to sign the challenge message
    | "authenticated"  // session active and valid
    | "expired"        // session exists in storage but has expired
    | "rejected";      // user rejected the signature request in their wallet

// ---------------------------------------------------------------------------
// Context type
// ---------------------------------------------------------------------------

export interface AuthContextValue {
    /** Current authentication status. */
    authStatus: AuthStatus;
    /** Active session (non-null only when authStatus === "authenticated"). */
    session: AuthSession | null;
    /** Initiate the sign-in flow. Resolves when sign-in completes or throws. */
    signIn: () => Promise<void>;
    /** Sign out — clears session and resets to "connected" (or "disconnected"). */
    signOut: () => void;
    /** Most recent error message, or null. Cleared on each new signIn() call. */
    error: string | null;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function AuthProvider({ children }: { children: React.ReactNode }) {
    const { address, accountId, isConnecting, signMessage, disconnect } = useWallet();

    const [authStatus, setAuthStatus] = useState<AuthStatus>("disconnected");
    const [session, setSession] = useState<AuthSession | null>(null);
    const [error, setError] = useState<string | null>(null);

    // Track the address that our current session was signed for.
    // Used to detect account changes.
    const sessionAddressRef = useRef<string | null>(null);

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    const doSignOut = useCallback(
        (nextStatus: AuthStatus) => {
            clearSession();
            setSession(null);
            sessionAddressRef.current = null;
            setAuthStatus(nextStatus);
        },
        [],
    );

    // -------------------------------------------------------------------------
    // On mount: restore session from sessionStorage
    // -------------------------------------------------------------------------

    useEffect(() => {
        // Wait until we know whether a wallet is connected
        if (isConnecting) return;

        if (!address) {
            setAuthStatus("disconnected");
            return;
        }

        // Try to restore a persisted session
        const stored = loadSession();
        if (stored && isSessionValid(stored, address)) {
            setSession(stored);
            sessionAddressRef.current = stored.address;
            setAuthStatus("authenticated");
        } else if (stored && !isSessionValid(stored, address)) {
            // Session exists but is expired or for a different address
            clearSession();
            const isExpired = stored.address.toLowerCase() === address.toLowerCase();
            setAuthStatus(isExpired ? "expired" : "connected");
        } else {
            setAuthStatus("connected");
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []); // Only on mount

    // -------------------------------------------------------------------------
    // React to wallet address changes
    // -------------------------------------------------------------------------

    useEffect(() => {
        if (isConnecting) {
            setAuthStatus("connecting");
            return;
        }

        if (!address) {
            // Wallet disconnected
            doSignOut("disconnected");
            return;
        }

        // Address is present. If it changed from what our session was signed for,
        // invalidate auth.
        if (
            sessionAddressRef.current !== null &&
            sessionAddressRef.current.toLowerCase() !== address.toLowerCase()
        ) {
            doSignOut("connected");
            return;
        }

        // If we had a session and address still matches, keep it.
        // Otherwise ensure we are at least "connected".
        if (authStatus === "disconnected") {
            setAuthStatus("connected");
        }
    }, [address, isConnecting, doSignOut, authStatus]);

    // -------------------------------------------------------------------------
    // Listen for EIP-1193 chainChanged — invalidate on chain switch
    // -------------------------------------------------------------------------

    useEffect(() => {
        const provider = (window as { ethereum?: { on?: (event: string, handler: () => void) => void; removeListener?: (event: string, handler: () => void) => void } }).ethereum;
        if (!provider?.on || !provider?.removeListener) return;

        const handleChainChanged = () => {
            // Chain change always invalidates auth — clear and stay at "connected"
            doSignOut(address ? "connected" : "disconnected");
        };

        provider.on("chainChanged", handleChainChanged);
        return () => {
            provider.removeListener?.("chainChanged", handleChainChanged);
        };
    }, [address, doSignOut]);

    // -------------------------------------------------------------------------
    // signIn — nonce → message → sign → create session
    // -------------------------------------------------------------------------

    const signIn = useCallback(async () => {
        if (!address || !accountId) {
            setError("Wallet not connected. Please connect your wallet first.");
            return;
        }

        setError(null);
        setAuthStatus("signing");

        try {
            const nonce = generateNonce();
            const issuedAt = new Date().toISOString();
            const message = buildLoginMessage(address, nonce, issuedAt);

            const signature = await signMessage(message);

            const newSession = createSession(
                address,
                signature,
                nonce,
                issuedAt,
                accountId,
            );

            persistSession(newSession);
            setSession(newSession);
            sessionAddressRef.current = address;
            setAuthStatus("authenticated");
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            // MetaMask rejection codes
            const isRejection =
                msg.includes("User denied") ||
                msg.includes("user rejected") ||
                msg.includes("4001");
            setError(isRejection ? "Signature request was rejected." : `Sign-in failed: ${msg}`);
            setAuthStatus(isRejection ? "rejected" : "connected");
        }
    }, [address, accountId, signMessage]);

    // -------------------------------------------------------------------------
    // signOut
    // -------------------------------------------------------------------------

    const signOut = useCallback(() => {
        doSignOut(address ? "connected" : "disconnected");
    }, [address, doSignOut]);

    // -------------------------------------------------------------------------
    // Context value
    // -------------------------------------------------------------------------

    const value = useMemo<AuthContextValue>(
        () => ({ authStatus, session, signIn, signOut, error }),
        [authStatus, session, signIn, signOut, error],
    );

    return (
        <AuthContext.Provider value={value}>
            {children}
        </AuthContext.Provider>
    );
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useAuth(): AuthContextValue {
    const ctx = useContext(AuthContext);
    if (!ctx) {
        throw new Error("useAuth must be used within an <AuthProvider>");
    }
    return ctx;
}
