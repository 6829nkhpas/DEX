// ---------------------------------------------------------------------------
// ConnectionBanner.tsx — slim full-width banner for WebSocket connection state
// ---------------------------------------------------------------------------
//
// Phase 15: improved with slide-down animation, clearer messaging,
//           reconnection context, and StatusIndicator usage.
// ---------------------------------------------------------------------------

import React from "react";
import { useDexStore } from "../state/StoreProvider";
import { StatusIndicator } from "./ui/StatusIndicator";

export const ConnectionBanner: React.FC = () => {
    const { connectionStatus } = useDexStore();

    if (connectionStatus === "connected") return null;

    const isConnecting = connectionStatus === "connecting";

    return (
        <div
            id="connection-banner"
            role="status"
            aria-live="polite"
            className={`
                fixed top-16 left-0 right-0 z-30 flex items-center justify-center gap-2.5
                px-4 py-2 text-xs font-semibold tracking-wide
                animate-slide-down
                ${isConnecting
                    ? "bg-amber-500/10 border-b border-amber-500/25 text-amber-400"
                    : "bg-rose-500/10 border-b border-rose-500/25 text-rose-400"
                }
            `}
        >
            {isConnecting ? (
                <>
                    <StatusIndicator status="loading" pulse size="sm" />
                    <span>Reconnecting to market data feed…</span>
                </>
            ) : (
                <>
                    <StatusIndicator status="error" size="sm" />
                    <span>Market data feed disconnected — prices may be stale</span>
                    <button
                        className="ml-2 px-2.5 py-0.5 rounded-md bg-rose-500/15 border border-rose-500/25 hover:bg-rose-500 hover:text-white transition-all text-[11px] font-bold"
                        onClick={() => window.location.reload()}
                    >
                        Reload
                    </button>
                </>
            )}
        </div>
    );
};
