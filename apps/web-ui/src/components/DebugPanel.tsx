// apps/web-ui/src/components/DebugPanel.tsx
import React from "react";
import { useDexStore } from "../state/StoreProvider";

export function DebugPanel() {
    const { state, connectionStatus } = useDexStore();
    const { metrics } = state;

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
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Store Metrics</h3>
                <div className="flex flex-col gap-1">
                    <div className="flex justify-between">
                        <span>Ignored (Dupes):</span>
                        <span>{metrics.events_ignored}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Gaps Detected:</span>
                        <span>{metrics.gaps_detected}</span>
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

            <div>
                <h3 className="text-white font-bold mb-2 uppercase border-b border-gray-700 pb-1">Active Streams</h3>
                <div className="flex flex-col gap-1">
                    <div className="flex justify-between">
                        <span>Orderbooks:</span>
                        <span>{state.orderbooks.size}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Tickers:</span>
                        <span>{state.tickers.size}</span>
                    </div>
                    <div className="flex justify-between">
                        <span>Trades:</span>
                        <span>{state.trades.size}</span>
                    </div>
                </div>
            </div>
        </div>
    );
}
