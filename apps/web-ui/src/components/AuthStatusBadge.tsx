// ---------------------------------------------------------------------------
// AuthStatusBadge.tsx — header widget showing full wallet + auth state
// ---------------------------------------------------------------------------
// Phase 15: improved with distinct icons per state, session expiry countdown,
//           tooltip on address hover, transition animations, and StatusIndicator.
// ---------------------------------------------------------------------------

import React, { useMemo, useState, useEffect } from "react";
import { useWallet } from "../wallet/WalletProvider";
import { useAuth } from "../auth/AuthProvider";
import type { AuthStatus } from "../auth/AuthProvider";
import { StatusIndicator } from "./ui/StatusIndicator";
import type { StatusType } from "./ui/StatusIndicator";

// ---- Map AuthStatus → StatusIndicator status type -------------------------

const AUTH_TO_STATUS: Record<AuthStatus, StatusType> = {
    disconnected: "disconnected",
    connecting: "loading",
    connected: "warning",
    signing: "loading",
    authenticated: "connected",
    expired: "error",
    rejected: "error",
};

// ---- Session countdown hook -----------------------------------------------

function useSessionCountdown(expiresAt: number | null): string | null {
    const [remaining, setRemaining] = useState<string | null>(null);

    useEffect(() => {
        if (!expiresAt) {
            setRemaining(null);
            return;
        }

        const update = () => {
            const diff = expiresAt - Date.now();
            if (diff <= 0) {
                setRemaining(null);
                return;
            }
            const hrs = Math.floor(diff / 3_600_000);
            const mins = Math.floor((diff % 3_600_000) / 60_000);
            if (hrs > 0) {
                setRemaining(`${hrs}h ${mins}m`);
            } else {
                setRemaining(`${mins}m`);
            }
        };

        update();
        const timer = setInterval(update, 60_000);
        return () => clearInterval(timer);
    }, [expiresAt]);

    return remaining;
}

// ---- Component -------------------------------------------------------------

export const AuthStatusBadge: React.FC = () => {
    const { address, isConnecting, connect, connectionError } = useWallet();
    const { authStatus, session, signIn, signOut, error } = useAuth();

    const short = useMemo(
        () => (address ? `${address.slice(0, 6)}…${address.slice(-4)}` : ""),
        [address],
    );

    // Session expiry countdown (24h sessions)
    const expiresAt = useMemo(() => {
        if (!session) return null;
        // Session issued_at + 24 hours
        const issued = new Date(session.issuedAt).getTime();
        return issued + 24 * 60 * 60 * 1000;
    }, [session]);
    const countdown = useSessionCountdown(
        authStatus === "authenticated" ? expiresAt : null,
    );

    // ---- Disconnected — show connect button ---------------------------------

    if (!address && !isConnecting) {
        return (
            <div className="flex items-center gap-2 animate-fade-in">
                {connectionError && (
                    <span className="text-[10px] text-red-400 font-medium max-w-[150px] truncate" title={connectionError}>
                        {connectionError}
                    </span>
                )}
                <button
                    id="wallet-connect-btn"
                    onClick={() => connect().catch((err) => console.error("Wallet connect error:", err))}
                    className="relative px-5 py-2 text-sm font-semibold text-white rounded-lg group overflow-hidden transition-all hover:scale-105"
                >
                    <div className="absolute inset-0 bg-gradient-to-r from-indigo-500 to-purple-500 pointer-events-none rounded-lg" />
                    <div className="absolute inset-0 opacity-0 group-hover:opacity-100 bg-gradient-to-r from-indigo-400 to-purple-400 transition-opacity pointer-events-none blur-md rounded-lg" />
                    <span className="relative z-10 flex items-center gap-2">
                        <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                                d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
                        </svg>
                        Connect Wallet
                    </span>
                </button>
            </div>
        );
    }

    // ---- Connecting -----------------------------------------------------------

    if (isConnecting || authStatus === "connecting") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-indigo-500/30 animate-fade-in">
                <StatusIndicator status="loading" pulse />
                <span className="text-sm text-slate-400">Connecting…</span>
            </div>
        );
    }

    // ---- All connected states share the address pill -------------------------

    const statusType = AUTH_TO_STATUS[authStatus];

    // ---- Signing --------------------------------------------------------------

    if (authStatus === "signing") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-yellow-500/30 animate-fade-in">
                <StatusIndicator status="loading" pulse />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <span className="text-xs text-yellow-400 flex items-center gap-1">
                    <svg className="w-3.5 h-3.5 animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                            d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                    </svg>
                    Awaiting signature…
                </span>
            </div>
        );
    }

    // ---- Authenticated --------------------------------------------------------

    if (authStatus === "authenticated") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-green-500/30 animate-fade-in">
                <StatusIndicator status="connected" />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                {countdown && (
                    <span className="text-[10px] text-slate-500 font-medium" title="Session expires in">
                        {countdown}
                    </span>
                )}
                <button
                    id="wallet-sign-out-btn"
                    onClick={signOut}
                    className="text-xs text-slate-400 hover:text-white transition-colors ml-1"
                >
                    Sign Out
                </button>
            </div>
        );
    }

    // ---- Expired --------------------------------------------------------------

    if (authStatus === "expired") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-red-500/30 animate-fade-in">
                <StatusIndicator status="error" />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <button
                    id="wallet-sign-in-btn"
                    onClick={() => signIn().catch(() => { })}
                    className="text-xs text-amber-400 hover:text-amber-300 transition-colors ml-1 flex items-center gap-1"
                >
                    <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    Session expired — Sign In
                </button>
            </div>
        );
    }

    // ---- Rejected -------------------------------------------------------------

    if (authStatus === "rejected") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-red-500/30 animate-fade-in">
                <StatusIndicator status="error" />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <button
                    id="wallet-sign-in-btn"
                    onClick={() => signIn().catch(() => { })}
                    className="text-xs text-rose-400 hover:text-rose-300 transition-colors ml-1 flex items-center gap-1"
                    title={error ?? "Signature rejected"}
                >
                    <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                            d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                    </svg>
                    Rejected — Retry
                </button>
            </div>
        );
    }

    // ---- Connected but not authenticated (default "connected" state) -----------

    return (
        <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-amber-500/30 animate-fade-in">
            <StatusIndicator status={statusType} />
            <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
            <button
                id="wallet-sign-in-btn"
                onClick={() => signIn().catch(() => { })}
                className="text-xs text-amber-400 hover:text-amber-300 transition-colors font-semibold ml-1 flex items-center gap-1"
            >
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                        d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                </svg>
                Sign In
            </button>
        </div>
    );
};
