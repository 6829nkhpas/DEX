//! Concurrency test
//!
//! Verifies that independent engines per market can run in parallel
//! without data races (each engine is independent, no shared state).

use simulation::engine::SimEngine;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::thread;
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
fn test_concurrent_markets() {
    let symbols = vec!["BTC/USDT", "ETH/USDT", "SOL/USDT", "DOGE/USDT"];
    let fee = test_fee();

    let handles: Vec<_> = symbols.into_iter().map(|sym| {
        let fee = fee.clone();
        let sym = sym.to_string();
        thread::spawn(move || {
            let mut engine = SimEngine::new(MarketId::new(&sym), fee);
            let acc1 = AccountId::new();
            let acc2 = AccountId::new();

            // Run 1000 orders per market
            for i in 0..500 {
                let ts = i as i64;
                engine.submit_order(
                    acc1, Side::SELL, Price::from_u64(50000), Decimal::ONE, ts,
                );
                engine.submit_order(
                    acc2, Side::BUY, Price::from_u64(50000), Decimal::ONE, ts + 1,
                );
            }

            // All should match
            assert_eq!(engine.trade_count(), 500);
            assert_eq!(engine.order_count(), 0);
            engine.trade_count()
        })
    }).collect();

    let mut total_trades = 0;
    for handle in handles {
        total_trades += handle.join().unwrap();
    }

    assert_eq!(total_trades, 2000); // 500 trades × 4 markets
}

#[test]
fn test_concurrent_determinism() {
    let fee = test_fee();
    let acc1 = AccountId::new();
    let acc2 = AccountId::new();

    // Run same simulation twice in parallel
    let fee1 = fee.clone();
    let fee2 = fee.clone();

    let h1 = thread::spawn(move || {
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), fee1);
        for i in 0..100 {
            engine.submit_order(acc1, Side::SELL, Price::from_u64(50000), Decimal::ONE, i);
            engine.submit_order(acc2, Side::BUY, Price::from_u64(50000), Decimal::ONE, i + 1);
        }
        (engine.trade_count(), engine.order_count(), engine.bid_depth(), engine.ask_depth())
    });

    let h2 = thread::spawn(move || {
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), fee2);
        for i in 0..100 {
            engine.submit_order(acc1, Side::SELL, Price::from_u64(50000), Decimal::ONE, i);
            engine.submit_order(acc2, Side::BUY, Price::from_u64(50000), Decimal::ONE, i + 1);
        }
        (engine.trade_count(), engine.order_count(), engine.bid_depth(), engine.ask_depth())
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    assert_eq!(r1, r2, "Parallel runs must produce identical results");
}
