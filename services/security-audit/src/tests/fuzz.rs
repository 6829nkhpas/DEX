//! Fuzz testing harness using `proptest`.
//! Uses property-based testing to explore edge cases in order matching, balance updates, etc.

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use types::account::{Account, AccountType, Balance};
    use rust_decimal::Decimal;

    proptest! {
        #[test]
        fn doesnt_crash_on_random_balance_updates(
            initial_balance in 0f64..1_000_000.0,
            amount in 0f64..2_000_000.0,
        ) {
            let initial = Decimal::try_from(initial_balance).unwrap_or(Decimal::ZERO);
            let mut balance = Balance::new("USDT", initial);
            let p_amount = Decimal::try_from(amount).unwrap_or(Decimal::ZERO);

            // Mutate and ensure invariants hold (no panics unless logic dictates)
            if p_amount <= balance.available && p_amount >= Decimal::ZERO {
                balance.lock(p_amount);
                assert!(balance.check_invariant());
                
                // Can unlock
                balance.unlock(p_amount);
                assert!(balance.check_invariant());
            }
        }
    }
}
