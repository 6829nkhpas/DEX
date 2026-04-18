// ---------------------------------------------------------------------------
// TradeTape — recent trades feed
// ---------------------------------------------------------------------------
// Phase 15: added panel header, skeleton loading, responsive width,
//           new-trade flash animation, consistent styling.
// ---------------------------------------------------------------------------

import React from "react";
import { useDexStore } from "../../state/StoreProvider";
import { Side } from "../../../../../types/generated-types";
import { LoadingSkeleton } from "../ui/LoadingSkeleton";

interface TradeTapeProps {
    symbol: string;
}

export const TradeTape: React.FC<TradeTapeProps> = React.memo(({ symbol }) => {
    const { store, state } = useDexStore();
    // Use the optimized getter from the store for trades
    // Also use state.trades just to trigger re-renders when state changes
    const trades = store.getTrades(symbol);

    return (
        <div
            id="trade-tape-panel"
            className="flex flex-col glass-panel rounded-xl h-[500px] shadow-2xl overflow-hidden relative border-t border-indigo-500/20 min-w-[260px]"
            style={{ gridArea: "trades" }}
        >
            {/* Panel header */}
            <div className="flex items-center justify-between px-4 py-2.5 border-b border-indigo-500/10 bg-slate-900/30">
                <span className="panel-header">Recent Trades</span>
                {trades && trades.length > 0 && (
                    <span className="panel-count">{trades.length}</span>
                )}
            </div>

            {/* Column headers */}
            <div className="flex justify-between px-4 py-1.5 text-[10px] font-semibold tracking-wider text-slate-500 uppercase bg-slate-900/20">
                <span>Price</span>
                <span>Size</span>
                <span>Time</span>
            </div>

            {!trades || trades.length === 0 ? (
                <div className="py-2 flex-1">
                    <LoadingSkeleton variant="row" count={10} />
                </div>
            ) : (
                <div className="flex flex-col overflow-y-auto overflow-x-hidden custom-scrollbar">
                    {trades.map((trade) => {
                        const isBuy = trade.side === Side.BUY;
                        const colorClass = isBuy ? "text-[#00E676] text-glow-buy" : "text-[#FF1744] text-glow-sell";
                        // format time as HH:mm:ss
                        // timestamp is nanoseconds string, convert to ms
                        const date = new Date(Number(BigInt(trade.timestamp) / 1_000_000n));
                        const timeStr = date.toLocaleTimeString([], { hour12: false });

                        return (
                            <div key={trade.event_id} className="flex justify-between py-1.5 px-4 text-sm hover:bg-indigo-500/10 cursor-default font-mono transition-colors animate-fade-in">
                                <span className={`tabular-nums ${colorClass}`}>{trade.price}</span>
                                <span className="text-slate-200 tabular-nums">{trade.quantity}</span>
                                <span className="text-slate-500 text-xs self-center tabular-nums">{timeStr}</span>
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
});
