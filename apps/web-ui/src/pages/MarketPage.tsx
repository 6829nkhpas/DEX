import React, { useState, useEffect, useRef } from "react";
import { useDexStore } from "../state/StoreProvider";
import { useWallet } from "../wallet/WalletProvider";
import { useAuth } from "../auth/AuthProvider";
import { Orderbook } from "../components/Orderbook/Orderbook";
import { TradeTape } from "../components/TradeTape/TradeTape";
import { TickerPanel } from "../components/Ticker/TickerPanel";
import { OrderEntry } from "../components/OrderEntry/OrderEntry";
import { OpenOrders } from "../components/OpenOrders/OpenOrders";
import { Positions } from "../components/Positions/Positions";
import { AccountPanel } from "../components/Account/AccountPanel";

// ---------------------------------------------------------------------------
// AuthGatePanel — inline auth gate for account-specific panels on this page
// ---------------------------------------------------------------------------

const AuthGatePanel: React.FC<{ label: string; children: React.ReactNode }> = ({
    label,
    children,
}) => {
    const { authStatus, signIn } = useAuth();

    if (authStatus === "authenticated") {
        return <>{children}</>;
    }

    return (
        <div className="glass-panel p-5 rounded-2xl flex items-center justify-center gap-3 border border-amber-500/20 bg-amber-500/5 text-amber-400 text-sm font-medium min-h-[80px]">
            <svg className="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                    d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
            <span>Sign in to view {label}</span>
            {authStatus !== "signing" && (
                <button
                    onClick={() => signIn().catch(() => { })}
                    className="ml-1 text-xs font-bold px-3 py-1 rounded-lg bg-amber-500/20 border border-amber-500/30 hover:bg-amber-500 hover:text-white hover:border-amber-500 transition-all"
                >
                    Sign In
                </button>
            )}
        </div>
    );
};

// ---------------------------------------------------------------------------
// MarketPage
// ---------------------------------------------------------------------------

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
                    <div className={`w-2 h-2 rounded-full ${connectionStatus === "connected" ? "bg-green-500" : connectionStatus === "connecting" ? "bg-yellow-500 animate-pulse" : "bg-red-500"}`}></div>
                    {connectionStatus}
                </div>
            </div>

            <TickerPanel symbol={symbol} />

            <div className="flex flex-row gap-6 mt-4 items-start">
                <Orderbook symbol={symbol} />
                <TradeTape symbol={symbol} />
                <div className="flex flex-col gap-4">
                    {/* OrderEntry has its own internal auth gate */}
                    <OrderEntry symbol={symbol} />
                    {/* AccountPanel — requires auth */}
                    <AuthGatePanel label="account balances">
                        <AccountPanel />
                    </AuthGatePanel>
                </div>
            </div>

            {/* Orders & Positions panels — both require auth */}
            <div className="flex flex-col gap-6 mt-2">
                <AuthGatePanel label="open orders">
                    <OpenOrders />
                </AuthGatePanel>
                <AuthGatePanel label="positions">
                    <Positions />
                </AuthGatePanel>
            </div>
        </div>
    );
};
