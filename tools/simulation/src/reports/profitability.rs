//! Profitability report
//!
//! Tracks per-account PnL, fee costs, and net results across simulation.

use crate::engine::SimEvent;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use types::ids::AccountId;

/// Per-account profitability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProfit {
    pub account_id: String,
    pub buy_volume: String,
    pub sell_volume: String,
    pub total_maker_fees: String,
    pub total_taker_fees: String,
    pub net_fee_cost: String,
    pub trade_count: u64,
}

/// Aggregated profitability report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfitabilityReport {
    pub accounts: Vec<AccountProfit>,
    pub total_volume: String,
    pub total_fees_collected: String,
    pub total_maker_rebates: String,
    pub net_exchange_revenue: String,
}

/// Internal accumulator for an account.
#[derive(Debug, Default)]
struct AccountAccum {
    buy_volume: Decimal,
    sell_volume: Decimal,
    maker_fees: Decimal,
    taker_fees: Decimal,
    trade_count: u64,
}

/// Generate a profitability report from simulation events.
pub fn analyze(events: &[SimEvent]) -> ProfitabilityReport {
    let mut accounts: HashMap<AccountId, AccountAccum> = HashMap::new();

    for event in events {
        if let SimEvent::TradeExecuted {
            maker_account_id,
            taker_account_id,
            price,
            quantity,
            maker_fee,
            taker_fee,
            ..
        } = event {
            let trade_value = *quantity * price.as_decimal();

            // Maker side
            let maker = accounts.entry(*maker_account_id).or_default();
            maker.sell_volume += trade_value;
            maker.maker_fees += *maker_fee;
            maker.trade_count += 1;

            // Taker side
            let taker = accounts.entry(*taker_account_id).or_default();
            taker.buy_volume += trade_value;
            taker.taker_fees += *taker_fee;
            taker.trade_count += 1;
        }
    }

    let mut total_volume = Decimal::ZERO;
    let mut total_fees = Decimal::ZERO;
    let mut total_rebates = Decimal::ZERO;

    let mut result_accounts: Vec<AccountProfit> = accounts.iter().map(|(id, acc)| {
        let volume = acc.buy_volume + acc.sell_volume;
        total_volume += volume;

        let net_fee = acc.maker_fees + acc.taker_fees;
        total_fees += acc.taker_fees;

        if acc.maker_fees < Decimal::ZERO {
            total_rebates += acc.maker_fees.abs();
        }

        AccountProfit {
            account_id: id.to_string(),
            buy_volume: acc.buy_volume.to_string(),
            sell_volume: acc.sell_volume.to_string(),
            total_maker_fees: acc.maker_fees.to_string(),
            total_taker_fees: acc.taker_fees.to_string(),
            net_fee_cost: net_fee.to_string(),
            trade_count: acc.trade_count,
        }
    }).collect();

    // Sort by trade count descending for readability
    result_accounts.sort_by(|a, b| b.trade_count.cmp(&a.trade_count));

    let net_revenue = total_fees - total_rebates;

    ProfitabilityReport {
        accounts: result_accounts,
        total_volume: total_volume.to_string(),
        total_fees_collected: total_fees.to_string(),
        total_maker_rebates: total_rebates.to_string(),
        net_exchange_revenue: net_revenue.to_string(),
    }
}

/// Export profitability report as JSON.
pub fn export_json(events: &[SimEvent]) -> String {
    let report = analyze(events);
    serde_json::to_string_pretty(&report).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::SimEngine;
    use types::fee::FeeTier;
    use types::ids::MarketId;
    use types::numeric::Price;
    use types::order::Side;

    #[test]
    fn test_profitability_basic() {
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
        assert_eq!(report.accounts.len(), 2);
        assert!(report.total_volume.parse::<Decimal>().unwrap() > Decimal::ZERO);
    }

    #[test]
    fn test_profitability_export() {
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

        let json = export_json(&engine.events);
        assert!(json.contains("total_volume"));
        assert!(json.contains("net_exchange_revenue"));
    }

    #[test]
    fn test_empty_events() {
        let report = analyze(&[]);
        assert!(report.accounts.is_empty());
        assert_eq!(report.total_volume, "0");
    }
}
