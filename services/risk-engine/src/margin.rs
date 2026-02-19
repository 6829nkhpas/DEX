//! Margin calculation functions
//!
//! Deterministic margin computations per spec §5 (Margin Methodology).
//! All calculations use fixed-point Decimal arithmetic with HALF_UP rounding.

use rust_decimal::Decimal;
use rust_decimal::prelude::*;

// ── Leverage tier table per spec §5.4.1 ──────────────────────────────────

/// Leverage tier configuration derived from spec §5.4.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeverageTier {
    /// Maximum position value (USDT) for this tier (exclusive upper bound)
    pub max_position_value: Option<Decimal>,
    /// Maximum allowed leverage
    pub max_leverage: u8,
    /// Initial margin rate
    pub im_rate: Decimal,
    /// Maintenance margin rate
    pub mm_rate: Decimal,
}

/// Returns the leverage tier for a given position value per spec §5.4.1.
///
/// | Position Value (USDT) | Max Leverage | IM Rate  | MM Rate  |
/// |-----------------------|-------------|----------|----------|
/// | 0 – 50,000            | 125x        | 0.80%    | 0.40%    |
/// | 50,001 – 250,000      | 100x        | 1.00%    | 0.50%    |
/// | 250,001 – 1,000,000   | 50x         | 2.00%    | 1.00%    |
/// | 1,000,001 – 5,000,000 | 20x         | 5.00%    | 2.50%    |
/// | 5,000,001 – 20,000,000| 10x         | 10.00%   | 5.00%    |
/// | 20,000,001+           | 5x          | 20.00%   | 10.00%   |
pub fn leverage_tier(position_value: Decimal) -> LeverageTier {
    let tiers = leverage_tiers();
    for (i, (threshold, tier)) in tiers.iter().enumerate() {
        if position_value <= *threshold {
            return *tier;
        }
        // If this is the last tier with a threshold, fall through
        if i == tiers.len() - 1 {
            return *tier;
        }
    }
    // Fallback: most conservative tier
    tiers.last().unwrap().1
}

/// Full leverage tier table (sorted ascending by threshold).
fn leverage_tiers() -> Vec<(Decimal, LeverageTier)> {
    vec![
        (
            Decimal::from(50_000),
            LeverageTier {
                max_position_value: Some(Decimal::from(50_000)),
                max_leverage: 125,
                im_rate: Decimal::from_str_exact("0.008").unwrap(),
                mm_rate: Decimal::from_str_exact("0.004").unwrap(),
            },
        ),
        (
            Decimal::from(250_000),
            LeverageTier {
                max_position_value: Some(Decimal::from(250_000)),
                max_leverage: 100,
                im_rate: Decimal::from_str_exact("0.01").unwrap(),
                mm_rate: Decimal::from_str_exact("0.005").unwrap(),
            },
        ),
        (
            Decimal::from(1_000_000),
            LeverageTier {
                max_position_value: Some(Decimal::from(1_000_000)),
                max_leverage: 50,
                im_rate: Decimal::from_str_exact("0.02").unwrap(),
                mm_rate: Decimal::from_str_exact("0.01").unwrap(),
            },
        ),
        (
            Decimal::from(5_000_000),
            LeverageTier {
                max_position_value: Some(Decimal::from(5_000_000)),
                max_leverage: 20,
                im_rate: Decimal::from_str_exact("0.05").unwrap(),
                mm_rate: Decimal::from_str_exact("0.025").unwrap(),
            },
        ),
        (
            Decimal::from(20_000_000),
            LeverageTier {
                max_position_value: Some(Decimal::from(20_000_000)),
                max_leverage: 10,
                im_rate: Decimal::from_str_exact("0.1").unwrap(),
                mm_rate: Decimal::from_str_exact("0.05").unwrap(),
            },
        ),
        (
            Decimal::MAX,
            LeverageTier {
                max_position_value: None,
                max_leverage: 5,
                im_rate: Decimal::from_str_exact("0.2").unwrap(),
                mm_rate: Decimal::from_str_exact("0.1").unwrap(),
            },
        ),
    ]
}

// ── Core margin calculations ─────────────────────────────────────────────

/// Calculate initial margin per spec §5.2.1
///
/// `initial_margin = position_value / leverage`
///
/// Rounds UP to favor safety per spec §5.9.2.
pub fn initial_margin(position_value: Decimal, leverage: u8) -> Decimal {
    assert!(leverage >= 1, "Leverage must be >= 1");
    let result = position_value / Decimal::from(leverage);
    round_up(result)
}

/// Calculate maintenance margin per spec §5.2.2
///
/// `maintenance_margin = position_value × mm_rate`
///
/// Rounds UP to favor safety per spec §5.9.2.
pub fn maintenance_margin(position_value: Decimal, mm_rate: Decimal) -> Decimal {
    let result = position_value * mm_rate;
    round_up(result)
}

