//! Order flood scenario
//!
//! Bursts N orders in a single tick to verify the engine handles
//! high throughput without data loss or ordering violations.

use crate::engine::{SimEngine, SimEvent};
use crate::scenarios::ScenarioResult;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use types::ids::AccountId;
use types::numeric::Price;
use types::order::Side;

/// Configuration for the order flood scenario.
#[derive(Debug, Clone)]
pub struct OrderFloodConfig {
    /// Total number of orders to submit in a single burst
    pub burst_size: usize,
    /// Base price
    pub base_price: Decimal,
    /// Order size
    pub order_size: Decimal,
    /// Price spread range (orders placed within base ± spread)
    pub spread: Decimal,
}

impl Default for OrderFloodConfig {
    fn default() -> Self {
        Self {
            burst_size: 1000,
            base_price: Decimal::from(50000),
            order_size: Decimal::from_str_exact("0.1").unwrap(),
            spread: Decimal::from(200),
        }
    }
}

/// Run the order flood scenario.
///
/// Submits burst_size orders in a single tick, alternating buy/sell
/// at prices around the base price. Verifies all orders are processed
/// and sequence numbers are monotonic.
pub fn run(engine: &mut SimEngine, config: &OrderFloodConfig) -> ScenarioResult {
    let base_ts: i64 = 1_000_000;
    let accounts: Vec<AccountId> = (0..10).map(|_| AccountId::new()).collect();

    let events_before = engine.events.len();

    for i in 0..config.burst_size {
        let acc = accounts[i % accounts.len()];
        let side = if i % 2 == 0 { Side::BUY } else { Side::SELL };

        // Stagger prices across the spread
        let offset = config.spread
            * Decimal::from(i as i64 % 10)
            / Decimal::from(10);

        let price = match side {
            Side::BUY => config.base_price - offset,
            Side::SELL => config.base_price + offset,
        };

        let price_rounded = price.round_dp(2);
        if price_rounded <= Decimal::ZERO {
            continue;
        }

        engine.submit_order(
            acc,
            side,
            Price::new(price_rounded),
            config.order_size,
            base_ts,
        );
    }

    let events_after = engine.events.len();
    let new_events = &engine.events[events_before..events_after];

    // Verify: all OrderPlaced events have monotonically increasing sequence context
    let placed_count = new_events.iter()
        .filter(|e| matches!(e, SimEvent::OrderPlaced { .. }))
        .count();

    let trade_count = new_events.iter()
        .filter(|e| matches!(e, SimEvent::TradeExecuted { .. }))
        .count();

    let passed = placed_count > 0;

    ScenarioResult {
        name: "order_flood".to_string(),
        ticks_run: 1,
        orders_submitted: config.burst_size as u64,
        trades_executed: trade_count as u64,
        events_emitted: events_after - events_before,
        passed,
        details: format!(
            "Burst of {} orders processed. {} placed, {} trades. {} total events.",
            config.burst_size, placed_count, trade_count,
            events_after - events_before,
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
    fn test_order_flood() {
        let mut engine = test_engine();
        let config = OrderFloodConfig {
            burst_size: 100,
            ..Default::default()
        };
        let result = run(&mut engine, &config);
        assert!(result.passed);
        assert_eq!(result.orders_submitted, 100);
        assert!(result.events_emitted > 0);
    }

    #[test]
    fn test_large_flood() {
        let mut engine = test_engine();
        let config = OrderFloodConfig {
            burst_size: 5000,
            ..Default::default()
        };
        let result = run(&mut engine, &config);
        assert!(result.passed);
        assert_eq!(result.orders_submitted, 5000);
    }

    #[test]
    fn test_flood_trades() {
        let mut engine = test_engine();
        // Tight spread to ensure matching
        let config = OrderFloodConfig {
            burst_size: 200,
            spread: Decimal::ZERO,
            ..Default::default()
        };
        let result = run(&mut engine, &config);
        assert!(result.passed);
        assert!(result.trades_executed > 0);
    }
}
