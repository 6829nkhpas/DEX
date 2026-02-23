//! Market maker bot — spread logic, inventory management, risk limits
//!
//! Implements a deterministic market-making strategy using seeded RNG.
//! Per spec §7 (Fee System), the MM earns maker rebates by providing liquidity.

use crate::engine::SimEngine;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::AccountId;
use types::numeric::Price;
use types::order::Side;

/// Configuration for the market maker bot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketMakerConfig {
    /// Spread in basis points (e.g., 10 = 0.10%)
    pub spread_bps: u32,
    /// Size of each order in base currency
    pub order_size: Decimal,
    /// Maximum net inventory (absolute value of net position)
    pub max_inventory: Decimal,
    /// Maximum daily loss before stopping
    pub max_daily_loss: Decimal,
    /// Maximum number of open orders
    pub max_open_orders: usize,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            spread_bps: 10,
            order_size: Decimal::ONE,
            max_inventory: Decimal::from(10),
            max_daily_loss: Decimal::from(5000),
            max_open_orders: 20,
        }
    }
}

/// Market maker bot state.
pub struct MarketMaker {
    pub account_id: AccountId,
    pub config: MarketMakerConfig,
    pub net_inventory: Decimal,
    pub realized_pnl: Decimal,
    pub orders_placed: usize,
    rng: ChaCha8Rng,
}

impl MarketMaker {
    /// Create a new market maker with a deterministic seed.
    pub fn new(account_id: AccountId, config: MarketMakerConfig, seed: u64) -> Self {
        Self {
            account_id,
            config,
            net_inventory: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            orders_placed: 0,
            rng: ChaCha8Rng::seed_from_u64(seed),
        }
    }

    /// Calculate bid price given mid price and inventory skew.
    ///
    /// When inventory is positive (long), skew bid down to discourage buying.
    pub fn calculate_bid(&self, mid: Decimal) -> Decimal {
        let half_spread = mid * Decimal::from(self.config.spread_bps)
            / Decimal::from(20_000);
        let skew = self.inventory_skew();
        let bid = mid - half_spread - skew;
        if bid > Decimal::ZERO { bid } else { Decimal::ONE }
    }

    /// Calculate ask price given mid price and inventory skew.
    ///
    /// When inventory is positive (long), skew ask down to encourage selling.
    pub fn calculate_ask(&self, mid: Decimal) -> Decimal {
        let half_spread = mid * Decimal::from(self.config.spread_bps)
            / Decimal::from(20_000);
        let skew = self.inventory_skew();
        let ask = mid + half_spread - skew;
        if ask > Decimal::ZERO { ask } else { Decimal::ONE }
    }

    /// Inventory skew: adjusts quotes toward reducing exposure.
    ///
    /// Positive inventory → positive skew → bid lower, ask lower (encourage sells).
    /// Negative inventory → negative skew → bid higher, ask higher.
    fn inventory_skew(&self) -> Decimal {
        let ratio = if self.config.max_inventory > Decimal::ZERO {
            self.net_inventory / self.config.max_inventory
        } else {
            Decimal::ZERO
        };
        // Scale skew: at max inventory, skew is half_spread worth
        ratio * Decimal::from(self.config.spread_bps) / Decimal::from(20_000)
    }

    /// Check if risk limits allow placing more orders.
    pub fn can_quote(&self) -> bool {
        if self.net_inventory.abs() >= self.config.max_inventory {
            return false;
        }
        if self.realized_pnl < -self.config.max_daily_loss {
            return false;
        }
        if self.orders_placed >= self.config.max_open_orders {
            return false;
        }
        true
    }