/// Calculate margin ratio per spec §5.3.3 / §4.8.1
///
/// `margin_ratio = equity / maintenance_margin`
///
/// Returns `Decimal::MAX` if maintenance_margin is zero (no position).
pub fn margin_ratio(equity: Decimal, maintenance_margin: Decimal) -> Decimal {
    if maintenance_margin == Decimal::ZERO {
        return Decimal::MAX;
    }
    equity / maintenance_margin
}

/// Calculate available margin per spec §5.3.1
///
/// `available_margin = equity - margin_used - locked_margin`
///
/// Rounds DOWN (conservative) per spec §5.9.2.
pub fn available_margin(
    equity: Decimal,
    margin_used: Decimal,
    locked_margin: Decimal,
) -> Decimal {
    let result = equity - margin_used - locked_margin;
    round_down(result)
}

/// Calculate order margin requirement per spec §5.3.1
///
/// `order_margin = (quantity × price) / leverage`
///
/// Rounds UP to favor safety.
pub fn order_margin(
    quantity: Decimal,
    price: Decimal,
    leverage: u8,
) -> Decimal {
    assert!(leverage >= 1, "Leverage must be >= 1");
    let notional = quantity * price;
    let result = notional / Decimal::from(leverage);
    round_up(result)
}

/// Check if requested leverage is within tier limits per spec §5.4.1.
///
/// Returns true if leverage is valid for the given position value.
pub fn is_leverage_valid(position_value: Decimal, requested_leverage: u8) -> bool {
    if requested_leverage < 1 {
        return false;
    }
    let tier = leverage_tier(position_value);
    requested_leverage <= tier.max_leverage
}

// ── Rounding helpers (deterministic, HALF_UP) ────────────────────────────

/// Round UP to 18 decimal places (favor safety for margins).
fn round_up(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(18, RoundingStrategy::MidpointAwayFromZero)
}

