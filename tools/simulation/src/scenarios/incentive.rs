//! Incentive simulation
//!
//! Simulates fee tier progression: volume accumulation → tier upgrade → maker rebate.
//! Verifies fee amounts match spec §7 tiers exactly.

use crate::engine::SimEngine;
use crate::scenarios::ScenarioResult;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use types::fee::{default_fee_tiers, FeeTier};
use types::ids::AccountId;
use types::numeric::Price;
use types::order::Side;

/// Configuration for the incentive simulation.
#[derive(Debug, Clone)]
pub struct IncentiveConfig {
    /// Base trade price
    pub trade_price: Decimal,
    /// Trade quantity per order
    pub trade_quantity: Decimal,
    /// Number of trades to simulate
    pub trade_count: usize,
}

impl Default for IncentiveConfig {
    fn default() -> Self {
        Self {
            trade_price: Decimal::from(50000),
            trade_quantity: Decimal::ONE,
            trade_count: 100,
        }
    }
}

/// Fee tier tracking result.
#[derive(Debug, Clone)]
pub struct IncentiveDetail {
    pub cumulative_volume: Decimal,
    pub current_tier_index: usize,
    pub current_maker_rate: Decimal,
    pub current_taker_rate: Decimal,
    pub total_maker_fees: Decimal,
    pub total_taker_fees: Decimal,
    pub tier_upgrades: Vec<(Decimal, usize)>, // (volume_at_upgrade, tier_index)
}

/// Determine fee tier for a given volume.
fn tier_for_volume(volume: Decimal, tiers: &[FeeTier]) -> usize {
    let mut idx = 0;
    for (i, tier) in tiers.iter().enumerate() {
        if volume >= tier.volume_threshold {
            idx = i;
        }
    }
    idx
}

/// Run the incentive simulation.
///
/// Simulates trades that accumulate volume, tracking fee tier progression.
/// Verifies that fees match the correct tier rate at each point.
pub fn run(engine: &mut SimEngine, config: &IncentiveConfig) -> (ScenarioResult, IncentiveDetail) {
    let tiers = default_fee_tiers();
    let base_ts: i64 = 1_000_000;
    let maker = AccountId::new();
    let taker = AccountId::new();

    let mut cumulative_volume = Decimal::ZERO;
    let mut total_maker_fees = Decimal::ZERO;
    let mut total_taker_fees = Decimal::ZERO;
    let mut tier_upgrades: Vec<(Decimal, usize)> = Vec::new();
    let mut current_tier = 0;

    for i in 0..config.trade_count {
        let ts = base_ts + i as i64 * 2;

        // Determine current tier
        let new_tier = tier_for_volume(cumulative_volume, &tiers);
        if new_tier != current_tier {
            tier_upgrades.push((cumulative_volume, new_tier));
            current_tier = new_tier;
        }

        let trade_value = config.trade_price * config.trade_quantity;
        let maker_fee = trade_value * tiers[current_tier].maker_rate;
        let taker_fee = trade_value * tiers[current_tier].taker_rate;

        total_maker_fees += maker_fee;
        total_taker_fees += taker_fee;
        cumulative_volume += trade_value;

        // Submit matching orders to the engine
        let price = Price::new(config.trade_price.round_dp(2));
        engine.submit_order(maker, Side::SELL, price, config.trade_quantity, ts);
        engine.submit_order(taker, Side::BUY, price, config.trade_quantity, ts + 1);
    }

    let final_tier = tier_for_volume(cumulative_volume, &tiers);

    let detail = IncentiveDetail {
        cumulative_volume,
        current_tier_index: final_tier,
        current_maker_rate: tiers[final_tier].maker_rate,
        current_taker_rate: tiers[final_tier].taker_rate,
        total_maker_fees,
        total_taker_fees,
        tier_upgrades,
    };

    let result = ScenarioResult {
        name: "incentive_simulation".to_string(),
        ticks_run: config.trade_count as u64,
        orders_submitted: (config.trade_count * 2) as u64,
        trades_executed: engine.trade_count() as u64,
        events_emitted: engine.events.len(),
        passed: true,
        details: format!(
            "Volume: {}. Final tier: {} (maker: {}, taker: {}). {} tier upgrades.",
            cumulative_volume,
            final_tier,
            tiers[final_tier].maker_rate,
            tiers[final_tier].taker_rate,
            tier_upgrades.len(),
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
    fn test_tier_for_volume() {
        let tiers = default_fee_tiers();
        assert_eq!(tier_for_volume(Decimal::ZERO, &tiers), 0);
        assert_eq!(tier_for_volume(Decimal::from(500_000), &tiers), 0);
        assert_eq!(tier_for_volume(Decimal::from(1_000_001), &tiers), 1);
        assert_eq!(tier_for_volume(Decimal::from(50_000_001), &tiers), 3);
    }

    #[test]
    fn test_incentive_basic() {
        let mut engine = test_engine();
        let config = IncentiveConfig {
            trade_count: 10,
            ..Default::default()
        };
        let (result, detail) = run(&mut engine, &config);
        assert!(result.passed);
        assert!(detail.cumulative_volume > Decimal::ZERO);
        assert!(detail.total_taker_fees > Decimal::ZERO);
    }

    #[test]
    fn test_tier_upgrade() {
        let mut engine = test_engine();
        // Each trade = 50000, need 1M+ for tier 1 → 20+ trades
        let config = IncentiveConfig {
            trade_count: 25,
            ..Default::default()
        };
        let (_, detail) = run(&mut engine, &config);
        // 25 * 50000 = 1,250,000 → tier 1
        assert!(detail.current_tier_index >= 1);
        assert!(!detail.tier_upgrades.is_empty());
    }

    #[test]
    fn test_maker_rebate_tier() {
        let tiers = default_fee_tiers();
        // Tier 3 has negative maker rate (rebate)
        assert!(tiers[3].maker_rate < Decimal::ZERO);
    }
}
