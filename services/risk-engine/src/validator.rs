//! Pre-trade risk validation
//!
//! Validates incoming orders against margin requirements, leverage limits,
//! and position limits per specs §5.3.1, §5.4, and §9.3.6.

use rust_decimal::Decimal;
use types::account::{Account, AccountType};
use types::order::Order;
use types::position::Position;
use types::risk::RiskCheckResult;

use crate::exposure;
use crate::margin;

/// Maximum position size per account tier per spec §5.14.2
#[allow(dead_code)]
const POSITION_LIMIT_RETAIL: u64 = 100_000;
const POSITION_LIMIT_DEFAULT: u64 = 1_000_000;

/// Validate an incoming order against all risk checks.
///
/// Returns `RiskCheckResult::Pass` if all checks succeed,
/// otherwise returns the first failing check.
///
/// Checks performed (in order):
/// 1. Account is active
/// 2. Leverage within tier limits
/// 3. Sufficient available margin
/// 4. Position size within limits
pub fn validate_order(
    account: &Account,
    order: &Order,
    positions: &[Position],
) -> RiskCheckResult {
    // 1. Account must be active
    if !account.is_active() {
        return RiskCheckResult::InsufficientMargin {
            required: Decimal::ZERO,
            available: Decimal::ZERO,
        };
    }

    let order_price = order.price.as_decimal();
    let order_qty = order.remaining_quantity.as_decimal();
    let notional = order_qty * order_price;

    // 2. Check leverage limits per spec §5.4.1
    let leverage = determine_leverage(account);
    let tier = margin::leverage_tier(notional);
    if leverage > tier.max_leverage {
        return RiskCheckResult::LeverageExceeded {
            max_leverage: tier.max_leverage,
            requested: leverage,
        };
    }

    // 3. Check collateral sufficiency per spec §5.3.1
    let required_margin = margin::order_margin(order_qty, order_price, leverage);
    let available = compute_available_margin(account, positions);
    if required_margin > available {
        return RiskCheckResult::InsufficientMargin {
            required: required_margin,
            available,
        };
    }

    // 4. Check position size limits
    let current_exposure = exposure::total_exposure(positions);
    let new_total = current_exposure + notional;
    let limit = position_limit(account);
    if new_total > limit {
        return RiskCheckResult::PositionLimitExceeded {
            limit: types::numeric::Quantity::new(limit),
            requested: types::numeric::Quantity::new(new_total),
        };
    }

    RiskCheckResult::Pass
}

/// Check collateral sufficiency only (simpler check).
///
/// Used for quick balance verification without full validation.
pub fn check_collateral(
    available_balance: Decimal,
    required_margin: Decimal,
) -> RiskCheckResult {
    if available_balance >= required_margin {
        RiskCheckResult::Pass
    } else {
        RiskCheckResult::InsufficientMargin {
            required: required_margin,
            available: available_balance,
        }
    }
}

/// Determine effective leverage for an account based on type.
fn determine_leverage(account: &Account) -> u8 {
    match account.account_type {
        AccountType::SPOT => 1,
        AccountType::MARGIN => 10,
        AccountType::FUTURES => 10, // Default; real impl reads from position
    }
}

/// Compute available margin from account state and positions.
fn compute_available_margin(account: &Account, positions: &[Position]) -> Decimal {
    // Sum all available balances as collateral
    let total_balance: Decimal = account
        .balances
        .values()
        .map(|b| b.available)
        .sum();

    let total_upnl = exposure::total_unrealized_pnl(positions);
    let eq = exposure::equity(total_balance, total_upnl);
    let mm_used = exposure::total_maintenance_margin(positions);
    let locked: Decimal = account.balances.values().map(|b| b.locked).sum();

    margin::available_margin(eq, mm_used, locked)
}

/// Position limit based on account type.
fn position_limit(account: &Account) -> Decimal {
    match account.account_type {
        AccountType::SPOT => Decimal::from(POSITION_LIMIT_DEFAULT),
        AccountType::MARGIN => Decimal::from(POSITION_LIMIT_DEFAULT),
        AccountType::FUTURES => Decimal::from(POSITION_LIMIT_DEFAULT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::account::{Account, AccountStatus, AccountType, Balance};
    use types::ids::{AccountId, MarketId};
    use types::numeric::{Price, Quantity};
    use types::order::{Order, Side, TimeInForce};
    use types::position::{Position, PositionSide};

    fn make_account(balance: u64) -> Account {
        let mut account = Account::new(AccountType::FUTURES, 1708123456789000000);
        let bal = Balance::new("USDT", Decimal::from(balance));
        account.set_balance(bal, 1708123456789000000);
        account
    }

    fn make_order(account_id: AccountId, price: u64, qty: &str) -> Order {
        Order::new(
            account_id,
            MarketId::new("BTC/USDT"),
            Side::BUY,
            Price::from_u64(price),
            Quantity::from_str(qty).unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        )
    }

    #[test]
    fn test_validate_order_passes() {
        let account = make_account(100_000);
        let order = make_order(account.account_id, 50_000, "0.1");
        // Notional = 0.1 × 50000 = 5000, margin = 5000/10 = 500
        // Available = 100000 (no positions)
        let result = validate_order(&account, &order, &[]);
        assert_eq!(result, RiskCheckResult::Pass);
    }

    #[test]
    fn test_validate_order_insufficient_margin() {
        let account = make_account(100);
        let order = make_order(account.account_id, 50_000, "1.0");
        // Notional = 50000, margin = 5000, available = 100
        let result = validate_order(&account, &order, &[]);
        assert!(matches!(result, RiskCheckResult::InsufficientMargin { .. }));
    }

    #[test]
    fn test_validate_order_inactive_account() {
        let mut account = make_account(100_000);
        account.status = AccountStatus::SUSPENDED;
        let order = make_order(account.account_id, 50_000, "0.1");
        let result = validate_order(&account, &order, &[]);
        assert!(matches!(result, RiskCheckResult::InsufficientMargin { .. }));
    }

    #[test]
    fn test_check_collateral_pass() {
        let result = check_collateral(Decimal::from(10_000), Decimal::from(5_000));
        assert_eq!(result, RiskCheckResult::Pass);
    }

    #[test]
    fn test_check_collateral_fail() {
        let result = check_collateral(Decimal::from(100), Decimal::from(5_000));
        match result {
            RiskCheckResult::InsufficientMargin { required, available } => {
                assert_eq!(required, Decimal::from(5_000));
                assert_eq!(available, Decimal::from(100));
            }
            _ => panic!("Expected InsufficientMargin"),
        }
    }

    #[test]
    fn test_validate_with_existing_positions() {
        let account = make_account(10_000);
        let order = make_order(account.account_id, 50_000, "0.1");

        // Existing position uses maintenance margin
        let position = Position::new(
            account.account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(50_000),
            Price::from_u64(49_500),
            Decimal::from(5_000),
            Decimal::from(5_000), // MM = 5000, eats into available
            10,
            1708123456789000000,
        );

        let result = validate_order(&account, &order, &[position]);
        // Available = equity(10000, 0) - mm(5000) - locked(0) = 5000
        // Required = 5000/10 = 500 → should pass
        assert_eq!(result, RiskCheckResult::Pass);
    }
}
