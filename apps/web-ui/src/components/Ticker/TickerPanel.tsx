import React from "react";
import { useDexStore } from "../../state/StoreProvider";

interface TickerPanelProps {
    symbol: string;
}

export const TickerPanel: React.FC<TickerPanelProps> = React.memo(({ symbol }) => {
    const { state } = useDexStore();
    const ticker = state.tickers.get(symbol);

    if (!ticker) {
        return (
            <div className="flex h-16 w-full items-center px-4 bg-gray-900 border border-gray-800 rounded text-gray-500">
                <span className="text-xl font-bold mr-4">{symbol}</span>
                <span>Waiting for ticker data...</span>
            </div>
        );
    }

    return (
        <div className="flex flex-row items-center w-full bg-gray-900 border border-gray-800 rounded px-6 py-3 font-mono space-x-8">
            <div className="flex flex-col">
                <span className="text-xl font-bold text-white">{ticker.symbol}</span>
            </div>

            <div className="flex flex-col">
                <span className="text-xs text-gray-500 uppercase">Last Price</span>
                <span className="text-lg text-white font-semibold">{ticker.last_price}</span>
            </div>

            <div className="flex flex-col">
                <span className="text-xs text-gray-500 uppercase">24h Vol</span>
                <span className="text-sm text-gray-300">{ticker.volume_24h}</span>
            </div>

            <div className="flex flex-col">
                <span className="text-xs text-gray-500 uppercase">24h High</span>
                <span className="text-sm text-gray-300">{ticker.high_24h}</span>
            </div>

            <div className="flex flex-col">
                <span className="text-xs text-gray-500 uppercase">24h Low</span>
                <span className="text-sm text-gray-300">{ticker.low_24h}</span>
            </div>

            <div className="flex flex-col">
                <span className="text-xs text-gray-500 uppercase">Mark Price</span>
                <span className="text-sm text-gray-300">{ticker.mark_price}</span>
            </div>
        </div>
    );
});
