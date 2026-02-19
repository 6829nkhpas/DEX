//! Risk Engine — orchestrator
//!
//! Ties together margin, exposure, liquidation, validation,
//! and event emission per specs §5, §6, §9.3.6.

use rust_decimal::Decimal;
use types::account::Account;
use types::ids::AccountId;
use types::order::Order;
use types::position::Position;
use types::risk::RiskCheckResult;

use crate::events::{self, RiskEvent};
use crate::exposure;
use crate::liquidation;
use crate::margin;
use crate::validator;

/// Risk engine configuration
#[derive(Debug, Clone)]
pub struct RiskEngineConfig {
    /// Default leverage for futures accounts
    pub default_futures_leverage: u8,
    /// Margin ratio threshold for warning events
    pub warning_threshold: Decimal,
    /// Margin ratio threshold for margin call events
    pub margin_call_threshold: Decimal,
    /// Margin ratio threshold for liquidation
    pub liquidation_threshold: Decimal,
}

impl Default for RiskEngineConfig {
    fn default() -> Self {
        Self {
            default_futures_leverage: 10,
            warning_threshold: Decimal::from_str_exact("2.0").unwrap(),
            margin_call_threshold: Decimal::from_str_exact("1.2").unwrap(),
            liquidation_threshold: Decimal::from_str_exact("1.1").unwrap(),
        }
    }
}

/// Risk engine service
#[derive(Debug, Clone)]
pub struct RiskEngine {
    config: RiskEngineConfig,
}

impl RiskEngine {
    /// Create a new risk engine with default configuration
    pub fn new() -> Self {
        Self {
            config: RiskEngineConfig::default(),
        }
    }

    /// Create a new risk engine with custom configuration
    pub fn with_config(config: RiskEngineConfig) -> Self {
        Self { config }
    }

    /// Pre-trade risk check per spec §9.3.6
    ///
    /// Validates an incoming order and returns Pass or rejection reason.
    /// If rejected, also returns a RiskCheckFailed event.
    pub fn check_pre_trade(
        &self,
        account: &Account,
        order: &Order,
        positions: &[Position],
        timestamp: i64,
    ) -> (RiskCheckResult, Vec<RiskEvent>) {
        let result = validator::validate_order(account, order, positions);

        let mut risk_events = Vec::new();
        if result != RiskCheckResult::Pass {
            risk_events.push(events::risk_check_failed_event(
                account.account_id,
                format!("{:?}", result),
                timestamp,
            ));
        }

        (result, risk_events)
    }

    /// Evaluate account health and generate risk events.
    ///
    /// Called periodically or on mark price updates per spec §6.2.2.
    pub fn evaluate_account(
        &self,
        account: &Account,
        positions: &[Position],
        timestamp: i64,
    ) -> Vec<RiskEvent> {
        if positions.is_empty() {
            return Vec::new();
        }

        let total_balance: Decimal = account
            .balances
            .values()
            .map(|b| b.total)
            .sum();

        let total_upnl = exposure::total_unrealized_pnl(positions);
        let eq = exposure::equity(total_balance, total_upnl);
        let total_mm = exposure::total_maintenance_margin(positions);
        let ratio = margin::margin_ratio(eq, total_mm);
        let health = liquidation::health_status(ratio);

        events::events_for_health(
            account.account_id,
            health,
            ratio,
            eq,
            total_mm,
            timestamp,
        )
    }

    /// Post-trade update: re-evaluate account after a trade executes.
    ///
    /// Returns any risk events triggered by the new position state.
    pub fn post_trade_update(
        &self,
        account: &Account,
        positions: &[Position],
        timestamp: i64,
    ) -> Vec<RiskEvent> {
        self.evaluate_account(account, positions, timestamp)
    }

    /// Calculate margin requirement for an order.
    pub fn compute_order_margin(
        &self,
        quantity: Decimal,
        price: Decimal,
        leverage: u8,
    ) -> Decimal {
        margin::order_margin(quantity, price, leverage)
    }

    /// Get current margin ratio for an account.
    pub fn get_margin_ratio(
        &self,
        account: &Account,
        positions: &[Position],
    ) -> Decimal {
        let total_balance: Decimal = account
            .balances
            .values()
            .map(|b| b.total)
            .sum();

        let total_upnl = exposure::total_unrealized_pnl(positions);
        let eq = exposure::equity(total_balance, total_upnl);
        let total_mm = exposure::total_maintenance_margin(positions);

        margin::margin_ratio(eq, total_mm)
    }
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::account::{Account, AccountType, Balance};
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

