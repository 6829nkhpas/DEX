//! Stress test: 100,000 orders
//!
//! Generates 100k orders via retail + market maker bots,
//! runs through engine, asserts all processed, measures throughput.

use simulation::bots::market_maker::{MarketMaker, MarketMakerConfig};
use simulation::bots::retail_trader::{RetailTrader, RetailTraderConfig};
use simulation::engine::SimEngine;
use simulation::metrics::SimMetrics;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::time::Instant;
use types::fee::FeeTier;
use types::ids::{AccountId, MarketId};
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
#[ignore] // Run with: cargo test --test stress_100k -- --ignored
fn test_100k_orders() {
    let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), test_fee());
    let mm_account = AccountId::new();

    // Seed the book with initial liquidity
    let seeder = AccountId::new();
    for i in 0..50 {
        let bid = Price::from_u64(49900 - i * 10);
        let ask = Price::from_u64(50100 + i * 10);
        engine.submit_order(seeder, Side::BUY, bid, Decimal::from(10), i as i64);
        engine.submit_order(seeder, Side::SELL, ask, Decimal::from(10), i as i64 + 1);
    }

    let config_mm = MarketMakerConfig {
        spread_bps: 20,
        order_size: Decimal::from_str_exact("0.5").unwrap(),
        max_inventory: Decimal::from(1000),
        max_daily_loss: Decimal::from(100_000),
        max_open_orders: 200_000,
    };
    let mut mm = MarketMaker::new(mm_account, config_mm, 42);

    let config_rt = RetailTraderConfig {
        min_size: Decimal::from_str_exact("0.01").unwrap(),
        max_size: Decimal::from_str_exact("0.5").unwrap(),
        market_order_ratio: 0.4,
        max_limit_distance_bps: 100,
    };

    let mut traders: Vec<RetailTrader> = (0..10)
        .map(|i| {
            RetailTrader::new(AccountId::new(), config_rt.clone(), 100 + i)
        })
        .collect();

    let start = Instant::now();
    let target_orders = 100_000;
    let mut order_count: u64 = 100; // Already seeded 100

    let mut tick: i64 = 10_000;

    while order_count < target_orders {
        // Market maker places 2 orders per tick
        let placed = mm.tick(&mut engine, tick);
        order_count += placed as u64;
        tick += 1;

        // Each retail trader places 1 order
        for trader in &mut traders {
            if order_count >= target_orders {
                break;
            }
            if trader.tick(&mut engine, tick) {
                order_count += 1;
            }
            tick += 1;
        }
    }

    let elapsed = start.elapsed();
    let elapsed_ns = elapsed.as_nanos() as u64;

    // Collect metrics
    let mut metrics = SimMetrics::new();
    metrics.ingest_events(&engine.events);
    metrics.set_elapsed(elapsed_ns);

    println!("=== STRESS TEST 100K RESULTS ===");
    println!("Total orders: {}", metrics.total_orders);
    println!("Total trades: {}", metrics.total_trades);
    println!("Total fills: {}", metrics.total_fills);
    println!("Total volume: {}", metrics.total_volume);
    println!("Elapsed: {:.2?}", elapsed);
    println!("Throughput: {:.0} orders/sec", metrics.orders_per_second());
    println!("================================");

    // Assertions
    assert!(
        metrics.total_orders >= target_orders,
        "Expected at least {} orders, got {}",
        target_orders, metrics.total_orders
    );
    assert!(metrics.total_trades > 0, "Expected some trades");
    assert!(metrics.total_volume > Decimal::ZERO, "Expected non-zero volume");
}

#[test]
fn test_10k_orders_quick() {
    let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), test_fee());
    let acc = AccountId::new();

    // Seed book
    engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::from(10000), 0);
    engine.submit_order(acc, Side::SELL, Price::from_u64(50100), Decimal::from(10000), 1);

    let config = RetailTraderConfig::default();
    let mut trader = RetailTrader::new(AccountId::new(), config, 42);

    let start = Instant::now();
    for i in 0..10_000 {
        trader.tick(&mut engine, 100 + i);
    }
    let elapsed = start.elapsed();

    let mut metrics = SimMetrics::new();
    metrics.ingest_events(&engine.events);

    assert!(metrics.total_orders > 0);
    println!("10k orders in {:.2?} ({:.0} orders/sec)",
        elapsed,
        metrics.total_orders as f64 / elapsed.as_secs_f64()
    );
}
