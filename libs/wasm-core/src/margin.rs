//! Margin Preview — Simulate margin effects of hypothetical orders
//!
//! Implements spec §5 (Margin Methodology) for client-side margin previews.
//! All calculations are deterministic: fixed-point `Decimal`, no system calls,
//! sorted iteration via `BTreeMap` per spec §12.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use types::ids::AccountId;
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::position::{Position, PositionSide};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Internal precision (spec §12, 18 dp).
const INTERNAL_DP: u32 = 18;

/// Display precision (8 dp).
const DISPLAY_DP: u32 = 8;

/// Liquidation trigger threshold (spec §5.3.3: margin_ratio < 1.1).
const LIQUIDATION_THRESHOLD: &str = "1.1";

/// Danger threshold (spec §5.3.3).
const DANGER_THRESHOLD: &str = "1.5";

/// Warning threshold (spec §5.3.3).
const WARNING_THRESHOLD: &str = "2.0";

// ---------------------------------------------------------------------------
// Maintenance margin rate table (spec §5.4.1)
// ---------------------------------------------------------------------------

/// Return maintenance margin rate given leverage tier.
fn maintenance_margin_rate(leverage: u8) -> Decimal {
    match leverage {
        1..=10 => Decimal::from_str_exact("0.005").unwrap(),   // 0.5%
        11..=20 => Decimal::from_str_exact("0.01").unwrap(),   // 1.0%
        21..=50 => Decimal::from_str_exact("0.02").unwrap(),   // 2.0%
        51..=100 => Decimal::from_str_exact("0.05").unwrap(),  // 5.0%
        101..=125 => Decimal::from_str_exact("0.10").unwrap(), // 10.0%
        _ => Decimal::from_str_exact("0.10").unwrap(),         // max
    }
}

// ---------------------------------------------------------------------------
// Risk level enum
// ---------------------------------------------------------------------------

/// Risk level derived from margin ratio thresholds (spec §5.3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// margin_ratio >= 2.0
    Healthy,
    /// 1.5 <= margin_ratio < 2.0
    Warning,
    /// 1.1 <= margin_ratio < 1.5
    Danger,
    /// margin_ratio < 1.1
    Liquidation,
}

/// Derive risk level from a margin ratio.
pub fn risk_level_from_ratio(margin_ratio: Decimal) -> RiskLevel {
    let liq = Decimal::from_str_exact(LIQUIDATION_THRESHOLD).unwrap();
    let danger = Decimal::from_str_exact(DANGER_THRESHOLD).unwrap();
    let warning = Decimal::from_str_exact(WARNING_THRESHOLD).unwrap();

    if margin_ratio < liq {
        RiskLevel::Liquidation
    } else if margin_ratio < danger {
        RiskLevel::Danger
    } else if margin_ratio < warning {
        RiskLevel::Warning
    } else {
        RiskLevel::Healthy
    }
}

// ---------------------------------------------------------------------------
// Margin mode
// ---------------------------------------------------------------------------

/// Margin mode (spec §5.2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarginMode {
    /// Shared collateral pool across all positions
    Cross,
    /// Per-position isolated margin (stub)
    Isolated,
}

// ---------------------------------------------------------------------------
// Margin preview result
// ---------------------------------------------------------------------------

/// Result of a margin simulation — what would happen if the order executed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarginPreview {
    /// Equity after hypothetical trade
    pub equity_after: Decimal,
    /// Total margin used after trade
    pub margin_used_after: Decimal,
    /// Available margin after trade
    pub margin_available_after: Decimal,
    /// Margin ratio after trade
    pub margin_ratio_after: Decimal,
    /// Estimated liquidation price for the resulting position
    pub liquidation_price: Decimal,
    /// Effective leverage after trade
    pub leverage_ratio: Decimal,
    /// Risk classification after trade
    pub risk_level: RiskLevel,
    /// Whether any computed balance would become negative
    pub has_negative_balance: bool,
}

