// ---------------------------------------------------------------------------
// AuthStatusBadge.tsx — header widget showing full wallet + auth state
// ---------------------------------------------------------------------------

import React from "react";
import { useWallet } from "../wallet/WalletProvider";
import { useAuth } from "../auth/AuthProvider";
import type { AuthStatus } from "../auth/AuthProvider";

// ---- Status dot colours ---------------------------------------------------

const DOT_CLASS: Record<AuthStatus, string> = {
    disconnected: "bg-slate-600",
    connecting: "bg-yellow-400 animate-pulse",
    connected: "bg-amber-400",
    signing: "bg-yellow-400 animate-pulse",
    authenticated: "bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.8)]",
    expired: "bg-red-500",
    rejected: "bg-red-500",
};

// ---- Component -------------------------------------------------------------

export const AuthStatusBadge: React.FC = () => {
    const { address, isConnecting, connect } = useWallet();
    const { authStatus, signIn, signOut, error } = useAuth();

    // ---- Disconnected — show connect button ---------------------------------

    if (!address && !isConnecting) {
        return (
            <button
                id="wallet-connect-btn"
                onClick={() => connect().catch((err) => console.error("Wallet connect error:", err))}
                className="relative px-5 py-2 text-sm font-semibold text-white rounded-lg group overflow-hidden transition-all hover:scale-105"
            >
                <div className="absolute inset-0 bg-gradient-to-r from-indigo-500 to-purple-500 pointer-events-none rounded-lg" />
                <div className="absolute inset-0 opacity-0 group-hover:opacity-100 bg-gradient-to-r from-indigo-400 to-purple-400 transition-opacity pointer-events-none blur-md rounded-lg" />
                <span className="relative z-10">Connect Wallet</span>
            </button>
        );
    }

    // ---- Connecting -----------------------------------------------------------

    if (isConnecting || authStatus === "connecting") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-indigo-500/30">
                <div className="w-2 h-2 rounded-full bg-yellow-400 animate-pulse" />
                <span className="text-sm text-slate-400">Connecting…</span>
            </div>
        );
    }

    // ---- Address pill (common to all connected states) -----------------------

    const short = address ? `${address.slice(0, 6)}…${address.slice(-4)}` : "";

    // ---- Signing --------------------------------------------------------------

    if (authStatus === "signing") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-yellow-500/30">
                <div className={`w-2 h-2 rounded-full ${DOT_CLASS.signing}`} />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <span className="text-xs text-yellow-400">Awaiting signature…</span>
            </div>
        );
    }

    // ---- Authenticated --------------------------------------------------------

    if (authStatus === "authenticated") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-green-500/30">
                <div className={`w-2 h-2 rounded-full ${DOT_CLASS.authenticated}`} />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
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
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-red-500/30">
                <div className={`w-2 h-2 rounded-full ${DOT_CLASS.expired}`} />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <button
                    id="wallet-sign-in-btn"
                    onClick={() => signIn().catch(() => { })}
                    className="text-xs text-amber-400 hover:text-amber-300 transition-colors ml-1"
                >
                    Session expired — Sign In
                </button>
            </div>
        );
    }

    // ---- Rejected -------------------------------------------------------------

    if (authStatus === "rejected") {
        return (
            <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-red-500/30">
                <div className={`w-2 h-2 rounded-full ${DOT_CLASS.rejected}`} />
                <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
                <button
                    id="wallet-sign-in-btn"
                    onClick={() => signIn().catch(() => { })}
                    className="text-xs text-rose-400 hover:text-rose-300 transition-colors ml-1"
                    title={error ?? "Signature rejected"}
                >
                    Rejected — Retry
                </button>
            </div>
        );
    }

    // ---- Connected but not authenticated (default "connected" state) -----------

    return (
        <div className="flex items-center gap-3 glass-panel px-3 py-1.5 rounded-full border border-amber-500/30">
            <div className={`w-2 h-2 rounded-full ${DOT_CLASS.connected}`} />
            <span className="text-sm font-mono text-slate-200" title={address ?? ""}>{short}</span>
            <button
                id="wallet-sign-in-btn"
                onClick={() => signIn().catch(() => { })}
                className="text-xs text-amber-400 hover:text-amber-300 transition-colors font-semibold ml-1"
            >
                Sign In
            </button>
        </div>
    );
};
