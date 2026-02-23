//! Invariant tests.
//! Checks global economic invariants like balance conservation.

use rust_decimal::Decimal;
use types::account::Balance;

/// Validates that the sum of all balances matches the sum of expected blockchain deposits minus withdrawals.
pub fn validate_balance_conservation(
    balances: &[Balance],
    expected_total_deposits: Decimal,
    expected_total_withdrawals: Decimal,
    total_fees_collected: Decimal,
) -> Result<(), &'static str> {
    let mut system_total = Decimal::ZERO;

    for b in balances {
        if !b.check_invariant() {
            return Err("Individual balance invariant failed");
        }
        system_total += b.total;
    }

    let expected_total =
        expected_total_deposits - expected_total_withdrawals - total_fees_collected;

    if system_total != expected_total {
        return Err(
            "Balance conservation violation: System total != Deposits - Withdrawals - Fees",
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservation_invariant() {
        let b1 = Balance::new("BTC", Decimal::from(10));
        let b2 = Balance::new("BTC", Decimal::from(15));

        // 30 deposited, 3 withdrawn, 2 in fees => 25 total expected in system
        assert_eq!(
            validate_balance_conservation(
                &[b1, b2],
                Decimal::from(30),
                Decimal::from(3),
                Decimal::from(2)
            ),
            Ok(())
        );
    }
}
