// ---------------------------------------------------------------------------
// MarketGrid — Virtualized grid for rendering 25+ market tiles efficiently
// ---------------------------------------------------------------------------
//
// Uses a lightweight virtualization approach (no external library dependency):
//   - Only renders tiles visible within the viewport + overscan buffer
//   - Each tile is isolated via React.memo with custom comparator
//   - Scroll position drives which tiles are rendered
//   - Grid dimensions are calculated from container width
//
// Acceptance: Render 25 symbols at 50 msg/sec each without freeze.
// ---------------------------------------------------------------------------

import React, { useState, useRef, useCallback, useMemo, useEffect } from "react";
import { MarketTile } from "./MarketTile";
import type { TickerState } from "../state/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MarketGridProps {
  /** All symbols to display in the grid. */
  symbols: string[];
  /** Map of symbol → ticker data (from store state). */
  tickers: ReadonlyMap<string, TickerState>;
  /** Currently focused symbol. */
  focusedSymbol: string | null;
  /** Callback when user selects a symbol tile. */
  onSelectSymbol: (symbol: string) => void;
  /** Fixed tile height in pixels. Default: 110 */
  tileHeight?: number;
  /** Fixed tile width in pixels. Default: 200 */
  tileWidth?: number;
  /** Gap between tiles in pixels. Default: 8 */
  gap?: number;
  /** Number of extra rows to render outside viewport. Default: 2 */
  overscan?: number;
  /** CSS class for the outer container. */
  className?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_TILE_HEIGHT = 110;
const DEFAULT_TILE_WIDTH = 200;
const DEFAULT_GAP = 8;
const DEFAULT_OVERSCAN = 2;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const MarketGrid: React.FC<MarketGridProps> = ({
  symbols,
  tickers,
  focusedSymbol,
  onSelectSymbol,
  tileHeight = DEFAULT_TILE_HEIGHT,
  tileWidth = DEFAULT_TILE_WIDTH,
  gap = DEFAULT_GAP,
  overscan = DEFAULT_OVERSCAN,
  className = "",
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerWidth, setContainerWidth] = useState(800);
  const [containerHeight, setContainerHeight] = useState(600);

  // Calculate grid layout
  const columns = useMemo(
    () => Math.max(1, Math.floor((containerWidth + gap) / (tileWidth + gap))),
    [containerWidth, tileWidth, gap]
  );

  const rowHeight = tileHeight + gap;
  const totalRows = Math.ceil(symbols.length / columns);
  const totalHeight = totalRows * rowHeight - gap; // no trailing gap

  // Determine visible row range with overscan
  const startRow = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const visibleRows = Math.ceil(containerHeight / rowHeight) + 2 * overscan;
  const endRow = Math.min(totalRows, startRow + visibleRows);

  // Visible symbols slice
  const visibleItems = useMemo(() => {
    const items: { symbol: string; index: number }[] = [];
    for (let row = startRow; row < endRow; row++) {
      for (let col = 0; col < columns; col++) {
        const idx = row * columns + col;
        if (idx < symbols.length) {
          items.push({ symbol: symbols[idx], index: idx });
        }
      }
    }
    return items;
  }, [symbols, startRow, endRow, columns]);

  // Scroll handler with passive event
  const handleScroll = useCallback(() => {
    if (containerRef.current) {
      setScrollTop(containerRef.current.scrollTop);
    }
  }, []);

  // Observe container resize
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerWidth(entry.contentRect.width);
        setContainerHeight(entry.contentRect.height);
      }
    });
    observer.observe(el);

    // Initialize from current size
    setContainerWidth(el.clientWidth);
    setContainerHeight(el.clientHeight);

    return () => observer.disconnect();
  }, []);

  // Extract ticker for each visible item (stable reference via useMemo)
  const tickerLookup = useCallback(
    (symbol: string): TickerState | undefined => tickers.get(symbol),
    [tickers]
  );

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className={`overflow-y-auto ${className}`}
      style={{ position: "relative" }}
      role="grid"
      aria-label="Market grid"
    >
      {/* Spacer for total scrollable height */}
      <div style={{ height: totalHeight, position: "relative" }}>
        {visibleItems.map(({ symbol, index }) => {
          const row = Math.floor(index / columns);
          const col = index % columns;
          const top = row * rowHeight;
          const left = col * (tileWidth + gap);

          return (
            <div
              key={symbol}
              style={{
                position: "absolute",
                top,
                left,
                width: tileWidth,
                height: tileHeight,
              }}
              role="gridcell"
            >
              <MarketTile
                symbol={symbol}
                ticker={tickerLookup(symbol)}
                isFocused={focusedSymbol === symbol}
                onSelect={onSelectSymbol}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
};

MarketGrid.displayName = "MarketGrid";
