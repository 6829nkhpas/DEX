// ---------------------------------------------------------------------------
// margin.ts — Pure margin calculation functions per spec §05
// ---------------------------------------------------------------------------
//
// All inputs and outputs are string-encoded decimals (no floating-point).
// Uses decimal.js with ROUND_UP for margin calculations (favor safety).
// Functions are pure — no side effects, no state mutation.
//
// Spec references:
//   - §05.2.1 Initial Margin: (position_size × entry_price) / leverage
//   - §05.2.2 Maintenance Margin: position_value × mm_rate
//   - §05.3.3 Unrealized PnL: Σ (mark - entry) × size × direction
//   - §05.3  Margin Ratio: equity / maintenance_margin
//   - §05.4  Leverage tiers by position value
// ---------------------------------------------------------------------------

import Decimal from "decimal.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Parameters required for margin calculations. */
export interface MarginParams {
  /** User-selected leverage (e.g. "10") */
  leverage: string;
  /** Maintenance margin rate (e.g. "0.005" for 0.5%) */
  maintenance_margin_rate: string;
}

/** A position for margin calculation purposes. */
export interface MarginPosition {
  symbol: string;
  /** Positive = long, negative = short */
  size: string;
  /** Average entry price */
  entry_price: string;
  /** Current mark price */
  mark_price: string;
}

/** Aggregated account-level margin metrics. */
export interface AccountMarginMetrics {
  /** Sum of initial margin across all positions */
  total_initial_margin: string;
  /** Sum of maintenance margin across all positions */
  total_maintenance_margin: string;
  /** Sum of unrealised PnL across all positions */
  total_unrealized_pnl: string;
  /** equity = balance + unrealized_pnl */
  equity: string;
  /** free_margin = equity - total_initial_margin */
  free_margin: string;
  /** margin_ratio = equity / maintenance_margin (or "Infinity" if no MM) */
  margin_ratio: string;
  /** Health status per spec §05.3.3 */
  health: "healthy" | "warning" | "danger" | "liquidation";
}

// ---------------------------------------------------------------------------
// Leverage tier lookup (spec §05.4.1)
// ---------------------------------------------------------------------------

export interface LeverageTier {
  max_position_value: string; // upper bound (inclusive), "Infinity" for last
  max_leverage: string;
  initial_margin_rate: string;
  maintenance_margin_rate: string;
}

export const LEVERAGE_TIERS: LeverageTier[] = [
  { max_position_value: "50000", max_leverage: "125", initial_margin_rate: "0.008", maintenance_margin_rate: "0.004" },
  { max_position_value: "250000", max_leverage: "100", initial_margin_rate: "0.01", maintenance_margin_rate: "0.005" },
  { max_position_value: "1000000", max_leverage: "50", initial_margin_rate: "0.02", maintenance_margin_rate: "0.01" },
  { max_position_value: "5000000", max_leverage: "20", initial_margin_rate: "0.05", maintenance_margin_rate: "0.025" },
  { max_position_value: "20000000", max_leverage: "10", initial_margin_rate: "0.10", maintenance_margin_rate: "0.05" },
  { max_position_value: "Infinity", max_leverage: "5", initial_margin_rate: "0.20", maintenance_margin_rate: "0.10" },
];

/**
 * Look up the leverage tier for a given notional position value.
 * Returns the tier's margin rates. Position value should be absolute.
 */
export function getTierForValue(positionValue: string): LeverageTier {
  const val = new Decimal(positionValue).abs();
  for (const tier of LEVERAGE_TIERS) {
    if (tier.max_position_value === "Infinity" || val.lte(new Decimal(tier.max_position_value))) {
      return tier;
    }
  }
  // Fallback: last tier
  return LEVERAGE_TIERS[LEVERAGE_TIERS.length - 1];
}

// ---------------------------------------------------------------------------
// Core calculations
// ---------------------------------------------------------------------------

