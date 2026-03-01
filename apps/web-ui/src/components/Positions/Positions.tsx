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
        <div
            id="positions-panel"
            style={{
                background: "#111827",
                border: "1px solid #374151",
                borderRadius: 8,
                padding: 16,
                fontFamily: "Inter, system-ui, sans-serif",
                color: "#e5e7eb",
                width: "100%",
            }}
        >
            <h3 style={{ margin: "0 0 12px", fontSize: 16, fontWeight: 600 }}>
                Positions
                {enriched.length > 0 && (
                    <span
                        style={{
                            marginLeft: 8,
                            fontSize: 12,
                            color: "#9ca3af",
                            fontWeight: 400,
                        }}
                    >
                        ({enriched.length})
                    </span>
                )}
            </h3>

            {enriched.length === 0 && (
                <div style={{ color: "#6b7280", fontSize: 13 }}>
                    No open positions.
                </div>
            )}

            {enriched.length > 0 && (
                <div style={{ overflowX: "auto" }}>
                    <table
                        style={{
                            width: "100%",
                            fontSize: 12,
                            borderCollapse: "collapse",
                            whiteSpace: "nowrap",
                        }}
                    >
                        <thead>
                            <tr style={{ borderBottom: "1px solid #374151" }}>
                                <th style={thStyle}>Symbol</th>
                                <th style={thStyle}>Size</th>
                                <th style={thStyle}>Entry</th>
                                <th style={thStyle}>Mark</th>
                                <th style={thStyle}>Unrealised PnL</th>
                                <th style={thStyle}>Liq. Price</th>
                            </tr>
                        </thead>
                        <tbody>
                            {enriched.map((pos) => {
                                const pnlNum = parseFloat(pos.pnl);
                                const isLong = parseFloat(pos.size) > 0;
                                return (
                                    <tr
                                        key={pos.symbol}
                                        style={{
                                            borderBottom: "1px solid #1f2937",
                                        }}
                                    >
                                        <td style={tdStyle}>{pos.symbol}</td>
                                        <td
                                            style={{
                                                ...tdStyle,
                                                color: isLong
                                                    ? "#10b981"
                                                    : "#ef4444",
                                                fontWeight: 600,
                                            }}
                                        >
                                            {pos.size}
                                        </td>
                                        <td style={tdStyle}>
                                            {pos.entry_price}
                                        </td>
                                        <td style={tdStyle}>
                                            {pos.mark_price}
                                        </td>
                                        <td
                                            style={{
                                                ...tdStyle,
                                                color:
                                                    pnlNum > 0
                                                        ? "#10b981"
                                                        : pnlNum < 0
                                                            ? "#ef4444"
                                                            : "#9ca3af",
                                                fontWeight: 600,
                                            }}
                                        >
                                            {pnlNum > 0 ? "+" : ""}
                                            {pos.pnl}
                                        </td>
                                        <td style={tdStyle}>
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

// ---------------------------------------------------------------------------
// Inline styles
// ---------------------------------------------------------------------------

const thStyle: React.CSSProperties = {
    textAlign: "left",
    padding: "6px 8px",
    color: "#9ca3af",
    fontWeight: 500,
};

const tdStyle: React.CSSProperties = {
    padding: "6px 8px",
};
