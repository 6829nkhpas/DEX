//! Fee calculation types
//!
//! Implements spec §7 (Fee System)

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Fee type per spec §7.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeeType {
    /// Maker fee (adds liquidity)
    MakerFee,
    /// Taker fee (removes liquidity)
    TakerFee,
    /// Liquidation fee
    LiquidationFee,
    /// Withdrawal fee
    WithdrawalFee,
    /// Funding rate payment
    FundingFee,
}

/// Fee tier configuration per spec §7.3
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeeTier {
    pub volume_threshold: Decimal,  // 30-day volume
    pub maker_rate: Decimal,        // Can be negative (rebate)
    pub taker_rate: Decimal,
}

impl FeeTier {
    /// Calculate fee for a trade value
    pub fn calculate_maker_fee(&self, trade_value: Decimal) -> Decimal {
        trade_value * self.maker_rate
    }

    /// Calculate taker fee for a trade value
    pub fn calculate_taker_fee(&self, trade_value: Decimal) -> Decimal {
        trade_value * self.taker_rate
    }
}

/// Standard fee tiers per spec §7.3
pub fn default_fee_tiers() -> Vec<FeeTier> {
    vec![
        // Tier 0: < $1M volume
        FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),      // 0.02% maker
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),      // 0.05% taker
        },
        // Tier 1: $1M - $10M
        FeeTier {
            volume_threshold: Decimal::from(1_000_000),
            maker_rate: Decimal::from_str_exact("0.00015").unwrap(),     // 0.015%
            taker_rate: Decimal::from_str_exact("0.00045").unwrap(),     // 0.045%
        },
        // Tier 2: $10M - $50M
        FeeTier {
            volume_threshold: Decimal::from(10_000_000),
            maker_rate: Decimal::from_str_exact("0.0001").unwrap(),      // 0.01%
            taker_rate: Decimal::from_str_exact("0.0004").unwrap(),      // 0.04%
        },
        // Tier 3: > $50M (maker rebate)
        FeeTier {
            volume_threshold: Decimal::from(50_000_000),
            maker_rate: Decimal::from_str_exact("-0.00005").unwrap(),    // -0.005% rebate
            taker_rate: Decimal::from_str_exact("0.00035").unwrap(),     // 0.035%
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_tier_calculation() {
        let tier = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };

        let trade_value = Decimal::from(100000);
        let maker_fee = tier.calculate_maker_fee(trade_value);
        let taker_fee = tier.calculate_taker_fee(trade_value);

        assert_eq!(maker_fee, Decimal::from(20));  // 100000 * 0.0002
        assert_eq!(taker_fee, Decimal::from(50));  // 100000 * 0.0005
    }

    #[test]
    fn test_maker_rebate() {
        let tier = FeeTier {
            volume_threshold: Decimal::from(50_000_000),
            maker_rate: Decimal::from_str_exact("-0.00005").unwrap(),
            taker_rate: Decimal::from_str_exact("0.00035").unwrap(),
        };

        let trade_value = Decimal::from(100000);
        let maker_fee = tier.calculate_maker_fee(trade_value);

        assert_eq!(maker_fee, Decimal::from(-5));  // Negative = rebate
    }

    #[test]
    fn test_default_tiers() {
        let tiers = default_fee_tiers();
        assert_eq!(tiers.len(), 4);
        assert_eq!(tiers[0].volume_threshold, Decimal::ZERO);
        assert_eq!(tiers[3].volume_threshold, Decimal::from(50_000_000));
    }
}

