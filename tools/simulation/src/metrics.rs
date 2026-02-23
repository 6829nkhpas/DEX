//! Performance metrics for simulation
//!
//! Tracks orders, trades, cancels, latency histograms, and throughput.

use crate::engine::SimEvent;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Latency histogram bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBucket {
    pub label: String,
    pub lower_ns: u64,
    pub upper_ns: u64,
    pub count: u64,
}

/// Aggregated simulation metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimMetrics {
    pub total_orders: u64,
    pub total_trades: u64,
    pub total_fills: u64,
    pub total_partial_fills: u64,
    pub total_cancels: u64,
    pub total_volume: Decimal,
    pub total_maker_fees: Decimal,
    pub total_taker_fees: Decimal,
    pub max_book_depth: usize,
    pub latency_buckets: Vec<LatencyBucket>,
    pub elapsed_ns: u64,
}

impl SimMetrics {
    /// Create empty metrics with default latency buckets.
    pub fn new() -> Self {
        Self {
            total_orders: 0,
            total_trades: 0,
            total_fills: 0,
            total_partial_fills: 0,
            total_cancels: 0,
            total_volume: Decimal::ZERO,
            total_maker_fees: Decimal::ZERO,
            total_taker_fees: Decimal::ZERO,
            max_book_depth: 0,
            latency_buckets: default_buckets(),
            elapsed_ns: 0,
        }
    }

    /// Record a single event into metrics.
    pub fn record_event(&mut self, event: &SimEvent) {
        match event {
            SimEvent::OrderPlaced { .. } => {
                self.total_orders += 1;
            }
            SimEvent::TradeExecuted {
                quantity,
                price,
                maker_fee,
                taker_fee,
                ..
            } => {
                self.total_trades += 1;
                self.total_volume += *quantity * price.as_decimal();
                self.total_maker_fees += *maker_fee;
                self.total_taker_fees += *taker_fee;
            }
            SimEvent::OrderFilled { .. } => {
                self.total_fills += 1;
            }
            SimEvent::OrderPartiallyFilled { .. } => {
                self.total_partial_fills += 1;
            }
            SimEvent::OrderCanceled { .. } => {
                self.total_cancels += 1;
            }
        }
    }

    /// Record latency in nanoseconds.
    pub fn record_latency(&mut self, latency_ns: u64) {
        for bucket in &mut self.latency_buckets {
            if latency_ns >= bucket.lower_ns && latency_ns < bucket.upper_ns {
                bucket.count += 1;
                return;
            }
        }
        // Overflow bucket (last)
        if let Some(last) = self.latency_buckets.last_mut() {
            last.count += 1;
        }
    }

    /// Update max book depth.
    pub fn update_book_depth(&mut self, depth: usize) {
        if depth > self.max_book_depth {
            self.max_book_depth = depth;
        }
    }

    /// Set elapsed time.
    pub fn set_elapsed(&mut self, ns: u64) {
        self.elapsed_ns = ns;
    }

    /// Throughput: orders per second.
    pub fn orders_per_second(&self) -> f64 {
        if self.elapsed_ns == 0 {
            return 0.0;
        }
        self.total_orders as f64 / (self.elapsed_ns as f64 / 1_000_000_000.0)
    }

    /// Throughput: trades per second.
    pub fn trades_per_second(&self) -> f64 {
        if self.elapsed_ns == 0 {
            return 0.0;
        }
        self.total_trades as f64 / (self.elapsed_ns as f64 / 1_000_000_000.0)
    }

    /// Build a summary string.
    pub fn summary(&self) -> String {
        format!(
            "Orders: {} | Trades: {} | Fills: {} | Cancels: {} | Volume: {} | Throughput: {:.0} orders/s",
            self.total_orders,
            self.total_trades,
            self.total_fills,
            self.total_cancels,
            self.total_volume,
            self.orders_per_second(),
        )
    }

    /// Process all events from an engine.
    pub fn ingest_events(&mut self, events: &[SimEvent]) {
        for event in events {
            self.record_event(event);
        }
    }
}

impl Default for SimMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Default latency histogram buckets.
fn default_buckets() -> Vec<LatencyBucket> {
    vec![
        LatencyBucket { label: "<1μs".into(), lower_ns: 0, upper_ns: 1_000, count: 0 },
        LatencyBucket { label: "1-10μs".into(), lower_ns: 1_000, upper_ns: 10_000, count: 0 },
        LatencyBucket { label: "10-100μs".into(), lower_ns: 10_000, upper_ns: 100_000, count: 0 },
        LatencyBucket { label: "100-500μs".into(), lower_ns: 100_000, upper_ns: 500_000, count: 0 },
        LatencyBucket { label: "500μs-1ms".into(), lower_ns: 500_000, upper_ns: 1_000_000, count: 0 },
        LatencyBucket { label: "1-10ms".into(), lower_ns: 1_000_000, upper_ns: 10_000_000, count: 0 },
        LatencyBucket { label: ">10ms".into(), lower_ns: 10_000_000, upper_ns: u64::MAX, count: 0 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{AccountId, OrderId, TradeId};
    use types::numeric::Price;
    use types::order::Side;

    #[test]
    fn test_metrics_creation() {
        let metrics = SimMetrics::new();
        assert_eq!(metrics.total_orders, 0);
        assert_eq!(metrics.total_trades, 0);
        assert_eq!(metrics.latency_buckets.len(), 7);
    }

    #[test]
    fn test_record_order_placed() {
        let mut metrics = SimMetrics::new();
        let event = SimEvent::OrderPlaced {
            order_id: OrderId::new(),
            account_id: AccountId::new(),
            side: Side::BUY,
            price: Price::from_u64(50000),
            quantity: Decimal::ONE,
            timestamp: 1000,
        };
        metrics.record_event(&event);
        assert_eq!(metrics.total_orders, 1);
    }

    #[test]
    fn test_record_trade() {
        let mut metrics = SimMetrics::new();
        let event = SimEvent::TradeExecuted {
            trade_id: TradeId::new(),
            maker_order_id: OrderId::new(),
            taker_order_id: OrderId::new(),
            maker_account_id: AccountId::new(),
            taker_account_id: AccountId::new(),
            price: Price::from_u64(50000),
            quantity: Decimal::ONE,
            maker_fee: Decimal::from(10),
            taker_fee: Decimal::from(25),
            timestamp: 1000,
        };
        metrics.record_event(&event);
        assert_eq!(metrics.total_trades, 1);
        assert_eq!(metrics.total_volume, Decimal::from(50000));
        assert_eq!(metrics.total_maker_fees, Decimal::from(10));
        assert_eq!(metrics.total_taker_fees, Decimal::from(25));
    }

    #[test]
    fn test_latency_buckets() {
        let mut metrics = SimMetrics::new();
        metrics.record_latency(500);    // <1μs
        metrics.record_latency(5_000);  // 1-10μs
        metrics.record_latency(50_000); // 10-100μs

        assert_eq!(metrics.latency_buckets[0].count, 1);
        assert_eq!(metrics.latency_buckets[1].count, 1);
        assert_eq!(metrics.latency_buckets[2].count, 1);
    }

    #[test]
    fn test_throughput() {
        let mut metrics = SimMetrics::new();
        metrics.total_orders = 100_000;
        metrics.elapsed_ns = 1_000_000_000; // 1 second
        assert_eq!(metrics.orders_per_second(), 100_000.0);
    }

    #[test]
    fn test_summary() {
        let metrics = SimMetrics::new();
        let summary = metrics.summary();
        assert!(summary.contains("Orders: 0"));
    }
}
