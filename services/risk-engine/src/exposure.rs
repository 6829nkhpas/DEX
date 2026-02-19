//! Exposure and position value calculations
//!
//! Deterministic computation of position value, equity,
//! unrealized PnL, and total exposure per specs §4.4.3 and §5.3.

use rust_decimal::Decimal;
use types::numeric::{Price, Quantity};
use types::position::{Position, PositionSide};

/// Calculate notional position value per spec §5.3.2
///
/// `position_value = size × mark_price`
pub fn position_value(size: Quantity, mark_price: Price) -> Decimal {
    size.as_decimal() * mark_price.as_decimal()
}

/// Calculate account equity per spec §5.3.3
///
/// `equity = total_balance + unrealized_pnl`
pub fn equity(total_balance: Decimal, unrealized_pnl: Decimal) -> Decimal {
    total_balance + unrealized_pnl
}

/// Calculate unrealized PnL per spec §4.4.3
///
/// LONG:  `(mark_price - entry_price) × size`
/// SHORT: `(entry_price - mark_price) × size`
pub fn unrealized_pnl(
    side: PositionSide,
    entry_price: Price,
    mark_price: Price,
    size: Quantity,
) -> Decimal {
    let size_d = size.as_decimal();
    match side {
        PositionSide::LONG => {
            (mark_price.as_decimal() - entry_price.as_decimal()) * size_d
        }
        PositionSide::SHORT => {
            (entry_price.as_decimal() - mark_price.as_decimal()) * size_d
        }
    }
}

/// Calculate total exposure across all positions.
///
/// `total_exposure = Σ (position.size × position.mark_price)`
pub fn total_exposure(positions: &[Position]) -> Decimal {
    positions.iter().fold(Decimal::ZERO, |acc, pos| {
        acc + position_value(pos.size, pos.mark_price)
    })
}

/// Calculate total unrealized PnL across all positions.
///
/// `total_upnl = Σ position.unrealized_pnl`
pub fn total_unrealized_pnl(positions: &[Position]) -> Decimal {
    positions.iter().fold(Decimal::ZERO, |acc, pos| {
        acc + pos.unrealized_pnl
    })
}

/// Calculate total maintenance margin across all positions.
///
/// `total_mm = Σ position.maintenance_margin`
pub fn total_maintenance_margin(positions: &[Position]) -> Decimal {
    positions.iter().fold(Decimal::ZERO, |acc, pos| {
        acc + pos.maintenance_margin
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{AccountId, MarketId};

    fn make_position(
        side: PositionSide,
        size_str: &str,
        entry: u64,
        mark: u64,
        im: i64,
        mm: i64,
    ) -> Position {
        Position::new(
            AccountId::new(),
            MarketId::new("BTC/USDT"),
            side,
            Quantity::from_str(size_str).unwrap(),
            Price::from_u64(entry),
            Price::from_u64(mark),
            Price::from_u64(if entry > 100 { entry - 100 } else { 1 }),
            Decimal::from(im),
            Decimal::from(mm),
            10,
            1708123456789000000,
        )
    }

    #[test]
    fn test_position_value() {
        let pv = position_value(
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50_000),
        );
        assert_eq!(pv, Decimal::from(50_000));
    }

    #[test]
    fn test_position_value_fractional() {
        let pv = position_value(
            Quantity::from_str("0.5").unwrap(),
            Price::from_u64(50_000),
        );
        assert_eq!(pv, Decimal::from(25_000));
    }

    #[test]
    fn test_equity_profit() {
        let eq = equity(Decimal::from(10_000), Decimal::from(2_000));
        assert_eq!(eq, Decimal::from(12_000));
    }

    #[test]
    fn test_equity_loss() {
        let eq = equity(Decimal::from(10_000), Decimal::from(-3_000));
        assert_eq!(eq, Decimal::from(7_000));
    }

    #[test]
    fn test_unrealized_pnl_long_profit() {
        let pnl = unrealized_pnl(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(pnl, Decimal::from(1_000));
    }

    #[test]
    fn test_unrealized_pnl_long_loss() {
        let pnl = unrealized_pnl(
            PositionSide::LONG,
            Price::from_u64(50_000),
            Price::from_u64(49_000),
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(pnl, Decimal::from(-1_000));
    }

    #[test]
    fn test_unrealized_pnl_short_profit() {
        let pnl = unrealized_pnl(
            PositionSide::SHORT,
            Price::from_u64(50_000),
            Price::from_u64(49_000),
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(pnl, Decimal::from(1_000));
    }

    #[test]
    fn test_unrealized_pnl_short_loss() {
        let pnl = unrealized_pnl(
            PositionSide::SHORT,
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(pnl, Decimal::from(-1_000));
    }

    #[test]
    fn test_total_exposure_multi_position() {
        let positions = vec![
            make_position(PositionSide::LONG, "1.0", 50_000, 51_000, 5000, 500),
            make_position(PositionSide::SHORT, "2.0", 3_000, 2_900, 600, 60),
        ];
        // 1×51000 + 2×2900 = 56800
        let exposure = total_exposure(&positions);
        assert_eq!(exposure, Decimal::from(56_800));
    }

    #[test]
    fn test_total_exposure_empty() {
        let exposure = total_exposure(&[]);
        assert_eq!(exposure, Decimal::ZERO);
    }

    #[test]
    fn test_total_unrealized_pnl() {
        let positions = vec![
            make_position(PositionSide::LONG, "1.0", 50_000, 51_000, 5000, 500),
            make_position(PositionSide::SHORT, "1.0", 3_000, 2_900, 300, 30),
        ];
        // LONG PnL: (51000-50000)*1 = 1000
        // SHORT PnL: (3000-2900)*1 = 100
        let total = total_unrealized_pnl(&positions);
        assert_eq!(total, Decimal::from(1_100));
    }

    #[test]
    fn test_deterministic_pnl() {
        let r1 = unrealized_pnl(
            PositionSide::LONG,
            Price::from_str("50000.123").unwrap(),
            Price::from_str("51000.456").unwrap(),
            Quantity::from_str("1.23456789").unwrap(),
        );
        let r2 = unrealized_pnl(
            PositionSide::LONG,
            Price::from_str("50000.123").unwrap(),
            Price::from_str("51000.456").unwrap(),
            Quantity::from_str("1.23456789").unwrap(),
        );
        assert_eq!(r1, r2, "Determinism violated");
    }
}
