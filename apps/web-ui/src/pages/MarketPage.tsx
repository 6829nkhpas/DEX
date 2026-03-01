import React, { useState, useEffect, useRef } from "react";
import { useDexStore } from "../state/StoreProvider";
import { useWallet } from "../wallet/WalletProvider";
import { Orderbook } from "../components/Orderbook/Orderbook";
import { TradeTape } from "../components/TradeTape/TradeTape";
import { TickerPanel } from "../components/Ticker/TickerPanel";
import { OrderEntry } from "../components/OrderEntry/OrderEntry";
import { OpenOrders } from "../components/OpenOrders/OpenOrders";
import { Positions } from "../components/Positions/Positions";
import { AccountPanel } from "../components/Account/AccountPanel";

export const MarketPage: React.FC = () => {
    const [symbol, setSymbol] = useState("BTC/USDT");
    const { client, connectionStatus } = useDexStore();
    const { accountId } = useWallet();

    // Track previous account_id so we can unsubscribe the old one on switch
    const prevAccountId = useRef<string | null>(null);

    // Market data + trades subscription (symbol-based)
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

    // Account subscription — dynamic, re-subscribes when wallet changes
    useEffect(() => {
        if (connectionStatus !== "connected") return;

        // Unsubscribe previous account when wallet switches
        if (prevAccountId.current && prevAccountId.current !== accountId) {
            client.unsubscribe("account", { account_id: prevAccountId.current });
        }

        if (accountId) {
            client.subscribe("account", { account_id: accountId });
            prevAccountId.current = accountId;
        }

        return () => {
            if (accountId) {
                client.unsubscribe("account", { account_id: accountId });
            }
        };
    }, [accountId, client, connectionStatus]);

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
                <div className="flex flex-col gap-4">
                    <OrderEntry symbol={symbol} />
                    <AccountPanel />
                </div>
            </div>

            {/* Orders & Positions panels */}
            <div className="flex flex-col gap-6 mt-2">
                <OpenOrders />
                <Positions />
            </div>
        </div>
    );
};
