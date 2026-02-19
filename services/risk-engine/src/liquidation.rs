//! Liquidation calculations
//!
//! Deterministic liquidation threshold, bankruptcy price, and fee
//! calculations per spec §6 (Liquidation Process).

use rust_decimal::Decimal;
use types::numeric::Price;
use types::position::PositionSide;

// ── Health levels per spec §5.3.3 ────────────────────────────────────────

/// Account health classification per spec §5.3.3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthLevel {
    /// margin_ratio >= 2.0 — no action needed
    Healthy,
    /// 1.5 <= margin_ratio < 2.0 — emit warning
    Warning,
    /// 1.1 <= margin_ratio < 1.5 — danger, reduce recommended
    Danger,
    /// margin_ratio < 1.1 — liquidation triggered
    Liquidation,
}

/// Classify health level from margin ratio per spec §5.3.3
pub fn health_status(margin_ratio: Decimal) -> HealthLevel {
    let threshold_liquidation = Decimal::from_str_exact("1.1").unwrap();
    let threshold_danger = Decimal::from_str_exact("1.5").unwrap();
    let threshold_warning = Decimal::from_str_exact("2.0").unwrap();

    if margin_ratio < threshold_liquidation {
        HealthLevel::Liquidation
    } else if margin_ratio < threshold_danger {
        HealthLevel::Danger
    } else if margin_ratio < threshold_warning {
        HealthLevel::Warning
    } else {
        HealthLevel::Healthy
    }
}

/// Check if liquidation should trigger per spec §6.2.1
///
/// `margin_ratio < 1.1` → true
pub fn should_liquidate(margin_ratio: Decimal) -> bool {
    margin_ratio < Decimal::from_str_exact("1.1").unwrap()
}

// ── Bankruptcy price per spec §6.4.3 ─────────────────────────────────────

/// Calculate bankruptcy price per spec §6.4.3
///
/// LONG:  `bankruptcy_price = entry_price - (initial_margin / size)`
/// SHORT: `bankruptcy_price = entry_price + (initial_margin / size)`
///
/// Returns None if the result would be non-positive (for LONG positions
/// with very high leverage).
pub fn bankruptcy_price(
    side: PositionSide,
    entry_price: Price,
    initial_margin: Decimal,
    size: Decimal,
) -> Option<Price> {
    assert!(size > Decimal::ZERO, "Position size must be positive");
    let margin_per_unit = initial_margin / size;

    match side {
        PositionSide::LONG => {
            let bp = entry_price.as_decimal() - margin_per_unit;
            Price::try_new(bp)
        }
        PositionSide::SHORT => {
            let bp = entry_price.as_decimal() + margin_per_unit;
            Price::try_new(bp)
        }
    }
}

/// Calculate liquidation price (slightly inside bankruptcy price).
///
/// LONG:  `liq_price = entry_price - ((initial_margin - maintenance_margin) / size)`
/// SHORT: `liq_price = entry_price + ((initial_margin - maintenance_margin) / size)`
///
/// Liquidation triggers before bankruptcy to protect insurance fund.
pub fn liquidation_price(
    side: PositionSide,
    entry_price: Price,
    initial_margin: Decimal,
    maintenance_margin: Decimal,
    size: Decimal,
) -> Option<Price> {
    assert!(size > Decimal::ZERO, "Position size must be positive");
    let margin_diff = initial_margin - maintenance_margin;
    let offset = margin_diff / size;

    match side {
        PositionSide::LONG => {
            let lp = entry_price.as_decimal() - offset;
            Price::try_new(lp)
        }
        PositionSide::SHORT => {
            let lp = entry_price.as_decimal() + offset;
            Price::try_new(lp)
        }
    }
}

// ── Liquidation fee per spec §6.7.1 ──────────────────────────────────────

