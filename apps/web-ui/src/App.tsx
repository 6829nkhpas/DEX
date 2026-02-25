import React from "react";
import { Route, Switch, Link } from "wouter";
import { DebugPanel } from "./components/DebugPanel";

// Placeholder pages
const Home = () => (
    <div className="p-8">
        <h1 className="text-3xl font-bold mb-4">DEX Trading Platform</h1>
        <p className="text-gray-400">Welcome to the distributed exchange. UI is under construction.</p>
    </div>
);

const Trade = () => (
    <div className="p-8">
        <h1 className="text-3xl font-bold mb-4 flex items-center gap-3">
            <span className="bg-blue-600 px-2 py-1 rounded text-sm text-white font-mono">BTC/USDT</span>
            Trading View
        </h1>
        <p className="text-gray-400">Orderbook, Tickers, and Trade form will live here.</p>
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
                    <Route path="/trade" component={Trade} />
                    <Route component={NotFound} />
                </Switch>
            </main>

            {/* Global Debug Panel (Phase 14A requirement) */}
            <DebugPanel />
        </div>
    );
}
