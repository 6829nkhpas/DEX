// ---------------------------------------------------------------------------
// liquidation.ts — Pure liquidation price computation per spec §06
// ---------------------------------------------------------------------------
//
// Spec references:
//   - §06.2.1 Trigger: margin_ratio < 1.1
//   - §06.4.3 Bankruptcy price:
//       LONG:  bankruptcy = entry - (initial_margin / size)
//       SHORT: bankruptcy = entry + (initial_margin / |size|)
//   - §06.3.1 Partial liquidation: smallest position first
//
// All inputs/outputs are string-encoded decimals.
// Uses decimal.js; no floating-point.
// ---------------------------------------------------------------------------

import Decimal from "decimal.js";
import { calculateInitialMargin, calculateMaintenanceMargin } from "./margin";
import type { MarginPosition, MarginParams } from "./margin";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Account state needed for liquidation price computation. */
export interface LiquidationAccount {
  /** Total balance (collateral) */
  balance: string;
  /** All open positions (needed for cross-margin calculation) */
  positions: MarginPosition[];
}

/** Result of computing liquidation price for a single position. */
export interface LiquidationPriceResult {
  symbol: string;
  /** Price at which this position triggers liquidation (margin_ratio < 1.1) */
  liquidation_price: string;
  /** Bankruptcy price — at which account equity = 0 for this position */
  bankruptcy_price: string;
  /** Distance from current mark to liquidation (percentage) */
  distance_pct: string;
}

