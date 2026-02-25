import React, { useEffect } from "react";
import { Route, Switch, Link } from "wouter";
import { DebugPanel } from "./components/DebugPanel";
import { useDexStore } from "./state/StoreProvider";
import type { BaseEvent } from "../../../types/generated-types";
import type { OrderbookSnapshotPayload, OrderbookDeltaPayload, TradePayload, TickerDeltaPayload } from "./state/types";
import { Side } from "../../../types/generated-types";
import { MarketPage } from "./pages/MarketPage";

function MockEventSimulation() {
    const { store } = useDexStore();

    useEffect(() => {
        let seq = 1;

        const timer1 = setTimeout(() => {
            const snapshot: BaseEvent<OrderbookSnapshotPayload> = {
                event_id: `mock-snap-${seq}`,
                event_type: "snapshot",
                sequence: String(seq),
                source: "market_data",
                timestamp: String(Date.now() * 1_000_000),
                payload: {
                    symbol: "BTC/USDT",
                    bids: [["50000.00", "1.5"], ["49999.00", "2.0"], ["49998.00", "0.5"], ["49990.00", "3.0"]],
                    asks: [["50010.00", "2.0"], ["50011.00", "1.1"], ["50015.00", "0.8"], ["50020.00", "4.0"]]
                },
                metadata: { version: "1.0", correlation_id: "", causation_id: "" }
            };
            store.dispatch(snapshot);

            // Ticker snapshot delta
            seq++;
            const tickerDelta: BaseEvent<TickerDeltaPayload> = {
                event_id: `mock-ticker-${seq}`,
                event_type: "delta",
                sequence: String(seq),
                source: "market_data",
                timestamp: String(Date.now() * 1_000_000),
                payload: {
                    symbol: "BTC/USDT",
                    last_price: "50005.00",
                    volume_24h: "12500.50",
                    high_24h: "51000.00",
                    low_24h: "49000.00",
                    mark_price: "50008.00"
                },
                metadata: { version: "1.0", correlation_id: "", causation_id: "" }
            };
            store.dispatch(tickerDelta);

            // Start sending deltas at high frequency
            const timer2 = setInterval(() => {
                seq++;

                // Randomly generate either an orderbook update or a trade
                const isTrade = Math.random() > 0.7;

                if (isTrade) {
                    const priceStr = (50000 + (Math.random() * 20 - 10)).toFixed(2);
                    const qtyStr = (Math.random() * 2).toFixed(4);
                    const side = Math.random() > 0.5 ? Side.BUY : Side.SELL;

                    const tradeDelta: BaseEvent<TradePayload> = {
                        event_id: `mock-trade-${seq}`,
                        event_type: "delta",
                        sequence: String(seq),
                        source: "trades",
                        timestamp: String(Date.now() * 1_000_000),
                        payload: {
                            symbol: "BTC/USDT",
                            price: priceStr,
                            quantity: qtyStr,
                            side
                        },
                        metadata: { version: "1.0", correlation_id: "", causation_id: "" }
                    };
                    store.dispatch(tradeDelta);
                } else {
                    const side = Math.random() > 0.5 ? "bids" : "asks";
                    const priceOffset = side === "bids" ? -Math.random() * 50 : Math.random() * 50;
                    const priceStr = (50000 + priceOffset).toFixed(2);
                    // sometimes qty 0 to remove level
                    const qtyStr = Math.random() > 0.8 ? "0.00" : (Math.random() * 5).toFixed(2);

                    const obDelta: BaseEvent<OrderbookDeltaPayload> = {
                        event_id: `mock-ob-${seq}`,
                        event_type: "delta",
                        sequence: String(seq),
                        source: "market_data",
                        timestamp: String(Date.now() * 1_000_000),
                        payload: {
                            symbol: "BTC/USDT",
                            [side]: [[priceStr, qtyStr]]
                        },
                        metadata: { version: "1.0", correlation_id: "", causation_id: "" }
                    };
                    store.dispatch(obDelta);
                }

                // Occasionally update ticker
                if (Math.random() > 0.95) {
                    seq++;
                    const tDelta: BaseEvent<TickerDeltaPayload> = {
                        event_id: `mock-tickupd-${seq}`,
                        event_type: "delta",
                        sequence: String(seq),
                        source: "market_data",
                        timestamp: String(Date.now() * 1_000_000),
                        payload: {
                            symbol: "BTC/USDT",
                            last_price: (50005 + (Math.random() * 10 - 5)).toFixed(2),
                        },
                        metadata: { version: "1.0", correlation_id: "", causation_id: "" }
                    };
                    store.dispatch(tDelta);
                }

            }, 10); // 100 updates/sec

            return () => clearInterval(timer2);
        }, 1000);

        return () => clearTimeout(timer1);
    }, [store]);

    return null;
}

// Placeholder pages
const Home = () => (
    <div className="p-8">
        <h1 className="text-3xl font-bold mb-4">DEX Trading Platform</h1>
        <p className="text-gray-400">Welcome to the distributed exchange. UI is under construction.</p>
    </div>
);

const NotFound = () => (
    <div className="p-8 text-center text-red-500">
        <h1 className="text-2xl font-bold mb-2">404</h1>
        <p>Page Not Found</p>
    </div>
);

export function App() {
    return (
        <div className="min-h-screen flex flex-col">
            <MockEventSimulation />
            {/* Header / Nav */}
            <header className="fixed top-0 w-full h-14 bg-gray-900 border-b border-gray-800 flex items-center px-6 z-40 shadow-sm">
                <div className="text-xl font-bold tracking-tight text-white mr-8">DEX</div>
                <nav className="flex gap-6">
                    <Link href="/" className="text-sm font-medium text-gray-400 hover:text-white transition-colors">
                        Markets
                    </Link>
                    <Link href="/trade" className="text-sm font-medium text-gray-400 hover:text-white transition-colors">
                        Trade
                    </Link>
                </nav>
            </header>

            {/* Main Content Area */}
            <main className="flex-1 mt-14 overflow-y-auto w-full relative">
                <Switch>
                    <Route path="/" component={Home} />
                    <Route path="/trade" component={MarketPage} />
                    <Route component={NotFound} />
                </Switch>
            </main>

            {/* Global Debug Panel (Phase 14A requirement) */}
            <DebugPanel />
        </div>
    );
}
