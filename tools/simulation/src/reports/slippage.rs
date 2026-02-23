//! Slippage report
//!
//! Analyzes per-order slippage: expected vs actual execution price.
//! Provides aggregated statistics (mean, p50, p99).

use crate::engine::SimEvent;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use types::ids::OrderId;
use types::numeric::Price;

/// Slippage record for a single order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageRecord {
    pub order_id: String,
    pub expected_price: String,
    pub actual_avg_price: String,
    pub slippage_bps: String,
    pub quantity: String,
}

/// Aggregated slippage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlippageReport {
    pub records: Vec<SlippageRecord>,
    pub mean_slippage_bps: String,
    pub median_slippage_bps: String,
    pub p99_slippage_bps: String,
    pub max_slippage_bps: String,
    pub total_orders_analyzed: usize,
}

/// Track fills per order for slippage calculation.
struct OrderFills {
    submitted_price: Price,
    fills: Vec<(Price, Decimal)>, // (price, quantity)
}

/// Generate a slippage report from simulation events.
pub fn analyze(events: &[SimEvent]) -> SlippageReport {
    let mut order_map: HashMap<OrderId, OrderFills> = HashMap::new();

    // Collect order submitted prices
    for event in events {
        if let SimEvent::OrderPlaced { order_id, price, .. } = event {
            order_map.insert(*order_id, OrderFills {
                submitted_price: *price,
                fills: Vec::new(),
            });
        }
    }

    // Collect fills (taker side gets slippage)
    for event in events {
        if let SimEvent::TradeExecuted {
            taker_order_id, price, quantity, ..
        } = event {
            if let Some(fills) = order_map.get_mut(taker_order_id) {
                fills.fills.push((*price, *quantity));
            }
        }
    }

    // Calculate slippage per order
    let mut records = Vec::new();
    let mut slippages: Vec<Decimal> = Vec::new();

    for (order_id, fills) in &order_map {
        if fills.fills.is_empty() {
            continue; // No fills, no slippage
        }

        let total_qty: Decimal = fills.fills.iter().map(|(_, q)| *q).sum();
        let total_value: Decimal = fills.fills.iter()
            .map(|(p, q)| p.as_decimal() * *q)
            .sum();

        if total_qty == Decimal::ZERO {
            continue;
        }

        let avg_price = total_value / total_qty;
        let expected = fills.submitted_price.as_decimal();

        let slippage_bps = if expected > Decimal::ZERO {
            ((avg_price - expected).abs() / expected) * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        slippages.push(slippage_bps);

        records.push(SlippageRecord {
            order_id: order_id.to_string(),
            expected_price: expected.to_string(),
            actual_avg_price: avg_price.round_dp(8).to_string(),
            slippage_bps: slippage_bps.round_dp(4).to_string(),
            quantity: total_qty.to_string(),
        });
    }

    // Sort slippages for percentile calculations
    slippages.sort();

    let mean = if slippages.is_empty() {
        Decimal::ZERO
    } else {
        let sum: Decimal = slippages.iter().sum();
        sum / Decimal::from(slippages.len())
    };

    let median = percentile(&slippages, 50);
    let p99 = percentile(&slippages, 99);
    let max = slippages.last().cloned().unwrap_or(Decimal::ZERO);

    SlippageReport {
        total_orders_analyzed: records.len(),
        records,
        mean_slippage_bps: mean.round_dp(4).to_string(),
        median_slippage_bps: median.round_dp(4).to_string(),
        p99_slippage_bps: p99.round_dp(4).to_string(),
        max_slippage_bps: max.round_dp(4).to_string(),
    }
}

/// Compute a percentile from a sorted list.
fn percentile(sorted: &[Decimal], p: usize) -> Decimal {
    if sorted.is_empty() {
        return Decimal::ZERO;
    }
    let idx = (p * sorted.len()) / 100;
    let idx = idx.min(sorted.len() - 1);
    sorted[idx]
}

/// Export slippage report as JSON.
pub fn export_json(events: &[SimEvent]) -> String {
    let report = analyze(events);
    serde_json::to_string_pretty(&report).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SimEngine;
    use types::fee::FeeTier;
    use types::ids::{AccountId, MarketId};
    use types::order::Side;

    fn run_scenario() -> Vec<SimEvent> {
        let fee = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), fee);
        let maker = AccountId::new();
        let taker = AccountId::new();

        // Maker places asks at different prices
        engine.submit_order(maker, Side::SELL, Price::from_u64(50100), Decimal::ONE, 100);
        engine.submit_order(maker, Side::SELL, Price::from_u64(50200), Decimal::ONE, 101);

        // Taker buys 2 — crosses both levels (slippage)
        engine.submit_order(taker, Side::BUY, Price::from_u64(50200), Decimal::from(2), 102);

        engine.events.clone()
    }

    #[test]
    fn test_slippage_analysis() {
        let events = run_scenario();
        let report = analyze(&events);
        assert!(report.total_orders_analyzed > 0);
    }

    #[test]
    fn test_export_json() {
        let events = run_scenario();
        let json = export_json(&events);
        assert!(json.contains("slippage_bps"));
        assert!(json.contains("mean_slippage_bps"));
    }

    #[test]
    fn test_zero_slippage() {
        let fee = FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        };
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), fee);
        let maker = AccountId::new();
        let taker = AccountId::new();

        engine.submit_order(maker, Side::SELL, Price::from_u64(50000), Decimal::ONE, 100);
        engine.submit_order(taker, Side::BUY, Price::from_u64(50000), Decimal::ONE, 101);

        let report = analyze(&engine.events);
        // Single level fill = zero slippage for taker
        for rec in &report.records {
            assert_eq!(rec.slippage_bps, "0.0000");
        }
    }
}
