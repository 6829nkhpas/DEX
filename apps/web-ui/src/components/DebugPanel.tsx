// apps/web-ui/src/components/DebugPanel.tsx
import React, { useMemo, useState, useEffect } from "react";
import { useDexStore } from "../state/StoreProvider";

export function DebugPanel() {
    const { state, connectionStatus } = useDexStore();
    const { metrics } = state;

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

    return (
        <div className="fixed bottom-4 right-4 w-96 bg-gray-900 border border-gray-700 text-xs text-green-400 p-4 rounded-lg shadow-xl font-mono flex flex-col gap-4 z-50">
            <div>
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Connection</h3>
                <div className="flex justify-between">
                    <span>Status:</span>
                    <span className={connectionStatus === "connected" ? "text-green-500" : connectionStatus === "error" ? "text-red-500" : "text-yellow-500"}>
                        {connectionStatus.toUpperCase()}
                    </span>
                </div>
            </div>

            <div>
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Stream Context</h3>
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
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Store Metrics</h3>
                <div className="flex flex-col gap-1">
                    <div className="flex justify-between">
                        <span>Ignored (Dupes):</span>
                        <span>{metrics.events_ignored}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Gaps Detected:</span>
                        <span className={metrics.gaps_detected > 0 ? "text-red-500" : ""}>{metrics.gaps_detected}</span>
                    </div>
                </div>
            </div>

            <div>
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Delta Buffers</h3>
                <div className="flex flex-col gap-1">
                    {Array.from(metrics.buffer_size_by_stream.entries()).length === 0 ? (
                        <span className="text-gray-500 italic">No buffered items</span>
                    ) : (
                        Array.from(metrics.buffer_size_by_stream.entries()).map(([stream, size]) => (
                            <div key={stream} className="flex justify-between">
                                <span className="truncate pr-2">{stream}:</span>
                                <span>{size}</span>
                            </div>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
}