/**
 * Calculate initial margin for a position.
 *
 * Formula (spec §05.2.1):
 *   initial_margin = (|size| × entry_price) / leverage
 *
 * Rounds UP (favor safety per spec §05.9.2).
 *
 * @param position - Position with size and entry_price
 * @param params   - Leverage parameter
 * @returns Initial margin as string decimal
 */
export function calculateInitialMargin(
  position: Pick<MarginPosition, "size" | "entry_price">,
  params: Pick<MarginParams, "leverage">,
): string {
  const size = new Decimal(position.size).abs();
  const entry = new Decimal(position.entry_price);
  const leverage = new Decimal(params.leverage);

  if (leverage.isZero()) {
    throw new Error("Leverage cannot be zero");
  }
  if (size.isZero()) {
    return "0";
  }

  return size.times(entry).div(leverage).toDecimalPlaces(18, Decimal.ROUND_UP).toFixed();
}

/**
 * Calculate maintenance margin for a position.
 *
 * Formula (spec §05.2.2):
 *   maintenance_margin = |size| × mark_price × mm_rate
 *
 * Uses current mark price for position value (not entry).
 * Rounds UP (favor safety per spec §05.9.2).
 *
 * @param position - Position with size and mark_price
 * @param params   - Maintenance margin rate
 * @returns Maintenance margin as string decimal
 */
export function calculateMaintenanceMargin(
  position: Pick<MarginPosition, "size" | "mark_price">,
  params: Pick<MarginParams, "maintenance_margin_rate">,
): string {
  const size = new Decimal(position.size).abs();
  const mark = new Decimal(position.mark_price);
  const mmRate = new Decimal(params.maintenance_margin_rate);

  if (size.isZero()) {
    return "0";
  }

  return size.times(mark).times(mmRate).toDecimalPlaces(18, Decimal.ROUND_UP).toFixed();
}

/**
 * Compute unrealized PnL for a single position.
 *
 * Formula (spec §05.3.3):
 *   pnl = (mark_price - entry_price) × size
 *   (size is + for long, - for short, so the sign handles direction)
 *
 * @returns Unrealized PnL as string decimal
 */
export function computeUnrealizedPnl(position: Pick<MarginPosition, "size" | "entry_price" | "mark_price">): string {
  const mark = new Decimal(position.mark_price);
  const entry = new Decimal(position.entry_price);
  const size = new Decimal(position.size);

  return mark.minus(entry).times(size).toFixed();
}

/**
 * Compute aggregated account-level margin metrics.
 *
 * @param positions - All open positions
 * @param balance   - Total account balance (string decimal)
 * @param params    - Per-position leverage and MM rate
 * @returns Aggregated metrics
 */
export function computeAccountMetrics(
  positions: MarginPosition[],
  balance: string,
  params: MarginParams,
): AccountMarginMetrics {
  let totalIM = new Decimal(0);
  let totalMM = new Decimal(0);
  let totalPnl = new Decimal(0);

  for (const pos of positions) {
    totalIM = totalIM.plus(new Decimal(calculateInitialMargin(pos, params)));
    totalMM = totalMM.plus(new Decimal(calculateMaintenanceMargin(pos, params)));
    totalPnl = totalPnl.plus(new Decimal(computeUnrealizedPnl(pos)));
  }

  const bal = new Decimal(balance);
  const equity = bal.plus(totalPnl);
  const freeMargin = equity.minus(totalIM);

  let marginRatio: string;
  if (totalMM.isZero()) {
    marginRatio = "Infinity";
  } else {
    marginRatio = equity.div(totalMM).toDecimalPlaces(6, Decimal.ROUND_DOWN).toFixed();
  }

  // Health per spec §05.3.3
  let health: AccountMarginMetrics["health"];
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
    total_initial_margin: totalIM.toFixed(),
    total_maintenance_margin: totalMM.toFixed(),
    total_unrealized_pnl: totalPnl.toFixed(),
    equity: equity.toFixed(),
    free_margin: freeMargin.toFixed(),
    margin_ratio: marginRatio,
    health,
  };
}