// ---------------------------------------------------------------------------
// Cross-margin engine
// ---------------------------------------------------------------------------

/// Cross-margin preview engine.
///
/// Holds the account snapshot and computes margin previews for hypothetical
/// orders without mutating the underlying state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossMarginEngine {
    pub account_id: AccountId,
    /// Total account balance (quote currency)
    pub total_balance: Decimal,
    /// Existing positions keyed by symbol (sorted)
    pub positions: BTreeMap<String, Position>,
}

impl CrossMarginEngine {
    /// Create a new engine from an account snapshot.
    pub fn new(account_id: AccountId, total_balance: Decimal) -> Self {
        Self {
            account_id,
            total_balance,
            positions: BTreeMap::new(),
        }
    }

    /// Add an existing position to the snapshot.
    pub fn add_position(&mut self, position: Position) {
        self.positions
            .insert(position.symbol.as_str().to_owned(), position);
    }

    // -- core queries ------------------------------------------------------

    /// Total unrealized PnL across all positions.
    pub fn total_unrealized_pnl(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for pos in self.positions.values() {
            total += unrealized_pnl(pos);
        }
        round_internal(total)
    }

    /// Equity = total_balance + unrealized_pnl (spec §5.3.2).
    pub fn equity(&self) -> Decimal {
        round_display(self.total_balance + self.total_unrealized_pnl())
    }

    /// Total maintenance margin across all positions.
    pub fn total_maintenance_margin(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for pos in self.positions.values() {
            let pv = position_value(pos);
            let rate = maintenance_margin_rate(pos.leverage);
            total += round_up(pv * rate, INTERNAL_DP);
        }
        round_display(total)
    }

    /// Total initial margin used across all positions.
    pub fn total_initial_margin(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for pos in self.positions.values() {
            total += pos.initial_margin;
        }
        round_display(total)
    }

    /// Margin available = equity − margin_used (spec §5.3.1).
    /// Rounded DOWN conservatively.
    pub fn margin_available(&self) -> Decimal {
        let avail = self.equity() - self.total_initial_margin();
        round_down(avail, DISPLAY_DP)
    }

    /// Margin ratio = equity / maintenance_margin (spec §5.3.3).
    pub fn margin_ratio(&self) -> Decimal {
        let mm = self.total_maintenance_margin();
        if mm == Decimal::ZERO {
            return Decimal::MAX;
        }
        round_display(self.equity() / mm)
    }

    /// Current risk level.
    pub fn risk_level(&self) -> RiskLevel {
        risk_level_from_ratio(self.margin_ratio())
    }

    // -- simulation --------------------------------------------------------

    /// Simulate the effect of a hypothetical new order.
    ///
    /// Returns a `MarginPreview` describing the margin state *after* the
    /// order fills completely.  Does **not** mutate `self`.
    pub fn simulate_order(
        &self,
        _symbol: &str,
        side: Side,
        price: Price,
        quantity: Quantity,
        leverage: u8,
    ) -> MarginPreview {
        let price_dec = price.as_decimal();
        let qty_dec = quantity.as_decimal();
        let notional = round_internal(price_dec * qty_dec);

        // New initial margin for the order (rounded UP for safety)
        let new_im = round_up(
            notional / Decimal::from(leverage),
            INTERNAL_DP,
        );

        // New maintenance margin
        let mm_rate = maintenance_margin_rate(leverage);
        let new_mm = round_up(notional * mm_rate, INTERNAL_DP);

        // Aggregate existing margins + new order
        let total_im_after = round_display(self.total_initial_margin() + new_im);
        let total_mm_after = round_display(self.total_maintenance_margin() + new_mm);

        // Hypothetical unrealized PnL stays the same until mark moves
        let equity_after = round_display(self.total_balance + self.total_unrealized_pnl());

        let margin_available_after = round_down(equity_after - total_im_after, DISPLAY_DP);

        let margin_ratio_after = if total_mm_after == Decimal::ZERO {
            Decimal::MAX
        } else {
            round_display(equity_after / total_mm_after)
        };

        let leverage_ratio = if total_im_after == Decimal::ZERO {
            Decimal::ZERO
        } else {
            let total_notional = self.total_position_value() + notional;
            round_display(total_notional / equity_after)
        };

        let liq_price = compute_liquidation_price(
            side_to_position_side(side),
            price_dec,
            leverage,
            mm_rate,
        );

        MarginPreview {
            equity_after,
            margin_used_after: total_im_after,
            margin_available_after,
            margin_ratio_after,
            liquidation_price: round_display(liq_price),
            leverage_ratio,
            risk_level: risk_level_from_ratio(margin_ratio_after),
            has_negative_balance: margin_available_after < Decimal::ZERO,
        }
    }

