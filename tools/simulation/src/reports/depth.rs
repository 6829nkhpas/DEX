//! Depth visualization export
//!
//! Exports order book depth snapshots as JSON for external visualization.

use crate::engine::SimEngine;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A single depth level for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthLevel {
    pub price: String,
    pub quantity: String,
    pub cumulative_quantity: String,
}

/// Complete depth snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthSnapshot {
    pub symbol: String,
    pub timestamp: i64,
    pub bids: Vec<DepthLevel>,
    pub asks: Vec<DepthLevel>,
    pub total_bid_depth: String,
    pub total_ask_depth: String,
    pub spread: Option<String>,
}

/// Generate a depth snapshot from the engine.
pub fn snapshot(engine: &SimEngine, timestamp: i64) -> DepthSnapshot {
    let bid_levels = engine.bid_levels();
    let ask_levels = engine.ask_levels();

    let mut cumulative = Decimal::ZERO;
    let bids: Vec<DepthLevel> = bid_levels.iter().map(|(price, qty)| {
        cumulative += *qty;
        DepthLevel {
            price: price.to_string(),
            quantity: qty.to_string(),
            cumulative_quantity: cumulative.to_string(),
        }
    }).collect();

    cumulative = Decimal::ZERO;
    let asks: Vec<DepthLevel> = ask_levels.iter().map(|(price, qty)| {
        cumulative += *qty;
        DepthLevel {
            price: price.to_string(),
            quantity: qty.to_string(),
            cumulative_quantity: cumulative.to_string(),
        }
    }).collect();

    let spread = match (engine.best_bid(), engine.best_ask()) {
        (Some(bid), Some(ask)) => {
            Some((ask.as_decimal() - bid.as_decimal()).to_string())
        }
        _ => None,
    };

    DepthSnapshot {
        symbol: engine.symbol.as_str().to_string(),
        timestamp,
        bids,
        asks,
        total_bid_depth: engine.bid_depth().to_string(),
        total_ask_depth: engine.ask_depth().to_string(),
        spread,
    }
}

/// Export depth snapshot as JSON string.
pub fn export_json(engine: &SimEngine, timestamp: i64) -> String {
    let snap = snapshot(engine, timestamp);
    serde_json::to_string_pretty(&snap).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SimEngine;
    use rust_decimal::Decimal;
    use types::fee::FeeTier;
    use types::ids::{AccountId, MarketId};
    use types::numeric::Price;
    use types::order::Side;

    fn test_engine() -> SimEngine {
        let fee = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };
        SimEngine::new(MarketId::new("BTC/USDT"), fee)
    }

    #[test]
    fn test_depth_snapshot() {
        let mut engine = test_engine();
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(2), 100);
        engine.submit_order(acc, Side::BUY, Price::from_u64(49800), Decimal::from(3), 101);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::from(1), 102);

        let snap = snapshot(&engine, 200);
        assert_eq!(snap.symbol, "BTC/USDT");
        assert_eq!(snap.bids.len(), 2);
        assert_eq!(snap.asks.len(), 1);
        assert!(snap.spread.is_some());
    }

    #[test]
    fn test_export_json() {
        let mut engine = test_engine();
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::ONE, 100);
        engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::ONE, 101);

        let json = export_json(&engine, 200);
        assert!(json.contains("BTC/USDT"));
        assert!(json.contains("bids"));
        assert!(json.contains("asks"));
    }

    #[test]
    fn test_cumulative_quantity() {
        let mut engine = test_engine();
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(2), 100);
        engine.submit_order(acc, Side::BUY, Price::from_u64(49800), Decimal::from(3), 101);

        let snap = snapshot(&engine, 200);
        // First bid: qty=2, cum=2. Second bid: qty=3, cum=5.
        assert_eq!(snap.bids.len(), 2);
        assert_eq!(snap.total_bid_depth, "5");
    }
}