/// Round DOWN to 18 decimal places (conservative for available margin).
fn round_down(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(18, RoundingStrategy::ToZero)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── leverage_tier tests ──

    #[test]
    fn test_tier_smallest_position() {
        let tier = leverage_tier(Decimal::from(1_000));
        assert_eq!(tier.max_leverage, 125);
        assert_eq!(tier.im_rate, Decimal::from_str_exact("0.008").unwrap());
        assert_eq!(tier.mm_rate, Decimal::from_str_exact("0.004").unwrap());
    }

    #[test]
    fn test_tier_boundary_50k() {
        let tier = leverage_tier(Decimal::from(50_000));
        assert_eq!(tier.max_leverage, 125);

        let tier = leverage_tier(Decimal::from(50_001));
        assert_eq!(tier.max_leverage, 100);
    }

    #[test]
    fn test_tier_boundary_250k() {
        let tier = leverage_tier(Decimal::from(250_000));
        assert_eq!(tier.max_leverage, 100);

        let tier = leverage_tier(Decimal::from(250_001));
        assert_eq!(tier.max_leverage, 50);
    }

    #[test]
    fn test_tier_boundary_1m() {
        let tier = leverage_tier(Decimal::from(1_000_000));
        assert_eq!(tier.max_leverage, 50);

        let tier = leverage_tier(Decimal::from(1_000_001));
        assert_eq!(tier.max_leverage, 20);
    }

    #[test]
    fn test_tier_boundary_5m() {
        let tier = leverage_tier(Decimal::from(5_000_000));
        assert_eq!(tier.max_leverage, 20);

        let tier = leverage_tier(Decimal::from(5_000_001));
        assert_eq!(tier.max_leverage, 10);
    }

    #[test]
    fn test_tier_boundary_20m() {
        let tier = leverage_tier(Decimal::from(20_000_000));
        assert_eq!(tier.max_leverage, 10);

        let tier = leverage_tier(Decimal::from(20_000_001));
        assert_eq!(tier.max_leverage, 5);
    }

    #[test]
    fn test_tier_very_large_position() {
        let tier = leverage_tier(Decimal::from(100_000_000));
        assert_eq!(tier.max_leverage, 5);
        assert_eq!(tier.im_rate, Decimal::from_str_exact("0.2").unwrap());
        assert_eq!(tier.mm_rate, Decimal::from_str_exact("0.1").unwrap());
    }

    // ── initial_margin tests ──

    #[test]
    fn test_initial_margin_10x() {
        // 1 BTC @ $50,000, 10x leverage → $5,000
        let im = initial_margin(Decimal::from(50_000), 10);
        assert_eq!(im, Decimal::from(5_000));
    }

    #[test]
    fn test_initial_margin_1x() {
        let im = initial_margin(Decimal::from(50_000), 1);
        assert_eq!(im, Decimal::from(50_000));
    }

    #[test]
    fn test_initial_margin_125x() {
        let im = initial_margin(Decimal::from(50_000), 125);
        assert_eq!(im, Decimal::from(400));
    }

    // ── maintenance_margin tests ──

    #[test]
    fn test_maintenance_margin() {
        // $50,000 position × 0.4% = $200
        let mm = maintenance_margin(
            Decimal::from(50_000),
            Decimal::from_str_exact("0.004").unwrap(),
        );
        assert_eq!(mm, Decimal::from(200));
    }

    #[test]
    fn test_maintenance_less_than_initial() {
        let pos_value = Decimal::from(50_000);
        let tier = leverage_tier(pos_value);
        let im = initial_margin(pos_value, tier.max_leverage);
        let mm = maintenance_margin(pos_value, tier.mm_rate);
        assert!(mm < im, "Invariant: maintenance < initial");
    }

    // ── margin_ratio tests ──

    #[test]
    fn test_margin_ratio_healthy() {
        // Equity=6000, MM=500 → ratio=12.0
        let ratio = margin_ratio(Decimal::from(6_000), Decimal::from(500));
        assert_eq!(ratio, Decimal::from(12));
    }

    #[test]
    fn test_margin_ratio_liquidation_zone() {
        // Equity=500, MM=500 → ratio=1.0 (< 1.1 = liquidation)
        let ratio = margin_ratio(Decimal::from(500), Decimal::from(500));
        assert_eq!(ratio, Decimal::from(1));
        assert!(ratio < Decimal::from_str_exact("1.1").unwrap());
    }

    #[test]
    fn test_margin_ratio_zero_mm() {
        let ratio = margin_ratio(Decimal::from(5_000), Decimal::ZERO);
        assert_eq!(ratio, Decimal::MAX);
    }

    // ── available_margin tests ──

    #[test]
    fn test_available_margin_positive() {
        let am = available_margin(
            Decimal::from(10_000),
            Decimal::from(3_000),
            Decimal::from(2_000),
        );
        assert_eq!(am, Decimal::from(5_000));
    }

    #[test]
    fn test_available_margin_negative() {
        // If losses exceed equity, available margin can be negative
        let am = available_margin(
            Decimal::from(1_000),
            Decimal::from(3_000),
            Decimal::from(2_000),
        );
        assert!(am < Decimal::ZERO);
    }

    // ── order_margin tests ──

    #[test]
    fn test_order_margin() {
        // Buy 1 BTC @ $50,000, 10x → $5,000
        let om = order_margin(Decimal::from(1), Decimal::from(50_000), 10);
        assert_eq!(om, Decimal::from(5_000));
    }

    #[test]
    fn test_order_margin_fractional() {
        // Buy 0.5 BTC @ $50,000, 10x → $2,500
        let om = order_margin(
            Decimal::from_str_exact("0.5").unwrap(),
            Decimal::from(50_000),
            10,
        );
        assert_eq!(om, Decimal::from(2_500));
    }

    // ── leverage validation tests ──

    #[test]
    fn test_leverage_valid_within_tier() {
        assert!(is_leverage_valid(Decimal::from(10_000), 100));
        assert!(is_leverage_valid(Decimal::from(10_000), 125));
    }

    #[test]
    fn test_leverage_exceeds_tier() {
        // $100,000 position → tier max 100x
        assert!(!is_leverage_valid(Decimal::from(100_000), 125));
        assert!(is_leverage_valid(Decimal::from(100_000), 100));
    }

    #[test]
    fn test_leverage_zero_invalid() {
        assert!(!is_leverage_valid(Decimal::from(10_000), 0));
    }

    // ── invariant tests ──

    #[test]
    fn test_margin_hierarchy_all_tiers() {
        let values = [
            1_000, 50_000, 100_000, 250_000, 500_000, 1_000_000,
            5_000_000, 10_000_000, 20_000_000, 50_000_000,
        ];
        for v in values {
            let pos_value = Decimal::from(v);
            let tier = leverage_tier(pos_value);
            let im = initial_margin(pos_value, tier.max_leverage);
            let mm = maintenance_margin(pos_value, tier.mm_rate);
            assert!(
                mm < im,
                "Invariant violated at position_value={}: mm={} >= im={}",
                v, mm, im
            );
            assert!(
                im < pos_value,
                "Invariant violated at position_value={}: im={} >= pv={}",
                v, im, pos_value
            );
        }
    }

    // ── determinism test ──

    #[test]
    fn test_deterministic_calculations() {
        let pv = Decimal::from_str_exact("123456.789").unwrap();
        let r1 = initial_margin(pv, 10);
        let r2 = initial_margin(pv, 10);
        assert_eq!(r1, r2, "Determinism violated");
    }
}