    /// Total notional value of existing positions.
    fn total_position_value(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for pos in self.positions.values() {
            total += position_value(pos);
        }
        round_internal(total)
    }
}

// ---------------------------------------------------------------------------
// Isolated margin stub (spec says user can opt-in; stub for now)
// ---------------------------------------------------------------------------

/// Placeholder for future isolated-margin support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolatedMarginEngine {
    pub account_id: AccountId,
}

impl IsolatedMarginEngine {
    pub fn new(account_id: AccountId) -> Self {
        Self { account_id }
    }

    /// Isolated margin simulation is not yet implemented.
    pub fn simulate_order(
        &self,
        _symbol: &str,
        _side: Side,
        _price: Price,
        _quantity: Quantity,
        _leverage: u8,
    ) -> MarginPreview {
        unimplemented!("Isolated margin mode is not yet supported")
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Compute unrealized PnL for a single position (spec §4.4.3).
fn unrealized_pnl(pos: &Position) -> Decimal {
    let size = pos.size.as_decimal();
    let entry = pos.entry_price.as_decimal();
    let mark = pos.mark_price.as_decimal();
    match pos.side {
        PositionSide::LONG => (mark - entry) * size,
        PositionSide::SHORT => (entry - mark) * size,
    }
}

/// Position notional value.
fn position_value(pos: &Position) -> Decimal {
    pos.entry_price.as_decimal() * pos.size.as_decimal()
}

/// Compute estimated liquidation price (spec §5, derived).
///
/// LONG:  liq_price = entry × (1 − 1/leverage + mm_rate)
/// SHORT: liq_price = entry × (1 + 1/leverage − mm_rate)
fn compute_liquidation_price(
    side: PositionSide,
    entry_price: Decimal,
    leverage: u8,
    mm_rate: Decimal,
) -> Decimal {
    let one = Decimal::ONE;
    let lev_inv = one / Decimal::from(leverage);
    match side {
        PositionSide::LONG => entry_price * (one - lev_inv + mm_rate),
        PositionSide::SHORT => entry_price * (one + lev_inv - mm_rate),
    }
}

/// Convert order `Side` to `PositionSide`.
fn side_to_position_side(side: Side) -> PositionSide {
    match side {
        Side::BUY => PositionSide::LONG,
        Side::SELL => PositionSide::SHORT,
    }
}

/// Round to internal precision, HALF_UP.
fn round_internal(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(INTERNAL_DP, RoundingStrategy::MidpointAwayFromZero)
}

/// Round to display precision, HALF_UP.
fn round_display(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(DISPLAY_DP, RoundingStrategy::MidpointAwayFromZero)
}

/// Round UP (away from zero) — used for margin requirements (spec §12.9.2).
fn round_up(v: Decimal, dp: u32) -> Decimal {
    v.round_dp_with_strategy(dp, RoundingStrategy::AwayFromZero)
}

/// Round DOWN — used for available margin (conservative; spec §12.9.2).
fn round_down(v: Decimal, dp: u32) -> Decimal {
    v.round_dp_with_strategy(dp, RoundingStrategy::ToZero)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{AccountId, MarketId};
    use types::numeric::{Price, Quantity};

    fn make_engine() -> CrossMarginEngine {
        let account_id = AccountId::new();
        let mut engine = CrossMarginEngine::new(account_id, Decimal::from(100_000));

        // Existing position: Long 2 BTC @ 50 000, mark 51 000, leverage 10
        let pos = Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("2.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Price::from_u64(49_500),
            Decimal::from(10_000), // IM = 100k / 10
            Decimal::from(500),
            10,
            1_708_123_456_789_000_000,
        );
        engine.add_position(pos);
        engine
    }

    #[test]
    fn test_unrealized_pnl() {
        let engine = make_engine();
        // Long 2 BTC: (51000 - 50000) × 2 = 2000
        assert_eq!(engine.total_unrealized_pnl(), Decimal::from(2_000));
    }

    #[test]
    fn test_equity() {
        let engine = make_engine();
        // 100 000 + 2 000 = 102 000
        assert_eq!(engine.equity(), Decimal::from(102_000));
    }

    #[test]
    fn test_margin_ratio() {
        let engine = make_engine();
        // MM = 100 000 × 0.005 = 500 (position value 100k, leverage 10)
        // equity = 102 000, ratio = 102000 / 500 = 204
        let ratio = engine.margin_ratio();
        assert_eq!(ratio, Decimal::from(204));
    }

    #[test]
    fn test_risk_level_healthy() {
        let engine = make_engine();
        assert_eq!(engine.risk_level(), RiskLevel::Healthy);
    }

    #[test]
    fn test_risk_level_warning() {
        let level = risk_level_from_ratio(Decimal::from_str_exact("1.8").unwrap());
        assert_eq!(level, RiskLevel::Warning);
    }

    #[test]
    fn test_risk_level_danger() {
        let level = risk_level_from_ratio(Decimal::from_str_exact("1.3").unwrap());
        assert_eq!(level, RiskLevel::Danger);
    }

    #[test]
    fn test_risk_level_liquidation() {
        let level = risk_level_from_ratio(Decimal::from_str_exact("1.05").unwrap());
        assert_eq!(level, RiskLevel::Liquidation);
    }

    #[test]
    fn test_simulate_order() {
        let engine = make_engine();

        let preview = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );

        // New notional = 3000 × 10 = 30 000
        // New IM = 30 000 / 20 = 1 500
        // Total IM = 10 000 + 1 500 = 11 500
        assert_eq!(preview.margin_used_after, Decimal::from(11_500));

        // equity_after still = 102 000 (mark hasn't moved)
        assert_eq!(preview.equity_after, Decimal::from(102_000));

        assert!(!preview.has_negative_balance);
        assert_eq!(preview.risk_level, RiskLevel::Healthy);
    }

    #[test]
    fn test_simulate_order_negative_balance() {
        let account_id = AccountId::new();
        // Very small balance
        let engine = CrossMarginEngine::new(account_id, Decimal::from(100));

        let preview = engine.simulate_order(
            "BTC/USDT",
            Side::BUY,
            Price::from_u64(50_000),
            Quantity::from_str("1.0").unwrap(),
            10,
        );

        // IM = 5 000, balance = 100 → margin_available = 100 − 5000 < 0
        assert!(preview.has_negative_balance);
    }

    #[test]
    fn test_liquidation_price_long() {
        let liq = compute_liquidation_price(
            PositionSide::LONG,
            Decimal::from(50_000),
            10,
            Decimal::from_str_exact("0.005").unwrap(),
        );
        // entry × (1 − 1/10 + 0.005) = 50000 × 0.905 = 45 250
        assert_eq!(round_display(liq), Decimal::from(45_250));
    }

    #[test]
    fn test_liquidation_price_short() {
        let liq = compute_liquidation_price(
            PositionSide::SHORT,
            Decimal::from(50_000),
            10,
            Decimal::from_str_exact("0.005").unwrap(),
        );
        // entry × (1 + 1/10 − 0.005) = 50000 × 1.095 = 54 750
        assert_eq!(round_display(liq), Decimal::from(54_750));
    }

    #[test]
    fn test_leverage_ratio() {
        let engine = make_engine();
        let preview = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );

        // Total notional = 100 000 (existing) + 30 000 (new) = 130 000
        // equity = 102 000
        // leverage_ratio = 130 000 / 102 000 ≈ 1.27
        assert!(preview.leverage_ratio > Decimal::ONE);
        assert!(preview.leverage_ratio < Decimal::TWO);
    }

