// ---------------------------------------------------------------------------
// Orderbook — live order book with depth visualization
// ---------------------------------------------------------------------------
// Phase 15: added panel header, skeleton loading, spread percentage,
//           responsive width, best bid/ask highlight, consistent styling.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import Decimal from "decimal.js";
import { useDexStore } from "../../state/StoreProvider";
import { LoadingSkeleton } from "../ui/LoadingSkeleton";

interface OrderbookProps {
    symbol: string;
}

const DepthBar = React.memo(({ total, maxTotal, color }: { total: string, maxTotal: Decimal, color: string }) => {
    const width = useMemo(() => {
        if (maxTotal.isZero()) return 0;
        return new Decimal(total).div(maxTotal).mul(100).toNumber();
    }, [total, maxTotal]);

    const gradient = color === "bg-[#00E676]"
        ? "bg-gradient-to-l from-[#00E676]/20 to-transparent"
        : "bg-gradient-to-l from-[#FF1744]/20 to-transparent";

    return (
        <div
            className={`absolute top-0 right-0 h-full ${gradient} pointer-events-none transition-all duration-300 ease-in-out`}
            style={{ width: `${width}%` }}
        />
    );
});

// React.memo with custom comparison avoids full re-render if data hasn't changed.
// We can just rely on the reference equality of bids/asks if the reducer is immutable.
const OrderbookRow = React.memo(({
    price,
    qty,
    total,
    maxTotal,
    type,
    isBest
}: {
    price: string,
    qty: string,
    total: string,
    maxTotal: Decimal,
    type: "bid" | "ask",
    isBest?: boolean
}) => {
    const colorClass = type === "bid" ? "text-[#00E676] text-glow-buy" : "text-[#FF1744] text-glow-sell";
    const bgClass = type === "bid" ? "bg-[#00E676]" : "bg-[#FF1744]";
    const bestHighlight = isBest ? (type === "bid" ? "bg-[#00E676]/5" : "bg-[#FF1744]/5") : "";

    return (
        <div className={`relative flex justify-between text-sm py-1 px-4 hover:bg-slate-800/50 cursor-pointer font-mono transition-colors group ${bestHighlight}`}>
            <DepthBar total={total} maxTotal={maxTotal} color={bgClass} />
            <span className={`z-10 font-medium ${colorClass} group-hover:brightness-125 transition-all tabular-nums`}>{price}</span>
            <span className="z-10 text-slate-200 tabular-nums">{qty}</span>
            <span className="z-10 text-slate-500 tabular-nums">{total}</span>
        </div>
    );
});

export const Orderbook: React.FC<OrderbookProps> = React.memo(({ symbol }) => {
    const { state } = useDexStore();
    const orderbook = state.orderbooks.get(symbol);

    const { bidsWithTotal, asksWithTotal, maxDepth, spreadStr, spreadPct } = useMemo(() => {
        if (!orderbook) {
            return { bidsWithTotal: [], asksWithTotal: [], maxDepth: new Decimal(0), spreadStr: "-", spreadPct: "-" };
        }

        const topBids = orderbook.bids.slice(0, 25);
        const topAsks = orderbook.asks.slice(0, 25);

        let bidTotal = new Decimal(0);
        const bWithTotal = topBids.map(([price, qty]) => {
            bidTotal = bidTotal.plus(qty);
            return { price, qty, total: bidTotal.toString() };
        });

        let askTotal = new Decimal(0);
        const aWithTotal = topAsks.map(([price, qty]) => {
            askTotal = askTotal.plus(qty);
            return { price, qty, total: askTotal.toString() };
        });

        const maxBidsTotal = bidTotal;
        const maxAsksTotal = askTotal;
        const maxDepth = Decimal.max(maxBidsTotal, maxAsksTotal);

        // Spread calculation
        let spreadStr = "-";
        let spreadPct = "-";
        if (topAsks.length > 0 && topBids.length > 0) {
            const bestAsk = new Decimal(topAsks[0][0]);
            const bestBid = new Decimal(topBids[0][0]);
            const spread = bestAsk.minus(bestBid).abs();
            spreadStr = spread.toString();
            if (!bestBid.isZero()) {
                spreadPct = spread.div(bestBid).mul(100).toFixed(3) + "%";
            }
        }

        return {
            bidsWithTotal: bWithTotal,
            asksWithTotal: aWithTotal.reverse(),
            maxDepth,
            spreadStr,
            spreadPct,
        };
    }, [orderbook]);

    return (
        <div
            id="orderbook-panel"
            className="flex flex-col glass-panel rounded-xl shadow-2xl overflow-hidden relative border-t border-indigo-500/20 min-w-[260px]"
            style={{ gridArea: "orderbook" }}
        >
            {/* Panel header */}
            <div className="flex items-center justify-between px-4 py-2.5 border-b border-indigo-500/10 bg-slate-900/30">
                <span className="panel-header">Orderbook</span>
                <span className="text-[10px] text-slate-500 font-mono">{symbol}</span>
            </div>

            {/* Column headers */}
            <div className="flex justify-between px-4 py-1.5 text-[10px] font-semibold tracking-wider text-slate-500 uppercase bg-slate-900/20">
                <span>Price</span>
                <span>Size</span>
                <span>Total</span>
            </div>

            {(!orderbook || (bidsWithTotal.length === 0 && asksWithTotal.length === 0)) ? (
                <div className="py-2">
                    <LoadingSkeleton variant="row" count={8} />
                </div>
            ) : (
                <div className="flex flex-col overflow-hidden">
                    {/* Asks (Red) */}
                    <div className="flex flex-col flex-1 justify-end pb-1 border-b border-indigo-500/10">
                        {asksWithTotal.map((item, idx) => (
                            <OrderbookRow
                                key={item.price}
                                type="ask"
                                price={item.price}
                                qty={item.qty}
                                total={item.total}
                                maxTotal={maxDepth}
                                isBest={idx === asksWithTotal.length - 1}
                            />
                        ))}
                    </div>

                    {/* Spread */}
                    <div className="py-1.5 px-4 text-center text-xs font-mono bg-slate-900/40 flex items-center justify-center gap-3 shadow-[inset_0_2px_10px_rgba(0,0,0,0.2)]">
                        <span className="text-slate-500 font-semibold tracking-widest text-[10px] uppercase">Spread</span>
                        <span className="text-slate-300 tabular-nums">{spreadStr}</span>
                        <span className="text-slate-500 tabular-nums text-[10px]">({spreadPct})</span>
                    </div>

                    {/* Bids (Green) */}
                    <div className="flex flex-col pt-1 flex-1 border-t border-indigo-500/10">
                        {bidsWithTotal.map((item, idx) => (
                            <OrderbookRow
                                key={item.price}
                                type="bid"
                                price={item.price}
                                qty={item.qty}
                                total={item.total}
                                maxTotal={maxDepth}
                                isBest={idx === 0}
                            />
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
});
