//! Multi-market simulation
//!
//! Runs independent engine instances per market symbol.
//! Aggregates cross-market metrics.

use crate::engine::SimEngine;
use crate::metrics::SimMetrics;
use rust_decimal::Decimal;
use types::fee::FeeTier;
use types::ids::MarketId;

/// A multi-market simulation runner.
pub struct MultiMarketSim {
    pub engines: Vec<SimEngine>,
}

impl MultiMarketSim {
    /// Create a multi-market simulation from market symbols and shared fee tier.
    pub fn new(symbols: Vec<MarketId>, fee_tier: FeeTier) -> Self {
        let engines = symbols.into_iter()
            .map(|sym| SimEngine::new(sym, fee_tier.clone()))
            .collect();
        Self { engines }
    }

    /// Get engine for a specific market index.
    pub fn engine(&self, index: usize) -> Option<&SimEngine> {
        self.engines.get(index)
    }

    /// Get mutable engine for a specific market index.
    pub fn engine_mut(&mut self, index: usize) -> Option<&mut SimEngine> {
        self.engines.get_mut(index)
    }

    /// Get engine by symbol.
    pub fn engine_by_symbol(&self, symbol: &str) -> Option<&SimEngine> {
        self.engines.iter().find(|e| e.symbol.as_str() == symbol)
    }

    /// Get mutable engine by symbol.
    pub fn engine_by_symbol_mut(&mut self, symbol: &str) -> Option<&mut SimEngine> {
        self.engines.iter_mut().find(|e| e.symbol.as_str() == symbol)
    }

    /// Number of markets.
    pub fn market_count(&self) -> usize {
        self.engines.len()
    }

    /// Aggregate metrics across all markets.
    pub fn aggregate_metrics(&self) -> SimMetrics {
        let mut combined = SimMetrics::new();
        for engine in &self.engines {
            combined.ingest_events(&engine.events);
        }
        combined
    }

    /// Total orders across all markets.
    pub fn total_orders(&self) -> usize {
        self.engines.iter().map(|e| e.order_count()).sum()
    }

    /// Total trades across all markets.
    pub fn total_trades(&self) -> usize {
        self.engines.iter().map(|e| e.trade_count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::AccountId;
    use types::numeric::Price;
    use types::order::Side;

    fn test_fee() -> FeeTier {
        FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        }
    }

    #[test]
    fn test_multi_market_creation() {
        let sim = MultiMarketSim::new(
            vec![MarketId::new("BTC/USDT"), MarketId::new("ETH/USDT")],
            test_fee(),
        );
        assert_eq!(sim.market_count(), 2);
    }

    #[test]
    fn test_multi_market_independent() {
        let mut sim = MultiMarketSim::new(
            vec![MarketId::new("BTC/USDT"), MarketId::new("ETH/USDT")],
            test_fee(),
        );
        let acc = AccountId::new();

        // Trade on BTC
        if let Some(btc) = sim.engine_by_symbol_mut("BTC/USDT") {
            btc.submit_order(acc, Side::SELL, Price::from_u64(50000), Decimal::ONE, 100);
            btc.submit_order(acc, Side::BUY, Price::from_u64(50000), Decimal::ONE, 101);
        }

        // ETH should be empty
        let eth = sim.engine_by_symbol("ETH/USDT").unwrap();
        assert_eq!(eth.trade_count(), 0);

        // BTC should have 1 trade
        let btc = sim.engine_by_symbol("BTC/USDT").unwrap();
        assert_eq!(btc.trade_count(), 1);
    }

    #[test]
    fn test_aggregate_metrics() {
        let mut sim = MultiMarketSim::new(
            vec![MarketId::new("BTC/USDT"), MarketId::new("ETH/USDT")],
            test_fee(),
        );
        let acc = AccountId::new();

        for engine in &mut sim.engines {
            engine.submit_order(acc, Side::SELL, Price::from_u64(1000), Decimal::ONE, 100);
            engine.submit_order(acc, Side::BUY, Price::from_u64(1000), Decimal::ONE, 101);
        }

        let metrics = sim.aggregate_metrics();
        assert_eq!(metrics.total_trades, 2); // 1 per market
        assert_eq!(metrics.total_orders, 4); // 2 per market
    }
}
