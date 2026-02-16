//! Risk management and liquidation types
//!
//! Implements spec §6 (Liquidation Process)

use crate::ids::AccountId;
use crate::numeric::{Price, Quantity};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Risk check result per spec §6.2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RiskCheckResult {
    /// Passed all risk checks
    Pass,
    /// Failed: insufficient margin
    InsufficientMargin {
        required: Decimal,
        available: Decimal,
    },
    /// Failed: position size exceeds limit
    PositionLimitExceeded {
        limit: Quantity,
        requested: Quantity,
    },
    /// Failed: leverage too high
    LeverageExceeded {
        max_leverage: u8,
        requested: u8,
    },
}

/// Liquidation event per spec §6.3
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Liquidation {
    pub liquidation_id: Uuid,
    pub account_id: AccountId,
    pub symbol: String,
    pub liquidation_price: Price,
    pub quantity: Quantity,
    pub liquidation_fee: Decimal,
    pub insurance_fund_used: Decimal,
    pub timestamp: i64,
    pub is_partial: bool,
}

impl Liquidation {
    /// Create a new liquidation event
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: AccountId,
        symbol: impl Into<String>,
        liquidation_price: Price,
        quantity: Quantity,
        liquidation_fee: Decimal,
        insurance_fund_used: Decimal,
        is_partial: bool,
        timestamp: i64,
    ) -> Self {
        Self {
            liquidation_id: Uuid::now_v7(),
            account_id,
            symbol: symbol.into(),
            liquidation_price,
            quantity,
            liquidation_fee,
            insurance_fund_used,
            timestamp,
            is_partial,
        }
    }

    /// Calculate total liquidation value
    pub fn liquidation_value(&self) -> Decimal {
        self.quantity.as_decimal() * self.liquidation_price.as_decimal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_check_pass() {
        let result = RiskCheckResult::Pass;
        assert_eq!(result, RiskCheckResult::Pass);
    }

    #[test]
    fn test_risk_check_insufficient_margin() {
        let result = RiskCheckResult::InsufficientMargin {
            required: Decimal::from(1000),
            available: Decimal::from(500),
        };

        match result {
            RiskCheckResult::InsufficientMargin { required, available } => {
                assert_eq!(required, Decimal::from(1000));
                assert_eq!(available, Decimal::from(500));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_liquidation_creation() {
        let liquidation = Liquidation::new(
            AccountId::new(),
            "BTC/USDT",
            Price::from_u64(40000),
            Quantity::from_str("1.5").unwrap(),
            Decimal::from(300),
            Decimal::from(100),
            false,
            1708123456789000000,
        );

        assert_eq!(liquidation.liquidation_value(), Decimal::from(60000));
        assert!(!liquidation.is_partial);
    }

    #[test]
    fn test_partial_liquidation() {
        let liquidation = Liquidation::new(
            AccountId::new(),
            "ETH/USDT",
            Price::from_u64(3000),
            Quantity::from_str("0.5").unwrap(),
            Decimal::from(75),
            Decimal::ZERO,
            true,
            1708123456789000000,
        );

        assert!(liquidation.is_partial);
        assert_eq!(liquidation.insurance_fund_used, Decimal::ZERO);
    }
}
