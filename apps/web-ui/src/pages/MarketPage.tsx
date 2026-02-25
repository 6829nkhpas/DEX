import React, { useState, useEffect } from "react";
import { useDexStore } from "../state/StoreProvider";
import { Orderbook } from "../components/Orderbook/Orderbook";
import { TradeTape } from "../components/TradeTape/TradeTape";
import { TickerPanel } from "../components/Ticker/TickerPanel";

export const MarketPage: React.FC = () => {
    const [symbol, setSymbol] = useState("BTC/USDT");
    const { client, connectionStatus } = useDexStore();

    useEffect(() => {
        if (connectionStatus === "connected") {
            const params = { symbol };

            client.subscribe("market_data", params);
            client.subscribe("trades", params);

            return () => {
                client.unsubscribe("market_data", params);
                client.unsubscribe("trades", params);
            };
        }
    }, [symbol, client, connectionStatus]);

    return (
        <div className="p-6 bg-black min-h-screen flex flex-col gap-6 text-white font-sans overflow-x-hidden">
            <div className="flex items-center gap-4 border-b border-gray-800 pb-4">
                <h1 className="text-2xl font-bold">Markets</h1>
                <select
                    value={symbol}
                    onChange={(e) => setSymbol(e.target.value)}
                    className="bg-gray-900 border border-gray-700 text-white text-sm rounded focus:ring-blue-500 focus:border-blue-500 block p-2"
                >
                    <option value="BTC/USDT">BTC/USDT</option>
                    <option value="ETH/USDT">ETH/USDT</option>
                    <option value="SOL/USDT">SOL/USDT</option>
                </select>
                <div className="ml-auto flex items-center gap-2 text-sm text-gray-400">
                    <div className={`w-2 h-2 rounded-full ${connectionStatus === "connected" ? "bg-green-500" : connectionStatus === "connecting" ? "bg-yellow-500" : "bg-red-500"}`}></div>
                    {connectionStatus}
                </div>
            </div>

            <TickerPanel symbol={symbol} />

            <div className="flex flex-row gap-6 mt-4 items-start">
                <Orderbook symbol={symbol} />
                <TradeTape symbol={symbol} />
            </div>
        </div>
    );
};