/// Calculate liquidation fee per spec §6.7.1
///
/// | Margin ratio at liquidation | Fee rate |
/// |-----------------------------|----------|
/// | 1.05 – 1.10                | 0.50%    |
/// | 0.50 – 1.05                | 1.00%    |
/// | < 0.50                     | 2.00%    |
///
/// Fee is capped at 5% of position value (§6.7.3).
pub fn liquidation_fee(position_value: Decimal, margin_ratio: Decimal) -> Decimal {
    let rate = if margin_ratio >= Decimal::from_str_exact("1.05").unwrap() {
        Decimal::from_str_exact("0.005").unwrap()
    } else if margin_ratio >= Decimal::from_str_exact("0.5").unwrap() {
        Decimal::from_str_exact("0.01").unwrap()
    } else {
        Decimal::from_str_exact("0.02").unwrap()
    };

    let fee = position_value * rate;
    let cap = position_value * Decimal::from_str_exact("0.05").unwrap();

    if fee > cap { cap } else { fee }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── health_status tests ──

    #[test]
    fn test_health_healthy() {
        assert_eq!(health_status(Decimal::from(3)), HealthLevel::Healthy);
        assert_eq!(health_status(Decimal::from(2)), HealthLevel::Healthy);
    }

    #[test]
    fn test_health_warning() {
        assert_eq!(
            health_status(Decimal::from_str_exact("1.9").unwrap()),
            HealthLevel::Warning
        );
        assert_eq!(
            health_status(Decimal::from_str_exact("1.5").unwrap()),
            HealthLevel::Warning
        );
    }

    #[test]
    fn test_health_danger() {
        assert_eq!(
            health_status(Decimal::from_str_exact("1.4").unwrap()),
            HealthLevel::Danger
        );
        assert_eq!(
            health_status(Decimal::from_str_exact("1.1").unwrap()),
            HealthLevel::Danger
        );
    }

    #[test]
    fn test_health_liquidation() {
        assert_eq!(
            health_status(Decimal::from_str_exact("1.09").unwrap()),
            HealthLevel::Liquidation
        );
        assert_eq!(
            health_status(Decimal::from_str_exact("0.5").unwrap()),
            HealthLevel::Liquidation
        );
    }

    // ── should_liquidate tests ──

    #[test]
    fn test_should_liquidate_yes() {
        assert!(should_liquidate(Decimal::from_str_exact("1.09").unwrap()));
        assert!(should_liquidate(Decimal::from_str_exact("0.5").unwrap()));
    }

    #[test]
    fn test_should_liquidate_no() {
        assert!(!should_liquidate(Decimal::from_str_exact("1.1").unwrap()));
        assert!(!should_liquidate(Decimal::from(2)));
    }

    // ── bankruptcy_price tests ──

    #[test]
    fn test_bankruptcy_price_long() {
        // Entry $50,000, IM $5,000, size 1 → BP = 50000 - 5000 = $45,000
        let bp = bankruptcy_price(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(1),
        );
        assert_eq!(bp, Some(Price::from_u64(45_000)));
    }

    #[test]
    fn test_bankruptcy_price_short() {
        // Entry $50,000, IM $5,000, size 1 → BP = 50000 + 5000 = $55,000
        let bp = bankruptcy_price(
            PositionSide::SHORT,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(1),
        );
        assert_eq!(bp, Some(Price::from_u64(55_000)));
    }

    #[test]
    fn test_bankruptcy_price_high_leverage_long() {
        // Entry $100, IM $100, size 1 → BP = 100 - 100 = 0 (None)
        let bp = bankruptcy_price(
            PositionSide::LONG,
            Price::from_u64(100),
            Decimal::from(100),
            Decimal::from(1),
        );
        assert_eq!(bp, None);
    }

    // ── liquidation_price tests ──

    #[test]
    fn test_liquidation_price_long() {
        // Entry $50,000, IM $5,000, MM $500, size 1
        // LP = 50000 - (5000 - 500) / 1 = $45,500
        let lp = liquidation_price(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(500),
            Decimal::from(1),
        );
        assert_eq!(lp, Some(Price::from_u64(45_500)));
    }

    #[test]
    fn test_liquidation_price_short() {
        // Entry $50,000, IM $5,000, MM $500, size 1
        // LP = 50000 + (5000 - 500) / 1 = $54,500
        let lp = liquidation_price(
            PositionSide::SHORT,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(500),
            Decimal::from(1),
        );
        assert_eq!(lp, Some(Price::from_u64(54_500)));
    }

    #[test]
    fn test_liquidation_inside_bankruptcy() {
        // Liquidation price should be closer to entry than bankruptcy price
        let bp = bankruptcy_price(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(1),
        ).unwrap();

        let lp = liquidation_price(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Decimal::from(5_000),
            Decimal::from(500),
            Decimal::from(1),
        ).unwrap();

        assert!(lp > bp, "Liquidation price should be > bankruptcy price for LONG");
    }

    // ── liquidation_fee tests ──

    #[test]
    fn test_liquidation_fee_high_margin() {
        // margin_ratio 1.08 → 0.5% fee
        let fee = liquidation_fee(
            Decimal::from(50_000),
            Decimal::from_str_exact("1.08").unwrap(),
        );
        assert_eq!(fee, Decimal::from(250)); // 50000 × 0.005
    }

    #[test]
    fn test_liquidation_fee_medium_margin() {
        // margin_ratio 0.8 → 1.0% fee
        let fee = liquidation_fee(
            Decimal::from(50_000),
            Decimal::from_str_exact("0.8").unwrap(),
        );
        assert_eq!(fee, Decimal::from(500)); // 50000 × 0.01
    }

    #[test]
    fn test_liquidation_fee_low_margin() {
        // margin_ratio 0.3 → 2.0% fee
        let fee = liquidation_fee(
            Decimal::from(50_000),
            Decimal::from_str_exact("0.3").unwrap(),
        );
        assert_eq!(fee, Decimal::from(1_000)); // 50000 × 0.02
    }

    #[test]
    fn test_liquidation_fee_capped() {
        // Fee cap: 5% of position value
        // With 2% rate on $1M → fee $20,000, cap $50,000 → not capped
        let fee = liquidation_fee(
            Decimal::from(1_000_000),
            Decimal::from_str_exact("0.3").unwrap(),
        );
        assert_eq!(fee, Decimal::from(20_000)); // Below cap

        // Verify cap is at 5%
        let cap = Decimal::from(1_000_000) * Decimal::from_str_exact("0.05").unwrap();
        assert!(fee <= cap);
    }
}
