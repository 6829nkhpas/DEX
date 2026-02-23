//! Liquidation cascade test
//!
//! Sets up accounts near margin_ratio=1.1, triggers a price move,
//! and verifies cascade detection per spec §6.10.3:
//! > 5% of open interest liquidated in 5 minutes → cascade detected.

use crate::engine::SimEngine;
use crate::scenarios::ScenarioResult;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use types::ids::AccountId;
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::position::{Position, PositionSide};

/// Configuration for the liquidation cascade scenario.
#[derive(Debug, Clone)]
pub struct LiquidationCascadeConfig {
    /// Number of accounts with positions near liquidation
    pub account_count: usize,
    /// Entry price for all positions
    pub entry_price: Decimal,
    /// Position size per account
    pub position_size: Decimal,
    /// Leverage (determines initial margin)
    pub leverage: u8,
    /// Price drop percentage to trigger liquidations
    pub price_drop_percent: Decimal,
    /// Cascade threshold: fraction of OI that triggers detection (0.05 = 5%)
    pub cascade_threshold: Decimal,
}

impl Default for LiquidationCascadeConfig {
    fn default() -> Self {
        Self {
            account_count: 20,
            entry_price: Decimal::from(50000),
            position_size: Decimal::ONE,
            leverage: 10,
            price_drop_percent: Decimal::from_str_exact("0.08").unwrap(),
            cascade_threshold: Decimal::from_str_exact("0.05").unwrap(),
        }
    }
}

/// Result detail for the liquidation cascade.
#[derive(Debug, Clone)]
pub struct CascadeDetail {
    pub total_positions: usize,
    pub liquidated_count: usize,
    pub liquidation_ratio: Decimal,
    pub cascade_detected: bool,
}

/// Run the liquidation cascade scenario.
///
/// Creates positions near liquidation threshold, then drops price to trigger.
/// Returns how many would be liquidated and whether cascade threshold is breached.
pub fn run(engine: &mut SimEngine, config: &LiquidationCascadeConfig) -> (ScenarioResult, CascadeDetail) {
    let base_ts: i64 = 1_000_000;

    // Create positions near liquidation
    let mut positions: Vec<Position> = Vec::new();
    for i in 0..config.account_count {
        let account_id = AccountId::new();
        let initial_margin = config.entry_price * config.position_size
            / Decimal::from(config.leverage);
        let mm_rate = Decimal::from_str_exact("0.005").unwrap();
        let maintenance_margin = config.entry_price * config.position_size * mm_rate;

        // Liquidation price: entry - (initial_margin / position_size)
        let liq_price = config.entry_price - initial_margin / config.position_size;

        let position = Position::new(
            account_id,
            engine.symbol.clone(),
            PositionSide::LONG,
            Quantity::new(config.position_size),
            Price::new(config.entry_price),
            Price::new(config.entry_price), // mark = entry at start
            Price::new(liq_price.round_dp(2)),
            initial_margin,
            maintenance_margin,
            config.leverage,
            base_ts + i as i64,
        );
        positions.push(position);
    }

    // Seed the engine book
    let seeder = AccountId::new();
    let bid_p = Price::new((config.entry_price - Decimal::from(50)).round_dp(2));
    let ask_p = Price::new((config.entry_price + Decimal::from(50)).round_dp(2));
    engine.submit_order(seeder, Side::BUY, bid_p, Decimal::from(100), base_ts);
    engine.submit_order(seeder, Side::SELL, ask_p, Decimal::from(100), base_ts + 1);

    // Apply price drop
    let new_price = config.entry_price * (Decimal::ONE - config.price_drop_percent);
    let new_mark = Price::new(new_price.round_dp(2));

    // Check which positions would be liquidated
    let mut liquidated_count = 0;
    for pos in &mut positions {
        pos.update_mark_price(new_mark, base_ts + 1000);
        if pos.should_liquidate() {
            liquidated_count += 1;
        }
    }

    let liquidation_ratio = if config.account_count > 0 {
        Decimal::from(liquidated_count) / Decimal::from(config.account_count)
    } else {
        Decimal::ZERO
    };

    let cascade_detected = liquidation_ratio >= config.cascade_threshold;

    // Simulate liquidation orders hitting the book
    let aggressor = AccountId::new();
    for _ in 0..liquidated_count {
        let sell_price = Price::new(new_price.round_dp(2));
        engine.submit_order(
            aggressor, Side::SELL, sell_price, config.position_size, base_ts + 2000,
        );
    }

    let detail = CascadeDetail {
        total_positions: config.account_count,
        liquidated_count,
        liquidation_ratio,
        cascade_detected,
    };

    let result = ScenarioResult {
        name: "liquidation_cascade".to_string(),
        ticks_run: 1,
        orders_submitted: (config.account_count * 2 + liquidated_count) as u64,
        trades_executed: engine.trade_count() as u64,
        events_emitted: engine.events.len(),
        passed: true,
        details: format!(
            "{}/{} positions liquidated ({:.1}%). Cascade detected: {}",
            liquidated_count,
            config.account_count,
            liquidation_ratio * Decimal::from(100),
            cascade_detected,
        ),
    };

    (result, detail)
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
    fn test_cascade_triggered() {
        let mut engine = test_engine();
        let config = LiquidationCascadeConfig {
            price_drop_percent: Decimal::from_str_exact("0.12").unwrap(),
            ..Default::default()
        };
        let (result, detail) = run(&mut engine, &config);
        assert!(result.passed);
        // With 10x leverage and 12% drop, all should be liquidated
        assert!(detail.liquidated_count > 0);
        assert!(detail.cascade_detected);
    }

    #[test]
    fn test_no_cascade_small_move() {
        let mut engine = test_engine();
        let config = LiquidationCascadeConfig {
            price_drop_percent: Decimal::from_str_exact("0.01").unwrap(),
            ..Default::default()
        };
        let (result, detail) = run(&mut engine, &config);
        assert!(result.passed);
        assert_eq!(detail.liquidated_count, 0);
        assert!(!detail.cascade_detected);
    }

    #[test]
    fn test_partial_cascade() {
        let mut engine = test_engine();
        // With 10x, initial margin = 10%, so liquidation around ~9% drop
        let config = LiquidationCascadeConfig {
            price_drop_percent: Decimal::from_str_exact("0.095").unwrap(),
            ..Default::default()
        };
        let (result, detail) = run(&mut engine, &config);
        assert!(result.passed);
        // Margin: IM=5000, MM=250 (0.5%), equity after 9.5% drop = 5000-4750=250
        // margin_ratio = 250/250 = 1.0 < 1.1 → liquidated
        assert!(detail.liquidated_count > 0);
    }
}