    #[test]
    fn test_maintenance_margin_rate_tiers() {
        assert_eq!(maintenance_margin_rate(5), Decimal::from_str_exact("0.005").unwrap());
        assert_eq!(maintenance_margin_rate(15), Decimal::from_str_exact("0.01").unwrap());
        assert_eq!(maintenance_margin_rate(30), Decimal::from_str_exact("0.02").unwrap());
        assert_eq!(maintenance_margin_rate(75), Decimal::from_str_exact("0.05").unwrap());
        assert_eq!(maintenance_margin_rate(110), Decimal::from_str_exact("0.10").unwrap());
    }

    #[test]
    fn test_cross_margin_shared_collateral() {
        let account_id = AccountId::new();
        let mut engine = CrossMarginEngine::new(account_id, Decimal::from(50_000));

        // Two positions sharing the same collateral pool
        let pos1 = Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Price::from_u64(49_500),
            Decimal::from(5_000),
            Decimal::from(250),
            10,
            1_708_123_456_789_000_000,
        );
        engine.add_position(pos1);

        let pos2 = Position::new(
            account_id,
            MarketId::new("ETH/USDT"),
            PositionSide::SHORT,
            Quantity::from_str("10.0").unwrap(),
            Price::from_u64(3_000),
            Price::from_u64(2_900),
            Price::from_u64(3_100),
            Decimal::from(3_000),
            Decimal::from(150),
            10,
            1_708_123_456_789_000_000,
        );
        engine.add_position(pos2);

