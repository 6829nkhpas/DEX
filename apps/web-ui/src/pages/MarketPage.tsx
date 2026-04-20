// ---------------------------------------------------------------------------
// MarketPage — main trading view
// ---------------------------------------------------------------------------
// Phase 15: responsive CSS Grid layout, section organization, improved
//           market selector, removed inline connection dot (uses banner),
//           consistent panel structure.
// ---------------------------------------------------------------------------

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
import { EmptyState } from "../components/ui/EmptyState";

// ---------------------------------------------------------------------------
// AuthGatePanel — inline auth gate for account-specific panels on this page
// ---------------------------------------------------------------------------

const AuthGatePanel: React.FC<{ label: string; gridArea?: string; children: React.ReactNode }> = ({
    label,
    gridArea,
    children,
}) => {
    const { authStatus, signIn } = useAuth();

    if (authStatus === "authenticated") {
        return <>{children}</>;
    }

    return (
        <div className="glass-panel p-5 rounded-2xl border-t border-indigo-500/20" style={gridArea ? { gridArea } : undefined}>
            <EmptyState
                icon="lock"
                message={`Sign in to view ${label}`}
                action={
                    authStatus !== "signing"
                        ? { label: "Sign In", onClick: () => signIn().catch(() => { }) }
                        : undefined
                }
            />
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

    const SYMBOLS = ["BTC/USDT", "ETH/USDT", "SOL/USDT"];

    return (
        <div className="p-4 lg:p-6 min-h-screen flex flex-col gap-4 text-white font-sans overflow-x-hidden">
            {/* Market selector bar */}
            <div className="flex items-center gap-3 flex-wrap">
                <div className="flex items-center gap-1 glass-panel rounded-xl overflow-hidden border border-indigo-500/15">
                    {SYMBOLS.map((s) => (
                        <button
                            key={s}
                            onClick={() => setSymbol(s)}
                            className={`px-4 py-2 text-sm font-bold tracking-wide transition-all ${
                                symbol === s
                                    ? "bg-indigo-500/20 text-white"
                                    : "text-slate-400 hover:text-white hover:bg-slate-800/50"
                            }`}
                        >
                            {s}
                        </button>
                    ))}
                </div>
            </div>

            {/* Trading grid — responsive layout */}
            <div className="trading-grid">
                <TickerPanel symbol={symbol} />
                <Orderbook symbol={symbol} />
                <TradeTape symbol={symbol} />
                {/* OrderEntry has its own internal auth gate */}
                <OrderEntry symbol={symbol} />
                {/* AccountPanel — requires auth */}
                <AuthGatePanel label="account balances" gridArea="account">
                    <AccountPanel />
                </AuthGatePanel>
                {/* Orders — requires auth */}
                <AuthGatePanel label="open orders" gridArea="orders">
                    <OpenOrders />
                </AuthGatePanel>
                {/* Positions — requires auth */}
                <AuthGatePanel label="positions" gridArea="positions">
                    <Positions />
                </AuthGatePanel>
            </div>
        </div>
    );
};
