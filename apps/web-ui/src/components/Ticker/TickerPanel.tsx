// ---------------------------------------------------------------------------
// TickerPanel — live market data summary bar
// ---------------------------------------------------------------------------
// Phase 15: redesigned with glass-panel, loading skeleton, responsive wrap,
//           price change indicator, and improved typography hierarchy.
// ---------------------------------------------------------------------------

import React, { useRef, useEffect, useState } from "react";
import { useDexStore } from "../../state/StoreProvider";
import { LoadingSkeleton } from "../ui/LoadingSkeleton";

interface TickerPanelProps {
    symbol: string;
}

export const TickerPanel: React.FC<TickerPanelProps> = React.memo(({ symbol }) => {
    const { state } = useDexStore();
    const ticker = state.tickers.get(symbol);

    // Track previous price for change indicator
    const prevPriceRef = useRef<string | null>(null);
    const [priceDirection, setPriceDirection] = useState<"up" | "down" | null>(null);

    useEffect(() => {
        if (!ticker?.last_price) return;
        if (prevPriceRef.current && prevPriceRef.current !== ticker.last_price) {
            const prev = parseFloat(prevPriceRef.current);
            const curr = parseFloat(ticker.last_price);
            if (!isNaN(prev) && !isNaN(curr)) {
                setPriceDirection(curr > prev ? "up" : curr < prev ? "down" : null);
                // Clear direction after animation
                const timer = setTimeout(() => setPriceDirection(null), 800);
                // eslint-disable-next-line @typescript-eslint/no-unused-vars
                prevPriceRef.current = ticker.last_price;
                return () => clearTimeout(timer);
            }
        }
        prevPriceRef.current = ticker.last_price;
    }, [ticker?.last_price]);

    if (!ticker) {
        return (
            <div className="glass-panel rounded-xl border-t border-indigo-500/15 overflow-hidden" style={{ gridArea: "ticker" }}>
                <LoadingSkeleton variant="ticker" />
            </div>
        );
    }

    const priceColor = priceDirection === "up"
        ? "text-[#00E676] text-glow-buy"
        : priceDirection === "down"
            ? "text-[#FF1744] text-glow-sell"
            : "text-white";

    const priceArrow = priceDirection === "up" ? "↑" : priceDirection === "down" ? "↓" : "";

    return (
        <div
            className="glass-panel rounded-xl border-t border-indigo-500/15 flex flex-row items-center flex-wrap gap-x-8 gap-y-2 px-6 py-3 font-mono"
            style={{ gridArea: "ticker" }}
        >
            {/* Symbol */}
            <div className="flex items-center gap-2.5 mr-2">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-amber-500/20 to-amber-600/10 border border-amber-500/20 flex items-center justify-center text-amber-400 text-xs font-bold">
                    {symbol.split("/")[0].slice(0, 3)}
                </div>
                <span className="text-lg font-display font-bold text-white tracking-tight">{symbol}</span>
            </div>

            {/* Last Price (primary) */}
            <div className="flex flex-col">
                <span className="text-[10px] text-slate-500 uppercase font-sans font-semibold tracking-wider">Last Price</span>
                <span className={`text-xl font-bold tabular-nums transition-colors duration-300 ${priceColor}`}>
                    {ticker.last_price}
                    {priceArrow && (
                        <span className="text-sm ml-1">{priceArrow}</span>
                    )}
                </span>
            </div>

            {/* Mark Price */}
            {ticker.mark_price && (
                <TickerStat label="Mark" value={ticker.mark_price} />
            )}

            {/* 24h Stats */}
            {ticker.volume_24h && <TickerStat label="24h Vol" value={ticker.volume_24h} />}
            {ticker.high_24h && <TickerStat label="24h High" value={ticker.high_24h} />}
            {ticker.low_24h && <TickerStat label="24h Low" value={ticker.low_24h} />}
        </div>
    );
});

const TickerStat: React.FC<{ label: string; value: string }> = ({ label, value }) => (
    <div className="flex flex-col">
        <span className="text-[10px] text-slate-500 uppercase font-sans font-semibold tracking-wider">{label}</span>
        <span className="text-sm text-slate-300 tabular-nums">{value}</span>
    </div>
);
