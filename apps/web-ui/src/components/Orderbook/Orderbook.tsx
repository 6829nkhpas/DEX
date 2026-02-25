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

    return (
        <div
            className={`absolute top-0 right-0 h-full opacity-10 ${color}`}
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
    const colorClass = type === "bid" ? "text-green-500" : "text-red-500";
    const bgClass = type === "bid" ? "bg-green-500" : "bg-red-500";

    return (
        <div className="relative flex justify-between text-sm py-0.5 px-2 hover:bg-gray-800 cursor-pointer font-mono">
            <DepthBar total={total} maxTotal={maxTotal} color={bgClass} />
            <span className={`z-10 ${colorClass}`}>{price}</span>
            <span className="z-10 text-gray-300">{qty}</span>
            <span className="z-10 text-gray-500">{total}</span>
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
        return <div className="p-4 text-gray-500">Waiting for orderbook snapshot...</div>;
    }

    return (
        <div className="flex flex-col w-64 bg-gray-900 border border-gray-800 rounded">
            <div className="flex justify-between px-2 py-1 text-xs text-gray-500 border-b border-gray-800">
                <span>Price</span>
                <span>Size</span>
                <span>Total</span>
            </div>

            <div className="flex flex-col overflow-hidden">
                {/* Asks (Red) */}
                <div className="flex flex-col border-b border-gray-800 pb-1">
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
                <div className="py-2 text-center text-sm font-mono bg-gray-950 border-b border-gray-800">
                    <span className="text-gray-400">Spread</span>
                </div>

                {/* Bids (Green) */}
                <div className="flex flex-col pt-1">
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