/** A cascade estimation entry — at what mark shift each position liquidates. */
export interface CascadeEntry {
  symbol: string;
  size: string;
  entry_price: string;
  liquidation_price: string;
  /** Mark price change needed to reach liquidation (absolute) */
  mark_delta_to_liquidation: string;
  /** Percentage from current mark */
  pct_from_current: string;
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/**
 * Compute the liquidation price for a single position under cross-margin.
 *
 * The liquidation price is where margin_ratio = 1.1 (the trigger threshold).
 *
 * For a single position in cross-margin:
 *   equity = balance + (liqPrice - entry) * size  (for all positions)
 *   maintenance_margin = |size| * liqPrice * mm_rate
 *   At liquidation: equity = 1.1 * maintenance_margin
 *
 * Solving for liqPrice (isolating one position, other positions at mark):
 *
 *   LONG (size > 0):
 *     liqPrice = (balance + otherPnl - 1.1 * otherMM - entry * size) /
 *                (size * (1.1 * mm_rate - 1))  — but this gives negative denom
 *     Rearranged:
 *     balance + (liqP - entry)*size + otherPnl = 1.1 * (|size|*liqP*mmr + otherMM)
 *     balance + liqP*size - entry*size + otherPnl = 1.1*|size|*liqP*mmr + 1.1*otherMM
 *     liqP*size - 1.1*|size|*mmr*liqP = 1.1*otherMM - balance + entry*size - otherPnl
 *     liqP * (size - 1.1*|size|*mmr) = entry*size - balance - otherPnl + 1.1*otherMM
 *     liqP = (entry*size - balance - otherPnl + 1.1*otherMM) / (size - 1.1*|size|*mmr)
 *
 *   SHORT (size < 0):
 *     Same formula works because size is negative.
 *
 * @param positionIndex - Index of the position to compute liquidation price for
 * @param account       - Account with all positions
 * @param params        - Leverage and MM rate
 */
export function computeLiquidationPrice(
  position: MarginPosition,
  account: LiquidationAccount,
  params: MarginParams,
): LiquidationPriceResult {
  const size = new Decimal(position.size);
  const entry = new Decimal(position.entry_price);
  const mark = new Decimal(position.mark_price);
  const mmRate = new Decimal(params.maintenance_margin_rate);
  const balance = new Decimal(account.balance);
  const TRIGGER = new Decimal("1.1");

  // Compute other positions' PnL and MM at their current mark prices
  let otherPnl = new Decimal(0);
  let otherMM = new Decimal(0);
  for (const p of account.positions) {
    if (p.symbol === position.symbol && p.size === position.size && p.entry_price === position.entry_price) {
      continue; // skip the target position
    }
    const pSize = new Decimal(p.size);
    const pEntry = new Decimal(p.entry_price);
    const pMark = new Decimal(p.mark_price);
    otherPnl = otherPnl.plus(pMark.minus(pEntry).times(pSize));
    otherMM = otherMM.plus(pSize.abs().times(pMark).times(mmRate));
  }

  // Solve: liqP = (entry*size - balance - otherPnl + TRIGGER*otherMM) / (size - TRIGGER*|size|*mmRate)
  const numerator = entry.times(size).minus(balance).minus(otherPnl).plus(TRIGGER.times(otherMM));
  const denominator = size.minus(TRIGGER.times(size.abs()).times(mmRate));

  let liqPrice: Decimal;
  if (denominator.isZero()) {
    // Degenerate case — set to 0 for long, very large for short
    liqPrice = size.gt(0) ? new Decimal(0) : new Decimal("999999999");
  } else {
    liqPrice = numerator.div(denominator);
  }

  // Clamp to 0 (price cannot be negative)
  if (liqPrice.lt(0)) {
    liqPrice = new Decimal(0);
  }

  // Bankruptcy price: equity = 0
  // LONG:  bankruptcy = entry - IM/|size|
  // SHORT: bankruptcy = entry + IM/|size|
  const im = new Decimal(calculateInitialMargin(position, params));
  let bankruptcy: Decimal;
  if (size.gt(0)) {
    bankruptcy = entry.minus(im.div(size.abs()));
  } else {
    bankruptcy = entry.plus(im.div(size.abs()));
  }
  if (bankruptcy.lt(0)) {
    bankruptcy = new Decimal(0);
  }

  // Distance percentage from current mark
  let distancePct: string;
  if (mark.isZero()) {
    distancePct = "0";
  } else {
    distancePct = liqPrice.minus(mark).div(mark).times(100).toDecimalPlaces(4).toFixed();
  }

  return {
    symbol: position.symbol,
    liquidation_price: liqPrice.toDecimalPlaces(18, Decimal.ROUND_DOWN).toFixed(),
    bankruptcy_price: bankruptcy.toDecimalPlaces(18, Decimal.ROUND_DOWN).toFixed(),
    distance_pct: distancePct,
  };
}

/**
 * Simulate a mark price change and return new PnL and margin usage.
 *
 * @param positions  - Current positions
 * @param markDelta  - Absolute change to apply to all mark prices (string decimal)
 * @param balance    - Account balance
 * @param params     - Margin parameters
 * @returns New metrics after the simulated mark change
 */
export function simulateMarkChange(
  positions: MarginPosition[],
  markDelta: string,
  balance: string,
  params: MarginParams,
): {
  positions: Array<MarginPosition & { pnl: string; initial_margin: string; maintenance_margin: string }>;
  total_pnl: string;
  total_initial_margin: string;
  total_maintenance_margin: string;
  equity: string;
  margin_ratio: string;
  health: "healthy" | "warning" | "danger" | "liquidation";
} {
  const delta = new Decimal(markDelta);
  let totalPnl = new Decimal(0);
  let totalIM = new Decimal(0);
  let totalMM = new Decimal(0);

  const newPositions = positions.map((pos) => {
    const newMark = new Decimal(pos.mark_price).plus(delta);
    const clampedMark = newMark.lt(0) ? new Decimal(0) : newMark;
    const newPos: MarginPosition = {
      ...pos,
      mark_price: clampedMark.toFixed(),
    };
    const pnl = new Decimal(newPos.mark_price).minus(new Decimal(newPos.entry_price)).times(new Decimal(newPos.size));
    const im = new Decimal(calculateInitialMargin(newPos, params));
    const mm = new Decimal(calculateMaintenanceMargin(newPos, params));

    totalPnl = totalPnl.plus(pnl);
    totalIM = totalIM.plus(im);
    totalMM = totalMM.plus(mm);

    return {
      ...newPos,
      pnl: pnl.toFixed(),
      initial_margin: im.toFixed(),
      maintenance_margin: mm.toFixed(),
    };
  });

  const equity = new Decimal(balance).plus(totalPnl);
  let marginRatio: string;
  if (totalMM.isZero()) {
    marginRatio = "Infinity";
  } else {
    marginRatio = equity.div(totalMM).toDecimalPlaces(6, Decimal.ROUND_DOWN).toFixed();
  }

  let health: "healthy" | "warning" | "danger" | "liquidation";
  if (marginRatio === "Infinity") {
    health = "healthy";
  } else {
    const mr = new Decimal(marginRatio);
    if (mr.gte("2.0")) health = "healthy";
    else if (mr.gte("1.5")) health = "warning";
    else if (mr.gte("1.1")) health = "danger";
    else health = "liquidation";
  }

  return {
    positions: newPositions,
    total_pnl: totalPnl.toFixed(),
    total_initial_margin: totalIM.toFixed(),
    total_maintenance_margin: totalMM.toFixed(),
    equity: equity.toFixed(),
    margin_ratio: marginRatio,
    health,
  };
}

/**
 * Estimate the liquidation cascade — at what mark price shift each position
 * would trigger liquidation. Returns entries sorted by absolute mark delta.
 *
 * @param positions - Current positions
 * @param account   - Account state
 * @param params    - Margin parameters
 */
export function estimateLiquidationCascade(
  positions: MarginPosition[],
  account: LiquidationAccount,
  params: MarginParams,
): CascadeEntry[] {
  const entries: CascadeEntry[] = [];

  for (const pos of positions) {
    const result = computeLiquidationPrice(pos, account, params);
    const mark = new Decimal(pos.mark_price);
    const liqP = new Decimal(result.liquidation_price);
    const delta = liqP.minus(mark);

    let pctFromCurrent: string;
    if (mark.isZero()) {
      pctFromCurrent = "0";
    } else {
      pctFromCurrent = delta.div(mark).times(100).toDecimalPlaces(4).toFixed();
    }

    entries.push({
      symbol: pos.symbol,
      size: pos.size,
      entry_price: pos.entry_price,
      liquidation_price: result.liquidation_price,
      mark_delta_to_liquidation: delta.toFixed(),
      pct_from_current: pctFromCurrent,
    });
  }

  // Sort by absolute mark delta ascending (closest to liquidation first)
  entries.sort((a, b) => {
    const absA = new Decimal(a.mark_delta_to_liquidation).abs();
    const absB = new Decimal(b.mark_delta_to_liquidation).abs();
    return absA.cmp(absB);
  });

  return entries;
}