    fn make_position(
        account_id: AccountId,
        side: PositionSide,
        size: &str,
        entry: u64,
        mark: u64,
        im: i64,
        mm: i64,
    ) -> Position {
        Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            side,
            Quantity::from_str(size).unwrap(),
            Price::from_u64(entry),
            Price::from_u64(mark),
            Price::from_u64(if entry > 500 { entry - 500 } else { 1 }),
            Decimal::from(im),
            Decimal::from(mm),
            10,
            1708123456789000000,
        )
    }

    // ── Pre-trade check tests ──

    #[test]
    fn test_pre_trade_pass() {
        let engine = RiskEngine::new();
        let account = make_account(100_000);
        let order = make_order(account.account_id, 50_000, "0.1");

        let (result, events) = engine.check_pre_trade(
            &account, &order, &[], 1708123456789000000,
        );
        assert_eq!(result, RiskCheckResult::Pass);
        assert!(events.is_empty());
    }

    #[test]
    fn test_pre_trade_insufficient_collateral() {
        let engine = RiskEngine::new();
        let account = make_account(100);
        let order = make_order(account.account_id, 50_000, "1.0");

        let (result, events) = engine.check_pre_trade(
            &account, &order, &[], 1708123456789000000,
        );
        assert!(matches!(result, RiskCheckResult::InsufficientMargin { .. }));
        assert_eq!(events.len(), 1);
    }

    // ── Account evaluation tests ──

    #[test]
    fn test_evaluate_healthy() {
        let engine = RiskEngine::new();
        let account = make_account(100_000);
        let pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            51_000,
            5_000,
            500,
        );

        let events = engine.evaluate_account(
            &account, &[pos], 1708123456789000000,
        );
        // Equity = 100000 + 1000 = 101000, MM = 500, ratio = 202
        assert!(events.is_empty());
    }

    #[test]
    fn test_evaluate_liquidation_triggered() {
        let engine = RiskEngine::new();
        let account = make_account(5_000);
        // Large loss pushes into liquidation
        let pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            45_500,
            5_000,
            5_000,
        );

        let events = engine.evaluate_account(
            &account, &[pos], 1708123456789000000,
        );
        // Equity = 5000 + (-4500) = 500, MM = 5000, ratio = 0.1
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            events::RiskEventType::LiquidationTriggered
        ));
    }

    #[test]
    fn test_evaluate_no_positions() {
        let engine = RiskEngine::new();
        let account = make_account(100_000);
        let events = engine.evaluate_account(
            &account, &[], 1708123456789000000,
        );
        assert!(events.is_empty());
    }

    // ── Post-trade update tests ──

    #[test]
    fn test_post_trade_emits_warning() {
        let engine = RiskEngine::new();
        let account = make_account(3_500);
        let pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            50_000,
            5_000,
            2_000,
        );

        let events = engine.post_trade_update(
            &account, &[pos], 1708123456789000000,
        );
        // Equity = 3500 + 0 = 3500, MM = 2000, ratio = 1.75 → Warning
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event_type,
            events::RiskEventType::MarginWarning
        ));
    }

    // ── Margin ratio tests ──

    #[test]
    fn test_get_margin_ratio_no_positions() {
        let engine = RiskEngine::new();
        let account = make_account(100_000);
        let ratio = engine.get_margin_ratio(&account, &[]);
        assert_eq!(ratio, Decimal::MAX);
    }

    #[test]
    fn test_get_margin_ratio_with_position() {
        let engine = RiskEngine::new();
        let account = make_account(10_000);
        let pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            51_000,
            5_000,
            1_000,
        );
        let ratio = engine.get_margin_ratio(&account, &[pos]);
        // Equity = 10000 + 1000 = 11000, MM = 1000, ratio = 11
        assert_eq!(ratio, Decimal::from(11));
    }

    // ── Simulation test ──

    #[test]
    fn test_simulation_sequential_orders() {
        let engine = RiskEngine::new();
        let account = make_account(50_000);

        // First order passes
        let order1 = make_order(account.account_id, 50_000, "0.5");
        let (r1, _) = engine.check_pre_trade(
            &account, &order1, &[], 1708123456789000000,
        );
        assert_eq!(r1, RiskCheckResult::Pass);

        // Simulate position after fill
        let pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "0.5",
            50_000,
            50_000,
            2_500,
            250,
        );

        // Second order still passes with reduced available
        let order2 = make_order(account.account_id, 50_000, "0.5");
        let (r2, _) = engine.check_pre_trade(
            &account, &order2, &[pos.clone()], 1708123456789000000,
        );
        assert_eq!(r2, RiskCheckResult::Pass);
    }

    // ── Extreme volatility test ──

    #[test]
    fn test_extreme_volatility_50pct_drop() {
        let engine = RiskEngine::new();
        let account = make_account(10_000);

        // Position at $50,000
        let mut pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            50_000,
            5_000,
            2_500,
        );

        // 50% price drop: mark drops to $25,000
        pos.update_mark_price(
            Price::from_u64(25_000),
            1708123456790000000,
        );

        let events = engine.evaluate_account(
            &account, &[pos.clone()], 1708123456790000000,
        );

        // Equity = 10000 + (-25000) = -15000
        // MM = 2500, ratio = -6 → Liquidation
        assert!(!events.is_empty());
        assert!(matches!(
            events[0].event_type,
            events::RiskEventType::LiquidationTriggered
        ));
    }

    #[test]
    fn test_extreme_volatility_gradual_decline() {
        let engine = RiskEngine::new();
        let account = make_account(6_000);

        let mut pos = make_position(
            account.account_id,
            PositionSide::LONG,
            "1.0",
            50_000,
            50_000,
            5_000,
            3_000,
        );

        // 1% drop: 49500
        pos.update_mark_price(Price::from_u64(49_500), 1);
        let e1 = engine.evaluate_account(&account, &[pos.clone()], 1);
        // Equity = 6000 - 500 = 5500, MM = 3000, ratio ≈ 1.83 → Warning
        assert_eq!(e1.len(), 1);
        assert!(matches!(e1[0].event_type, events::RiskEventType::MarginWarning));

        // 5% drop: 47500
        pos.update_mark_price(Price::from_u64(47_500), 2);
        let e2 = engine.evaluate_account(&account, &[pos.clone()], 2);
        // Equity = 6000 - 2500 = 3500, MM = 3000, ratio ≈ 1.17 → Danger
        assert_eq!(e2.len(), 1);
        assert!(matches!(e2[0].event_type, events::RiskEventType::MarginCall));

        // 8% drop: 46000
        pos.update_mark_price(Price::from_u64(46_000), 3);
        let e3 = engine.evaluate_account(&account, &[pos.clone()], 3);
        // Equity = 6000 - 4000 = 2000, MM = 3000, ratio ≈ 0.67 → Liquidation
        assert_eq!(e3.len(), 1);
        assert!(matches!(
            e3[0].event_type,
            events::RiskEventType::LiquidationTriggered
        ));
    }
}