        // uPnL = +1000 (BTC) + +1000 (ETH) = 2000
        assert_eq!(engine.total_unrealized_pnl(), Decimal::from(2_000));

        // equity = 50 000 + 2 000 = 52 000
        assert_eq!(engine.equity(), Decimal::from(52_000));

        // total IM = 5000 + 3000 = 8000 (shared pool)
        assert_eq!(engine.total_initial_margin(), Decimal::from(8_000));
    }

    #[test]
    fn test_deterministic_simulation() {
        let engine = make_engine();
        let p1 = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );
        let p2 = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );
        assert_eq!(p1, p2, "Simulation must be deterministic");
    }

    #[test]
    fn test_empty_engine() {
        let engine = CrossMarginEngine::new(AccountId::new(), Decimal::from(10_000));
        assert_eq!(engine.equity(), Decimal::from(10_000));
        assert_eq!(engine.total_unrealized_pnl(), Decimal::ZERO);
        assert_eq!(engine.total_initial_margin(), Decimal::ZERO);
        assert_eq!(engine.risk_level(), RiskLevel::Healthy);
    }

    #[test]
    #[should_panic(expected = "Isolated margin mode is not yet supported")]
    fn test_isolated_margin_stub_panics() {
        let engine = IsolatedMarginEngine::new(AccountId::new());
        engine.simulate_order(
            "BTC/USDT",
            Side::BUY,
            Price::from_u64(50_000),
            Quantity::from_str("1.0").unwrap(),
            10,
        );
    }

    #[test]
    fn test_margin_preview_serialization() {
        let engine = make_engine();
        let preview = engine.simulate_order(
            "ETH/USDT",
            Side::BUY,
            Price::from_u64(3_000),
            Quantity::from_str("10.0").unwrap(),
            20,
        );
        let json = serde_json::to_string(&preview).unwrap();
        let restored: MarginPreview = serde_json::from_str(&json).unwrap();
        assert_eq!(preview, restored);
    }
}
