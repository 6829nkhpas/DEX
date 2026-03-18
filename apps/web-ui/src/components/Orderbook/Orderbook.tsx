import React, { useMemo } from "react";
import Decimal from "decimal.js";
import { useDexStore } from "../../state/StoreProvider";
import { PriceLevel } from "../../state/types";

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
    type
}: {
    price: string,
    qty: string,
    total: string,
    maxTotal: Decimal,
    type: "bid" | "ask"
}) => {
    const colorClass = type === "bid" ? "text-[#00E676] text-glow-buy" : "text-[#FF1744] text-glow-sell";
    const bgClass = type === "bid" ? "bg-[#00E676]" : "bg-[#FF1744]";

    return (
        <div className="relative flex justify-between text-sm py-1 px-4 hover:bg-slate-800/50 cursor-pointer font-mono transition-colors group">
            <DepthBar total={total} maxTotal={maxTotal} color={bgClass} />
            <span className={`z-10 font-medium ${colorClass} group-hover:brightness-125 transition-all`}>{price}</span>
            <span className="z-10 text-slate-200">{qty}</span>
            <span className="z-10 text-slate-500">{total}</span>
        </div>
    );
});

export const Orderbook: React.FC<OrderbookProps> = React.memo(({ symbol }) => {
    const { state } = useDexStore();
    const orderbook = state.orderbooks.get(symbol);

    const { bidsWithTotal, asksWithTotal, maxDepth } = useMemo(() => {
        if (!orderbook) {
            return { bidsWithTotal: [], asksWithTotal: [], maxDepth: new Decimal(0) };
        }

        const topBids = orderbook.bids.slice(0, 25);
        // Asks are sorted ascending (lowest price first). We want to display highest price at the top?
        // Wait, standard orderbook: Asks highest at top, descending down to lowest ask. Then bids highest bid, descending down.
        // If asks are ASC in state: index 0 is lowest ask (best ask).
        // Best ask should be closest to spread. That's usually at the bottom of the asks list.
        // So we take top 25 asks, and reverse them for display.
        const topAsks = orderbook.asks.slice(0, 25);

        let bidTotal = new Decimal(0);
        const bWithTotal = topBids.map(([price, qty]) => {
            bidTotal = bidTotal.plus(qty);
            return { price, qty, total: bidTotal.toString() };
        });

        let askTotal = new Decimal(0);
        // We compute cumulative over topAsks starting from best (index 0).
        const aWithTotal = topAsks.map(([price, qty]) => {
            askTotal = askTotal.plus(qty);
            return { price, qty, total: askTotal.toString() };
        });

        // The max depth for the background bars
        const maxBidsTotal = bidTotal;
        const maxAsksTotal = askTotal;
        const maxDepth = Decimal.max(maxBidsTotal, maxAsksTotal);

        return {
            bidsWithTotal: bWithTotal,
            // reverse for display so best ask is at the bottom of the top half
            asksWithTotal: aWithTotal.reverse(),
            maxDepth
        };
    }, [orderbook]); // recompute only when orderbook object changes (assuming immutable store)

    if (!orderbook || (bidsWithTotal.length === 0 && asksWithTotal.length === 0)) {
        return <div className="p-4 text-slate-500 glass-panel rounded-xl flex items-center justify-center w-72 h-[500px]">Waiting for orderbook snapshot...</div>;
    }

    return (
        <div className="flex flex-col w-72 glass-panel rounded-xl shadow-2xl overflow-hidden relative border-t border-indigo-500/20">
            <div className="flex justify-between px-4 py-2 text-xs font-semibold tracking-wider text-slate-400 border-b border-indigo-500/10 bg-slate-900/40 uppercase">
                <span>Price</span>
                <span>Size</span>
                <span>Total</span>
            </div>

            <div className="flex flex-col overflow-hidden py-1">
                {/* Asks (Red) */}
                <div className="flex flex-col flex-1 justify-end pb-1 border-b border-indigo-500/10">
                    {asksWithTotal.map((item) => (
                        <OrderbookRow
                            key={item.price}
                            type="ask"
                            price={item.price}
                            qty={item.qty}
                            total={item.total}
                            maxTotal={maxDepth}
                        />
                    ))}
                </div>

                {/* Spread spacing */}
                <div className="py-2 text-center text-sm font-mono bg-slate-900/40 flex items-center justify-center gap-2 shadow-[inset_0_2px_10px_rgba(0,0,0,0.2)]">
                    <span className="text-slate-400 font-semibold tracking-widest text-xs">SPREAD</span>
                    <span className="text-slate-300">
                        {asksWithTotal.length > 0 && bidsWithTotal.length > 0
                            ? new Decimal(asksWithTotal[asksWithTotal.length - 1].price).minus(bidsWithTotal[0].price).abs().toString()
                            : "-"}
                    </span>
                </div>

                {/* Bids (Green) */}
                <div className="flex flex-col pt-1 flex-1 border-t border-indigo-500/10">
                    {bidsWithTotal.map((item) => (
                        <OrderbookRow
                            key={item.price}
                            type="bid"
                            price={item.price}
                            qty={item.qty}
                            total={item.total}
                            maxTotal={maxDepth}
                        />
                    ))}
                </div>
            </div>
        </div>
    );
});
