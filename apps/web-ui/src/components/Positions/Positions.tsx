// ---------------------------------------------------------------------------
// Positions — read-only position panel with live PnL
// ---------------------------------------------------------------------------
//
// Displays position size, entry price, mark price, unrealised PnL, and
// liquidation price. PnL is computed using decimal.js for precision:
//
//   Long PnL  = (mark − entry) × size
//   Short PnL = (entry − mark) × |size|   (size is negative for shorts)
//
// Mark price is read from the store's ticker state, so PnL updates live
// whenever a new ticker delta arrives.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import Decimal from "decimal.js";
import { useDexStore } from "../../state/StoreProvider";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Position {
    symbol: string;
    /** Positive = long, negative = short */
    size: string;
    entry_price: string;
    liquidation_price?: string;
}

export interface PositionsProps {
    /** Static positions to display (derived from account or passed in) */
    positions?: Position[];
}

// ---------------------------------------------------------------------------
// PnL computation (exported for unit tests)
// ---------------------------------------------------------------------------

/**
 * Compute unrealised PnL using decimal.js.
 *
 *   Long  (size > 0): PnL = (mark - entry) * size
 *   Short (size < 0): PnL = (mark - entry) * size   (size is already negative)
 *
 * Returns the PnL as a string decimal.
 */
export function computePnl(
    markPrice: string,
    entryPrice: string,
    size: string,
): string {
    const mark = new Decimal(markPrice);
    const entry = new Decimal(entryPrice);
    const qty = new Decimal(size);
    return mark.minus(entry).times(qty).toFixed();
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const Positions: React.FC<PositionsProps> = ({ positions = [] }) => {
    const { state } = useDexStore();

    // Enrich positions with live mark price + PnL
    const enriched = useMemo(() => {
        return positions.map((pos) => {
            const ticker = state.tickers.get(pos.symbol);
            const markPrice = ticker?.mark_price ?? pos.entry_price; // fallback to entry if no ticker
            const pnl = computePnl(markPrice, pos.entry_price, pos.size);
            return { ...pos, mark_price: markPrice, pnl };
        });
    }, [positions, state.tickers]);

    // ---- Render -------------------------------------------------------------

    return (
        <div id="positions-panel" className="glass-panel p-6 rounded-2xl w-full flex flex-col gap-4 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20">
            <h3 className="text-xl font-display font-bold tracking-tight text-white m-0 flex items-center gap-2">
                Positions
                {enriched.length > 0 && (
                    <span className="px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-400 text-xs font-semibold">
                        {enriched.length}
                    </span>
                )}
            </h3>

            {enriched.length === 0 ? (
                <div className="text-slate-500 py-8 text-center font-medium bg-slate-900/30 rounded-xl border border-indigo-500/10">
                    No open positions.
                </div>
            ) : (
                <div className="overflow-x-auto custom-scrollbar rounded-xl border border-indigo-500/10 bg-slate-900/40">
                    <table className="w-full text-left text-sm whitespace-nowrap">
                        <thead className="bg-slate-800/50 text-xs text-slate-500 uppercase tracking-wider">
                            <tr>
                                <th className="px-5 py-3 font-semibold">Symbol</th>
                                <th className="px-5 py-3 font-semibold">Size</th>
                                <th className="px-5 py-3 font-semibold">Entry</th>
                                <th className="px-5 py-3 font-semibold">Mark</th>
                                <th className="px-5 py-3 font-semibold">Unrealised PnL</th>
                                <th className="px-5 py-3 font-semibold">Liq. Price</th>
                            </tr>
                        </thead>
                        <tbody className="divide-y divide-indigo-500/10">
                            {enriched.map((pos) => {
                                const pnlNum = parseFloat(pos.pnl);
                                const isLong = parseFloat(pos.size) > 0;
                                return (
                                    <tr
                                        key={pos.symbol}
                                        className="hover:bg-indigo-500/5 transition-colors group cursor-default"
                                    >
                                        <td className="px-5 py-3 font-bold text-white tracking-wide">
                                            {pos.symbol}
                                        </td>
                                        <td className={`px-5 py-3 font-mono font-bold ${isLong ? "text-[#00E676]" : "text-[#FF1744]"}`}>
                                            {pos.size}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-300">
                                            {pos.entry_price}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-300 group-hover:text-white transition-colors">
                                            {pos.mark_price}
                                        </td>
                                        <td className={`px-5 py-3 font-mono font-bold tracking-wide flex items-center gap-1 ${pnlNum > 0 ? "text-[#00E676] text-glow-buy" : pnlNum < 0 ? "text-[#FF1744] text-glow-sell" : "text-slate-500"}`}>
                                            {pnlNum > 0 ? "+" : ""}
                                            {pos.pnl}
                                        </td>
                                        <td className="px-5 py-3 font-mono text-slate-500">
                                            {pos.liquidation_price ?? "—"}
                                        </td>
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
};