    /// Generate and submit bid/ask orders to the engine.
    ///
    /// Returns the number of orders placed (0, 1, or 2).
    pub fn tick(&mut self, engine: &mut SimEngine, timestamp: i64) -> usize {
        if !self.can_quote() {
            return 0;
        }

        let mid = match engine.mid_price() {
            Some(m) => m,
            None => return 0,
        };

        let mut count = 0;

        // Place bid if inventory allows
        if self.net_inventory < self.config.max_inventory {
            let bid_price = self.calculate_bid(mid);
            if let Some(p) = Price::try_new(bid_price.round_dp(2)) {
                engine.submit_order(
                    self.account_id, Side::BUY, p, self.config.order_size, timestamp,
                );
                self.orders_placed += 1;
                count += 1;
            }
        }

        // Place ask if inventory allows
        if self.net_inventory > -self.config.max_inventory {
            let ask_price = self.calculate_ask(mid);
            if let Some(p) = Price::try_new(ask_price.round_dp(2)) {
                engine.submit_order(
                    self.account_id, Side::SELL, p, self.config.order_size, timestamp,
                );
                self.orders_placed += 1;
                count += 1;
            }
        }

        count
    }

    /// Update inventory after a fill (positive = bought, negative = sold).
    pub fn record_fill(&mut self, side: Side, quantity: Decimal, price: Decimal) {
        match side {
            Side::BUY => self.net_inventory += quantity,
            Side::SELL => self.net_inventory -= quantity,
        }
    }

    /// Reset daily counters.
    pub fn reset_daily(&mut self) {
        self.realized_pnl = Decimal::ZERO;
        self.orders_placed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_spread_calculation() {
        let config = MarketMakerConfig {
            spread_bps: 20, // 0.20%
            ..Default::default()
        };
        let mm = MarketMaker::new(AccountId::new(), config, 42);

        let mid = Decimal::from(50000);
        let bid = mm.calculate_bid(mid);
        let ask = mm.calculate_ask(mid);

        // Half spread = 50000 * 20 / 20000 = 50
        assert!(bid < mid);
        assert!(ask > mid);
        assert_eq!(ask - bid, Decimal::from(100)); // Full spread = 100
    }

    #[test]
    fn test_inventory_skew() {
        let config = MarketMakerConfig {
            spread_bps: 20,
            max_inventory: Decimal::from(10),
            ..Default::default()
        };
        let mut mm = MarketMaker::new(AccountId::new(), config, 42);

        let mid = Decimal::from(50000);

        // No inventory: symmetric
        let bid_neutral = mm.calculate_bid(mid);
        let ask_neutral = mm.calculate_ask(mid);

        // Positive inventory: skew toward selling
        mm.net_inventory = Decimal::from(5);
        let bid_long = mm.calculate_bid(mid);
        let ask_long = mm.calculate_ask(mid);

        // Long skew: bid should be lower, ask should be lower (encourage sells)
        assert!(bid_long < bid_neutral);
        assert!(ask_long < ask_neutral);
    }

    #[test]
    fn test_risk_limit_inventory() {
        let config = MarketMakerConfig {
            max_inventory: Decimal::from(5),
            ..Default::default()
        };
        let mut mm = MarketMaker::new(AccountId::new(), config, 42);
        assert!(mm.can_quote());

        mm.net_inventory = Decimal::from(5);
        assert!(!mm.can_quote());
    }

    #[test]
    fn test_risk_limit_loss() {
        let config = MarketMakerConfig {
            max_daily_loss: Decimal::from(1000),
            ..Default::default()
        };
        let mut mm = MarketMaker::new(AccountId::new(), config, 42);
        assert!(mm.can_quote());

        mm.realized_pnl = Decimal::from(-1001);
        assert!(!mm.can_quote());
    }

    #[test]
    fn test_tick_places_orders() {
        let mut engine = test_engine();
        let acc = AccountId::new();

        // Seed the book so mid price exists
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(1), 100);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::from(1), 101);

        let config = MarketMakerConfig::default();
        let mut mm = MarketMaker::new(AccountId::new(), config, 42);

        let placed = mm.tick(&mut engine, 200);
        assert_eq!(placed, 2); // bid + ask
    }

    #[test]
    fn test_record_fill() {
        let mut mm = MarketMaker::new(AccountId::new(), MarketMakerConfig::default(), 42);
        mm.record_fill(Side::BUY, Decimal::from(2), Decimal::from(50000));
        assert_eq!(mm.net_inventory, Decimal::from(2));

        mm.record_fill(Side::SELL, Decimal::from(1), Decimal::from(50100));
        assert_eq!(mm.net_inventory, Decimal::from(1));
    }
}
