import React from "react";
import { useDexStore } from "../../state/StoreProvider";
import { Side } from "../../../../../types/generated-types";

interface TradeTapeProps {
    symbol: string;
}

export const TradeTape: React.FC<TradeTapeProps> = React.memo(({ symbol }) => {
    const { store, state } = useDexStore();
    // Use the optimized getter from the store for trades
    // Also use state.trades just to trigger re-renders when state changes
    const trades = store.getTrades(symbol);

    if (!trades || trades.length === 0) {
        return <div className="p-4 text-gray-500">Waiting for trades...</div>;
    }

    return (
        <div className="flex flex-col w-64 bg-gray-900 border border-gray-800 rounded h-[500px]">
            <div className="flex justify-between px-2 py-1 text-xs text-gray-500 border-b border-gray-800">
                <span>Price</span>
                <span>Size</span>
                <span>Time</span>
            </div>

            <div className="flex flex-col overflow-y-auto overflow-x-hidden">
                {trades.map((trade) => {
                    const isBuy = trade.side === Side.BUY;
                    const colorClass = isBuy ? "text-green-500" : "text-red-500";
                    // format time as HH:mm:ss
                    // timestamp is nanoseconds string, convert to ms
                    const date = new Date(Number(BigInt(trade.timestamp) / 1_000_000n));
                    const timeStr = date.toLocaleTimeString([], { hour12: false });

                    return (
                        <div key={trade.event_id} className="flex justify-between py-1 px-2 text-sm hover:bg-gray-800 font-mono">
                            <span className={colorClass}>{trade.price}</span>
                            <span className="text-gray-300">{trade.quantity}</span>
                            <span className="text-gray-500 text-xs self-center">{timeStr}</span>
                        </div>
                    );
                })}
            </div>
        </div>
    );
});
