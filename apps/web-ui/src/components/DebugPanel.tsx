// apps/web-ui/src/components/DebugPanel.tsx
import React, { useMemo, useState } from "react";
import { useDexStore } from "../state/StoreProvider";
import { Terminal, X } from "lucide-react";

export function DebugPanel() {
    const { state, connectionStatus } = useDexStore();
    const { metrics } = state;
    const [isExpanded, setIsExpanded] = useState(false);

    // We can infer current symbol from the first active orderbook or ticker
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
        <div className="fixed bottom-6 right-6 w-96 bg-gray-900 border border-gray-700 text-xs text-green-400 p-4 rounded-xl shadow-[0_0_40px_rgba(0,0,0,0.5)] font-mono flex flex-col gap-4 z-50 animate-fade-in glass-panel-heavy">
            <div className="flex items-center justify-between border-b border-gray-700 pb-2 mb-1">
                <div className="flex items-center gap-2 text-white font-bold uppercase tracking-wider">
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

            <div>
                <h3 className="text-white font-bold mb-2 uppercase text-[10px] tracking-widest opacity-60">Connection</h3>
                <div className="flex justify-between">
                    <span>Status:</span>
                    <span className={connectionStatus === "connected" ? "text-green-500" : connectionStatus === "error" ? "text-red-500" : "text-yellow-500"}>
                        {connectionStatus.toUpperCase()}
                    </span>
                </div>
            </div>

            <div>
                <h3 className="text-white font-bold mb-2 uppercase text-[10px] tracking-widest opacity-60">Stream Context</h3>
                <div className="flex flex-col gap-1">
                    <div className="flex justify-between">
                        <span>Current Symbol:</span>
                        <span className="text-white">{currentSymbol}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Last Sequence:</span>
                        <span className="text-blue-400">{lastSequence}</span>
                    </div>
                </div>
            </div>

            <div>
                <h3 className="text-white font-bold mb-2 uppercase text-[10px] tracking-widest opacity-60">Store Metrics</h3>
                <div className="flex flex-col gap-1">
                    <div className="flex justify-between">
                        <span>Ignored (Dupes):</span>
                        <span>{metrics.events_ignored}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Gaps Detected:</span>
                        <span className={metrics.gaps_detected > 0 ? "text-red-500 text-glow-sell" : ""}>{metrics.gaps_detected}</span>
                    </div>
                </div>
            </div>

            <div>
                <h3 className="text-white font-bold mb-2 uppercase text-[10px] tracking-widest opacity-60">Delta Buffers</h3>
                <div className="flex flex-col gap-1">
                    {Array.from(metrics.buffer_size_by_stream.entries()).length === 0 ? (
                        <span className="text-gray-500 italic">No buffered items</span>
                    ) : (
                        Array.from(metrics.buffer_size_by_stream.entries()).map(([stream, size]) => (
                            <div key={stream} className="flex justify-between">
                                <span className="truncate pr-2 text-slate-400">{stream}:</span>
                                <span className="text-white">{size}</span>
                            </div>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
}
