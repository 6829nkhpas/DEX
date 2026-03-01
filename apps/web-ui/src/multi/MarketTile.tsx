// ---------------------------------------------------------------------------
// MarketTile — isolated, memoized tile for a single symbol in the grid
// ---------------------------------------------------------------------------

import React from "react";
import type { TickerState } from "../state/types";

export interface MarketTileProps {
  symbol: string;
  ticker: TickerState | undefined;
  isFocused: boolean;
  onSelect: (symbol: string) => void;
}

/**
 * A single market tile rendering ticker data. Memoized so it only re-renders
 * when its own ticker data or focus state changes — not when other symbols update.
 */
export const MarketTile: React.FC<MarketTileProps> = React.memo(
  ({ symbol, ticker, isFocused, onSelect }) => {
    const handleClick = React.useCallback(() => {
      onSelect(symbol);
    }, [symbol, onSelect]);

    return (
      <div
        onClick={handleClick}
        className={`
                    flex flex-col p-3 rounded border cursor-pointer transition-colors
                    ${isFocused
            ? "border-blue-500 bg-gray-800"
            : "border-gray-700 bg-gray-900 hover:border-gray-600"
          }
                `}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") handleClick(); }}
        aria-label={`Select market ${symbol}`}
        aria-pressed={isFocused}
      >
        <div className="flex items-center justify-between mb-1">
          <span className="text-sm font-bold text-white truncate">{symbol}</span>
          {isFocused && (
            <span className="text-xs bg-blue-600 text-white px-1.5 py-0.5 rounded">
              FOCUS
            </span>
          )}
        </div>

        {ticker ? (
          <>
            <span className="text-lg font-mono text-white">{ticker.last_price}</span>
            <div className="flex justify-between text-xs text-gray-400 mt-1 font-mono">
              <span>V: {ticker.volume_24h}</span>
              <span>M: {ticker.mark_price}</span>
            </div>
            <div className="flex justify-between text-xs text-gray-500 mt-0.5 font-mono">
              <span>H: {ticker.high_24h}</span>
              <span>L: {ticker.low_24h}</span>
            </div>
          </>
        ) : (
          <span className="text-xs text-gray-500 mt-1">Awaiting data…</span>
        )}
      </div>
    );
  },
  // Custom comparator: only re-render when ticker data or focus changes
  (prev, next) => {
    if (prev.isFocused !== next.isFocused) return false;
    if (prev.symbol !== next.symbol) return false;
    if (prev.ticker === next.ticker) return true;
    if (!prev.ticker || !next.ticker) return false;
    return (
      prev.ticker.last_price === next.ticker.last_price &&
      prev.ticker.volume_24h === next.ticker.volume_24h &&
      prev.ticker.high_24h === next.ticker.high_24h &&
      prev.ticker.low_24h === next.ticker.low_24h &&
      prev.ticker.mark_price === next.ticker.mark_price &&
      prev.ticker.lastSeq === next.ticker.lastSeq
    );
  }
);

MarketTile.displayName = "MarketTile";
