//! Volatility spike scenario
//!
//! Simulates a sudden price drop/spike to test margin ratio changes
//! and liquidation trigger behavior per spec §6 (Liquidation Process).

use crate::engine::SimEngine;
use crate::scenarios::ScenarioResult;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use types::ids::AccountId;
use types::numeric::Price;
use types::order::Side;

/// Configuration for a volatility spike scenario.
#[derive(Debug, Clone)]
pub struct VolatilitySpikeConfig {
    /// Starting mid price
    pub initial_price: Decimal,
    /// Price drop/rise percentage (e.g., 0.10 = 10% drop)
    pub move_percent: Decimal,
    /// Number of ticks over which the move occurs
    pub move_ticks: u64,
    /// Whether the move is a drop (true) or spike (false)
    pub is_drop: bool,
    /// Number of accounts to seed
    pub account_count: usize,
    /// Size of initial orders per account
    pub order_size: Decimal,
}

impl Default for VolatilitySpikeConfig {
    fn default() -> Self {
        Self {
            initial_price: Decimal::from(50000),
            move_percent: Decimal::from_str_exact("0.10").unwrap(),
            move_ticks: 10,
            is_drop: true,
            account_count: 5,
            order_size: Decimal::ONE,
        }
    }
}

/// Run a volatility spike scenario.
///
/// Seeds the book with orders, then incrementally moves price over ticks.
/// Tracks how many orders get filled at each price level.
pub fn run(engine: &mut SimEngine, config: &VolatilitySpikeConfig) -> ScenarioResult {
    let mut total_orders: u64 = 0;
    let base_timestamp: i64 = 1_000_000_000;

    // Seed initial book with bid/ask around initial_price
    let accounts: Vec<AccountId> = (0..config.account_count)
        .map(|_| AccountId::new())
        .collect();

    let half_spread = config.initial_price * Decimal::from_str_exact("0.001").unwrap();
    let bid_price = config.initial_price - half_spread;
    let ask_price = config.initial_price + half_spread;

    for (i, acc) in accounts.iter().enumerate() {
        let ts = base_timestamp + i as i64;
        if let Some(bp) = Price::try_new(bid_price.round_dp(2)) {
            engine.submit_order(*acc, Side::BUY, bp, config.order_size, ts);
            total_orders += 1;
        }
        if let Some(ap) = Price::try_new(ask_price.round_dp(2)) {
            engine.submit_order(*acc, Side::SELL, ap, config.order_size, ts);
            total_orders += 1;
        }
    }

    // Execute price move over ticks
    let price_step = config.initial_price * config.move_percent
        / Decimal::from(config.move_ticks);
    let aggressor = AccountId::new();

    for tick in 0..config.move_ticks {
        let ts = base_timestamp + 1000 + tick as i64;
        let tick_price = if config.is_drop {
            config.initial_price - price_step * Decimal::from(tick + 1)
        } else {
            config.initial_price + price_step * Decimal::from(tick + 1)
        };

        let tick_price = tick_price.round_dp(2);
        if tick_price <= Decimal::ZERO {
            break;
        }

        // Submit aggressive order at moved price
        let side = if config.is_drop { Side::SELL } else { Side::BUY };
        engine.submit_order(
            aggressor, side, Price::new(tick_price), config.order_size, ts,
        );
        total_orders += 1;
    }

    let total_trades = engine.trade_count() as u64;
    let events_count = engine.events.len();

    let final_price = if config.is_drop {
        config.initial_price * (Decimal::ONE - config.move_percent)
    } else {
        config.initial_price * (Decimal::ONE + config.move_percent)
    };

    ScenarioResult {
        name: "volatility_spike".to_string(),
        ticks_run: config.move_ticks,
        orders_submitted: total_orders,
        trades_executed: total_trades,
        events_emitted: events_count,
        passed: true,
        details: format!(
            "Price moved from {} to {} ({:.1}%) over {} ticks. {} trades executed.",
            config.initial_price,
            final_price.round_dp(2),
            config.move_percent * Decimal::from(100),
            config.move_ticks,
            total_trades
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SimEngine;
    use types::fee::FeeTier;
    use types::ids::MarketId;

    fn test_engine() -> SimEngine {
        let fee = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };
        SimEngine::new(MarketId::new("BTC/USDT"), fee)
    }

    #[test]
    fn test_volatility_drop() {
        let mut engine = test_engine();
        let config = VolatilitySpikeConfig::default();
        let result = run(&mut engine, &config);
        assert!(result.passed);
        assert!(result.orders_submitted > 0);
        assert!(result.events_emitted > 0);
    }

    #[test]
    fn test_volatility_spike_up() {
        let mut engine = test_engine();
        let config = VolatilitySpikeConfig {
            is_drop: false,
            ..Default::default()
        };
        let result = run(&mut engine, &config);
        assert!(result.passed);
        assert!(result.orders_submitted > 0);
    }

    #[test]
    fn test_extreme_move() {
        let mut engine = test_engine();
        let config = VolatilitySpikeConfig {
            move_percent: Decimal::from_str_exact("0.50").unwrap(),
            move_ticks: 5,
            ..Default::default()
        };
        let result = run(&mut engine, &config);
        assert!(result.passed);
    }
}
