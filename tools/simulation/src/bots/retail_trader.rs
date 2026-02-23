//! Retail random trader bot
//!
//! Generates random orders with deterministic seeded RNG for simulation.
//! Produces a mix of market-like and limit orders to simulate retail flow.

use crate::engine::SimEngine;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::AccountId;
use types::numeric::Price;
use types::order::Side;

/// Configuration for the retail random trader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetailTraderConfig {
    /// Minimum order size
    pub min_size: Decimal,
    /// Maximum order size
    pub max_size: Decimal,
    /// Probability of market order (0.0 to 1.0)
    pub market_order_ratio: f64,
    /// Maximum distance from mid price for limit orders (in bps)
    pub max_limit_distance_bps: u32,
}

impl Default for RetailTraderConfig {
    fn default() -> Self {
        Self {
            min_size: Decimal::from_str_exact("0.01").unwrap(),
            max_size: Decimal::from_str_exact("1.0").unwrap(),
            market_order_ratio: 0.3,
            max_limit_distance_bps: 50,
        }
    }
}

/// Generated order parameters from the retail trader.
#[derive(Debug, Clone)]
pub struct RetailOrder {
    pub side: Side,
    pub price: Price,
    pub size: Decimal,
    pub is_market: bool,
}

/// Retail random trader with deterministic seeded RNG.
pub struct RetailTrader {
    pub account_id: AccountId,
    pub config: RetailTraderConfig,
    pub orders_submitted: usize,
    rng: ChaCha8Rng,
}

impl RetailTrader {
    /// Create a new retail trader with a deterministic seed.
    pub fn new(account_id: AccountId, config: RetailTraderConfig, seed: u64) -> Self {
        Self {
            account_id,
            config,
            orders_submitted: 0,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Generate a random order based on current mid price.
    ///
    /// Returns None if mid price is unavailable.
    pub fn generate_order(&mut self, mid_price: Decimal) -> Option<RetailOrder> {
        if mid_price <= Decimal::ZERO {
            return None;
        }

        // Random side
        let side = if self.rng.gen_bool(0.5) { Side::BUY } else { Side::SELL };

        // Random size within range
        let min_f = self.config.min_size.to_f64().unwrap_or(0.01);
        let max_f = self.config.max_size.to_f64().unwrap_or(1.0);
        let size_f: f64 = self.rng.gen_range(min_f..=max_f);
        let size = Decimal::from_f64(size_f)
            .unwrap_or(self.config.min_size)
            .round_dp(8);
        let size = if size <= Decimal::ZERO { self.config.min_size } else { size };

        // Market or limit?
        let is_market = self.rng.gen_bool(self.config.market_order_ratio);

        let price = if is_market {
            // For market-like orders: use aggressive price
            match side {
                Side::BUY => {
                    let aggressive = mid_price * Decimal::from_str_exact("1.01").unwrap();
                    Price::new(aggressive.round_dp(2))
                }
                Side::SELL => {
                    let aggressive = mid_price * Decimal::from_str_exact("0.99").unwrap();
                    let rounded = aggressive.round_dp(2);
                    if rounded > Decimal::ZERO {
                        Price::new(rounded)
                    } else {
                        Price::new(Decimal::ONE)
                    }
                }
            }
        } else {
            // Limit order: random distance from mid
            let bps: u32 = self.rng.gen_range(1..=self.config.max_limit_distance_bps);
            let distance = mid_price * Decimal::from(bps) / Decimal::from(10_000);
            match side {
                Side::BUY => {
                    let p = (mid_price - distance).round_dp(2);
                    if p > Decimal::ZERO { Price::new(p) } else { Price::new(Decimal::ONE) }
                }
                Side::SELL => {
                    Price::new((mid_price + distance).round_dp(2))
                }
            }
        };

        self.orders_submitted += 1;
        Some(RetailOrder { side, price, size, is_market })
    }

    /// Generate and submit an order directly to the engine.
    ///
    /// Returns true if an order was submitted.
    pub fn tick(&mut self, engine: &mut SimEngine, timestamp: i64) -> bool {
        let mid = match engine.mid_price() {
            Some(m) => m,
            None => return false,
        };

        if let Some(order) = self.generate_order(mid) {
            engine.submit_order(
                self.account_id,
                order.side,
                order.price,
                order.size,
                timestamp,
            );
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::MarketId;
    use types::fee::FeeTier;
    use crate::engine::SimEngine;

    fn test_engine() -> SimEngine {
        let fee = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };
        SimEngine::new(MarketId::new("BTC/USDT"), fee)
    }

    #[test]
    fn test_deterministic_output() {
        let mid = Decimal::from(50000);

        let mut t1 = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 42);
        let mut t2 = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 42);

        let o1 = t1.generate_order(mid).unwrap();
        let o2 = t2.generate_order(mid).unwrap();

        assert_eq!(o1.side, o2.side);
        assert_eq!(o1.price, o2.price);
        assert_eq!(o1.size, o2.size);
        assert_eq!(o1.is_market, o2.is_market);
    }

    #[test]
    fn test_order_validity() {
        let mid = Decimal::from(50000);
        let mut trader = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 123);

        for _ in 0..100 {
            let order = trader.generate_order(mid).unwrap();
            assert!(order.price.as_decimal() > Decimal::ZERO);
            assert!(order.size > Decimal::ZERO);
        }
    }

    #[test]
    fn test_different_seeds_different_output() {
        let mid = Decimal::from(50000);
        let mut t1 = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 1);
        let mut t2 = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 2);

        let mut same_count = 0;
        for _ in 0..10 {
            let o1 = t1.generate_order(mid).unwrap();
            let o2 = t2.generate_order(mid).unwrap();
            if o1.side == o2.side && o1.size == o2.size {
                same_count += 1;
            }
        }
        // Extremely unlikely all 10 are the same
        assert!(same_count < 10);
    }

    #[test]
    fn test_tick_with_engine() {
        let mut engine = test_engine();
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(10), 100);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::from(10), 101);

        let mut trader = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 42);
        let submitted = trader.tick(&mut engine, 200);
        assert!(submitted);
        assert_eq!(trader.orders_submitted, 1);
    }

    #[test]
    fn test_no_mid_price_returns_none() {
        let mut trader = RetailTrader::new(AccountId::new(), RetailTraderConfig::default(), 42);
        let result = trader.generate_order(Decimal::ZERO);
        assert!(result.is_none());
    }
}
