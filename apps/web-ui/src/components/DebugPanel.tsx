// ---------------------------------------------------------------------------
// DebugPanel — developer diagnostic overlay
// ---------------------------------------------------------------------------
// Phase 15: added WASM status, auth session, wallet connection sections.
// ---------------------------------------------------------------------------

import React, { useMemo, useState } from "react";
import { useDexStore, useAppSelector } from "../state/StoreProvider";
import { useAuth } from "../auth/AuthProvider";
import { useWallet } from "../wallet/WalletProvider";
import { Terminal, X } from "lucide-react";
import { StatusIndicator } from "./ui/StatusIndicator";
import type { StatusType } from "./ui/StatusIndicator";

export function DebugPanel() {
    const { connectionStatus } = useDexStore();
    const state = useAppSelector(state => state);
    const { authStatus, session } = useAuth();
    const { address, accountId, isReconnecting } = useWallet();
    const { metrics } = state;
    const [isExpanded, setIsExpanded] = useState(false);

    // Infer current symbol from first active orderbook
    const currentSymbol = useMemo(() => {
        if (state.orderbooks.size > 0) {
            return Array.from(state.orderbooks.keys())[0];
        }
        return "None";
    }, [state.orderbooks]);

    const lastSequence = useMemo(() => {
        if (currentSymbol !== "None") {
            const ob = state.orderbooks.get(currentSymbol);
            return ob?.lastSeq ?? "0";
        }
        return "0";
    }, [state.orderbooks, currentSymbol]);

    const wsStatus: StatusType = connectionStatus === "connected" ? "connected" : connectionStatus === "error" ? "error" : "loading";

    if (!isExpanded) {
        return (
            <button
                onClick={() => setIsExpanded(true)}
                className="fixed bottom-6 right-6 p-3 rounded-full bg-slate-800/80 border border-slate-700/50 text-indigo-400 hover:bg-slate-700 shadow-xl transition-all hover:scale-105 z-50 glass-panel group"
                title="Open Debug Panel"
            >
                <Terminal className="w-5 h-5 group-hover:text-indigo-300 transition-colors" />
            </button>
        );
    }

    return (
        <div className="fixed bottom-6 right-6 w-96 bg-gray-900 border border-gray-700 text-xs text-green-400 p-4 rounded-xl shadow-[0_0_40px_rgba(0,0,0,0.5)] font-mono flex flex-col gap-3 z-50 animate-fade-in glass-panel-heavy max-h-[70vh] overflow-y-auto custom-scrollbar">
            <div className="flex items-center justify-between border-b border-gray-700 pb-2">
                <div className="flex items-center gap-2 text-white font-bold uppercase tracking-wider text-[10px]">
                    <Terminal className="w-4 h-4 text-indigo-400" />
                    Debug Panel
                </div>
                <button
                    onClick={() => setIsExpanded(false)}
                    className="text-gray-500 hover:text-white transition-colors p-1"
                >
                    <X className="w-4 h-4" />
                </button>
            </div>

            {/* Connection status */}
            <DebugSection title="Connection">
                <DebugRow label="WebSocket">
                    <StatusIndicator status={wsStatus} label={connectionStatus.toUpperCase()} size="sm" />
                </DebugRow>
            </DebugSection>

            {/* Wallet / Auth */}
            <DebugSection title="Wallet & Auth">
                <DebugRow label="Wallet">
                    <span className="text-slate-300 font-mono">
                        {address ? `${address.slice(0, 8)}…${address.slice(-4)}` : "Not connected"}
                    </span>
                </DebugRow>
                <DebugRow label="Account ID">
                    <span className="text-slate-300 font-mono">
                        {accountId ? `${accountId.slice(0, 12)}…` : "—"}
                    </span>
                </DebugRow>
                <DebugRow label="Auth Status">
                    <StatusIndicator
                        status={authStatus === "authenticated" ? "connected" : authStatus === "signing" ? "loading" : authStatus === "expired" || authStatus === "rejected" ? "error" : authStatus === "connected" ? "warning" : "disconnected"}
                        label={authStatus.toUpperCase()}
                        size="sm"
                    />
                </DebugRow>
                {isReconnecting && (
                    <DebugRow label="Reconnecting">
                        <StatusIndicator status="loading" label="YES" pulse size="sm" />
                    </DebugRow>
                )}
                {session && (
                    <DebugRow label="Session Issued">
                        <span className="text-slate-400">{session.issuedAt}</span>
                    </DebugRow>
                )}
            </DebugSection>

            {/* WASM Status */}
            <DebugSection title="WASM Compute">
                <DebugRow label="Execution Path">
                    <span className="status-badge status-badge-neutral">Native (server-side)</span>
                </DebugRow>
                <DebugRow label="WASM Available">
                    <span className="text-slate-400">Not loaded</span>
                </DebugRow>
            </DebugSection>

            {/* Stream Context */}
            <DebugSection title="Stream Context">
                <DebugRow label="Current Symbol">
                    <span className="text-white">{currentSymbol}</span>
                </DebugRow>
                <DebugRow label="Last Sequence">
                    <span className="text-blue-400">{lastSequence}</span>
                </DebugRow>
            </DebugSection>

            {/* Store Metrics */}
            <DebugSection title="Store Metrics">
                <DebugRow label="Ignored (Dupes)">
                    <span>{metrics.events_ignored}</span>
                </DebugRow>
                <DebugRow label="Gaps Detected">
                    <span className={metrics.gaps_detected > 0 ? "text-red-500 text-glow-sell" : ""}>
                        {metrics.gaps_detected}
                    </span>
                </DebugRow>
            </DebugSection>

            {/* Delta Buffers */}
            <DebugSection title="Delta Buffers">
                {Array.from(metrics.buffer_size_by_stream.entries()).length === 0 ? (
                    <span className="text-gray-500 italic text-[10px]">No buffered items</span>
                ) : (
                    Array.from(metrics.buffer_size_by_stream.entries()).map(([stream, size]) => (
                        <DebugRow key={stream} label={stream}>
                            <span className="text-white">{size}</span>
                        </DebugRow>
                    ))
                )}
            </DebugSection>
        </div>
    );
}

// ---- Helper components -------------------------------------------------------

const DebugSection: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
    <div>
        <h3 className="text-white font-bold mb-1.5 uppercase text-[10px] tracking-widest opacity-60">{title}</h3>
        <div className="flex flex-col gap-1">{children}</div>
    </div>
);

const DebugRow: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
    <div className="flex justify-between items-center">
        <span className="text-slate-400">{label}:</span>
        {children}
    </div>
);
