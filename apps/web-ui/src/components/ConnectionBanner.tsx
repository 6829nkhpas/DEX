// ---------------------------------------------------------------------------
// ConnectionBanner.tsx — slim full-width banner for WebSocket connection state
// ---------------------------------------------------------------------------
//
// Renders only when the WS connection is "disconnected" or "connecting".
// Positioned to sit just below the fixed 64px header (top-16).
// Disappears automatically when connected.
// ---------------------------------------------------------------------------

import React from "react";
import { useDexStore } from "../state/StoreProvider";

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
                fixed top-16 left-0 right-0 z-30 flex items-center justify-center gap-2
                px-4 py-2 text-xs font-semibold tracking-wide
                transition-all duration-300 ease-in-out
                ${isConnecting
                    ? "bg-amber-500/15 border-b border-amber-500/30 text-amber-400"
                    : "bg-rose-500/15 border-b border-rose-500/30 text-rose-400"
                }
            `}
        >
            {isConnecting ? (
                <>
                    <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse shrink-0" />
                    Connecting to market data…
                </>
            ) : (
                <>
                    <svg className="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                            d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                    </svg>
                    Market data disconnected — market data may be stale
                    <button
                        className="ml-2 underline underline-offset-2 hover:text-rose-300 transition-colors"
                        onClick={() => window.location.reload()}
                    >
                        Reload
                    </button>
                </>
            )}
        </div>
    );
};
