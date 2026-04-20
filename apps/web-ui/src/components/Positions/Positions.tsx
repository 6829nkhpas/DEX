// ---------------------------------------------------------------------------
// Positions — read-only position panel with live PnL
// ---------------------------------------------------------------------------
// Phase 15: LONG/SHORT label badges, PnL background tint, liquidation
//           proximity warning, consistent panel structure, EmptyState.
// ---------------------------------------------------------------------------

import React, { useMemo } from "react";
import Decimal from "decimal.js";
import { useDexStore } from "../../state/StoreProvider";
import { EmptyState } from "../ui/EmptyState";

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

/**
 * Compute proximity of mark price to liquidation price as 0..1 (1 = at liq.).
 * Returns null if no liquidation price or insufficient data.
 */
export function liquidationProximity(
    markPrice: string,
    entryPrice: string,
    liquidationPrice: string | undefined,
): number | null {
    if (!liquidationPrice) return null;
    try {
        const mark = new Decimal(markPrice);
        const entry = new Decimal(entryPrice);
        const liq = new Decimal(liquidationPrice);
        const totalRange = entry.minus(liq).abs();
        if (totalRange.isZero()) return 1;
        const currentDist = mark.minus(liq).abs();
        const proximity = new Decimal(1).minus(currentDist.div(totalRange));
        return Math.max(0, Math.min(1, proximity.toNumber()));
    } catch {
        return null;
    }
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
            const liqProx = liquidationProximity(markPrice, pos.entry_price, pos.liquidation_price);
            return { ...pos, mark_price: markPrice, pnl, liqProx };
        });
    }, [positions, state.tickers]);

    // ---- Render -------------------------------------------------------------

    return (
        <div id="positions-panel" className="glass-panel p-6 rounded-2xl w-full flex flex-col gap-4 text-slate-200 shadow-2xl relative overflow-hidden border-t border-indigo-500/20" style={{ gridArea: "positions" }}>
            <div className="flex items-center justify-between">
                <span className="panel-header">
                    Positions
                    {enriched.length > 0 && (
                        <span className="panel-count">{enriched.length}</span>
                    )}
                </span>
            </div>

            {enriched.length === 0 ? (
                <EmptyState message="No open positions" icon="chart" />
            ) : (
                <div className="overflow-x-auto custom-scrollbar rounded-xl border border-indigo-500/10 bg-slate-900/40">
                    <table className="data-table">
                        <thead>
                            <tr>
                                <th>Symbol</th>
                                <th>Side</th>
                                <th>Size</th>
                                <th>Entry</th>
                                <th>Mark</th>
                                <th>Unrealised PnL</th>
                                <th>Liq. Price</th>
                            </tr>
                        </thead>
                        <tbody>
                            {enriched.map((pos) => {
                                const pnlNum = parseFloat(pos.pnl);
                                const isLong = parseFloat(pos.size) > 0;
                                const pnlBgClass = pnlNum > 0
                                    ? "bg-emerald-500/5"
                                    : pnlNum < 0
                                        ? "bg-rose-500/5"
                                        : "";

                                // Liquidation warning: amber at >50% proximity, red at >80%
                                const liqWarning = pos.liqProx !== null
                                    ? pos.liqProx > 0.8
                                        ? "text-red-400 font-bold"
                                        : pos.liqProx > 0.5
                                            ? "text-amber-400 font-semibold"
                                            : "text-slate-500"
                                    : "text-slate-500";

                                return (
                                    <tr key={pos.symbol} className="cursor-default group">
                                        <td className="font-bold text-white tracking-wide">
                                            {pos.symbol}
                                        </td>
                                        <td>
                                            <span className={`status-badge ${isLong ? "status-badge-success" : "status-badge-error"}`}>
                                                {isLong ? "LONG" : "SHORT"}
                                            </span>
                                        </td>
                                        <td className={`font-mono font-bold tabular-nums ${isLong ? "text-[#00E676]" : "text-[#FF1744]"}`}>
                                            {pos.size}
                                        </td>
                                        <td className="font-mono text-slate-300 tabular-nums">
                                            {pos.entry_price}
                                        </td>
                                        <td className="font-mono text-slate-300 group-hover:text-white transition-colors tabular-nums">
                                            {pos.mark_price}
                                        </td>
                                        <td className={`font-mono font-bold tracking-wide tabular-nums ${pnlBgClass} ${pnlNum > 0 ? "text-[#00E676] text-glow-buy" : pnlNum < 0 ? "text-[#FF1744] text-glow-sell" : "text-slate-500"}`}>
                                            {pnlNum > 0 ? "+" : ""}
                                            {pos.pnl}
                                        </td>
                                        <td className={`font-mono tabular-nums ${liqWarning}`}>
                                            {pos.liquidation_price ?? "—"}
                                            {pos.liqProx !== null && pos.liqProx > 0.5 && (
                                                <svg className="w-3 h-3 inline ml-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                                                        d="M12 9v2m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                                                </svg>
                                            )}
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
