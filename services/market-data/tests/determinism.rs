//! Determinism tests for the Market Data Service
//!
//! Validates that the system produces identical outputs given identical
//! inputs, as required by spec §12 (Determinism Rules).
//!
//! Tests include:
//! - Dual replay comparison
//! - Random event reordering detection
//! - Missing event simulation
//! - High volatility scenario

use market_data::candles::{CandleBuilder, Timeframe};
use market_data::delta::DeltaGenerator;
use market_data::events::{MarketEvent, MarketEventPayload};
use market_data::order_book::OrderBookState;
use market_data::replay::ReplayEngine;
use market_data::snapshot::SnapshotBuilder;
use market_data::trades::TradeBuffer;
use types::ids::{AccountId, MarketId, OrderId, TradeId};
use types::numeric::{Price, Quantity};
use types::order::Side;
use uuid::Uuid;

use rust_decimal::Decimal;

fn make_event(seq: u64, payload: MarketEventPayload) -> MarketEvent {
    MarketEvent {
        event_id: Uuid::now_v7(),
        sequence: seq,
        timestamp: 1708123456789000000 + (seq as i64 * 1_000_000),
        source: "matching-engine".to_string(),
        payload,
        schema_version: "1.0.0".to_string(),
        correlation_id: Uuid::now_v7(),
    }
}

fn order_accepted(
    seq: u64,
    order_id: OrderId,
    side: Side,
    price: u64,
    qty: &str,
) -> MarketEvent {
    make_event(
        seq,
        MarketEventPayload::OrderAccepted {
            order_id,
            account_id: AccountId::new(),
            symbol: MarketId::new("BTC/USDT"),
            side,
            price: Price::from_u64(price),
            quantity: Quantity::from_str(qty).unwrap(),
        },
    )
}

fn trade_executed(
    seq: u64,
    maker_id: OrderId,
    price: u64,
    qty: &str,
) -> MarketEvent {
    make_event(
        seq,
        MarketEventPayload::TradeExecuted {
            trade_id: TradeId::new(),
            symbol: MarketId::new("BTC/USDT"),
            maker_order_id: maker_id,
            taker_order_id: OrderId::new(),
            maker_account_id: AccountId::new(),
            taker_account_id: AccountId::new(),
            price: Price::from_u64(price),
            quantity: Quantity::from_str(qty).unwrap(),
            side: Side::BUY,
            executed_at: 1708123456789000000 + (seq as i64 * 1_000_000),
        },
    )
}

fn order_canceled(
    seq: u64,
    order_id: OrderId,
    side: Side,
    price: u64,
    remaining: &str,
) -> MarketEvent {
    make_event(
        seq,
        MarketEventPayload::OrderCanceled {
            order_id,
            symbol: MarketId::new("BTC/USDT"),
            side,
            price: Price::from_u64(price),
            remaining_quantity: Quantity::from_str(remaining).unwrap(),
            canceled_by: market_data::events::CancelSource::User,
            reason: "user_cancel".to_string(),
        },
    )
}

/// Build a realistic event sequence for testing.
fn build_scenario() -> (Vec<MarketEvent>, Vec<OrderId>) {
    let mut events = Vec::new();
    let mut order_ids = Vec::new();

    // Phase 1: Build up the book with multiple levels
    for i in 0..5 {
        let bid_id = OrderId::new();
        let ask_id = OrderId::new();
        order_ids.push(bid_id);
        order_ids.push(ask_id);

        events.push(order_accepted(
            (i * 2 + 1) as u64,
            bid_id,
            Side::BUY,
            50000 - i * 100,
            "2.0",
        ));
        events.push(order_accepted(
            (i * 2 + 2) as u64,
            ask_id,
            Side::SELL,
            51000 + i * 100,
            "2.0",
        ));
    }

    // Phase 2: Execute some trades
    let maker_id = order_ids[1]; // First ask order
    events.push(trade_executed(11, maker_id, 51000, "0.5"));
    events.push(trade_executed(12, maker_id, 51000, "0.3"));

    // Phase 3: Cancel an order
    let cancel_id = order_ids[0]; // First bid order
    events.push(order_canceled(13, cancel_id, Side::BUY, 50000, "2.0"));

    // Phase 4: More orders
    let new_bid = OrderId::new();
    let new_ask = OrderId::new();
    events.push(order_accepted(14, new_bid, Side::BUY, 50100, "3.0"));
    events.push(order_accepted(15, new_ask, Side::SELL, 50900, "1.5"));

    (events, order_ids)
}

/// Test 1: Two identical replays produce identical state checksums.
#[test]
fn test_deterministic_replay_produces_identical_state() {
    let (events, _) = build_scenario();

    let engine = ReplayEngine::new();

    let result1 = engine.replay(&events).unwrap();
    let result2 = engine.replay(&events).unwrap();

    assert_eq!(
        result1.metrics.state_checksum,
        result2.metrics.state_checksum,
        "Two replays of the same events must produce identical checksums"
    );

    // Also compare book state explicitly
    let book1 = &result1.books["BTC/USDT"];
    let book2 = &result2.books["BTC/USDT"];

    assert_eq!(book1.bid_depth(), book2.bid_depth());
    assert_eq!(book1.ask_depth(), book2.ask_depth());
    assert_eq!(book1.best_bid(), book2.best_bid());
    assert_eq!(book1.best_ask(), book2.best_ask());
}

/// Test 2: Delta streams from identical event sequences are identical.
#[test]
fn test_identical_delta_streams() {
    let (events, _) = build_scenario();

    fn process_events_to_deltas(events: &[MarketEvent]) -> Vec<String> {
        let mut book = OrderBookState::new(MarketId::new("BTC/USDT"));
        let mut gen = DeltaGenerator::new();
        let mut all_deltas = Vec::new();

        for event in events {
            gen.capture_before(&book);

            match &event.payload {
                MarketEventPayload::OrderAccepted {
                    order_id, side, price, quantity, ..
                } => {
                    book.apply_order_accepted(
                        *order_id, *side, *price, *quantity, event.sequence,
                    );
                }
                MarketEventPayload::TradeExecuted {
                    maker_order_id, quantity, ..
                } => {
                    book.apply_trade_executed(
                        *maker_order_id, *quantity, event.sequence,
                    );
                }
                MarketEventPayload::OrderCanceled {
                    order_id, remaining_quantity, ..
                } => {
                    book.apply_cancel(
                        *order_id, *remaining_quantity, event.sequence,
                    );
                }
                _ => {}
            }

            let deltas = gen.generate_after(&book, event.sequence, event.timestamp);
            for d in &deltas {
                all_deltas.push(serde_json::to_string(d).unwrap());
            }
        }

        all_deltas
    }

    let deltas1 = process_events_to_deltas(&events);
    let deltas2 = process_events_to_deltas(&events);

    assert_eq!(
        deltas1.len(),
        deltas2.len(),
        "Same events must produce same number of deltas"
    );

    for (i, (d1, d2)) in deltas1.iter().zip(deltas2.iter()).enumerate() {
        assert_eq!(d1, d2, "Delta {} differs between runs", i);
    }
}

/// Test 3: Candles from identical trade sequences are identical.
#[test]
fn test_identical_candles() {
    fn build_candles(events: &[MarketEvent]) -> Vec<String> {
        let mut builder = CandleBuilder::new(
            Timeframe::M1,
            MarketId::new("BTC/USDT"),
            1000,
        );
        let mut closed = Vec::new();

        for event in events {
            if let MarketEventPayload::TradeExecuted { price, quantity, .. } =
                &event.payload
            {
                if let Some(candle) = builder.process_trade(
                    *price,
                    quantity.as_decimal(),
                    event.timestamp,
                ) {
                    closed.push(serde_json::to_string(&candle).unwrap());
                }
            }
        }

        // Close final candle
        if let Some(candle) = builder.close_current() {
            closed.push(serde_json::to_string(&candle).unwrap());
        }

        closed
    }

    let (events, _) = build_scenario();

    let candles1 = build_candles(&events);
    let candles2 = build_candles(&events);

    assert_eq!(candles1.len(), candles2.len());
    for (i, (c1, c2)) in candles1.iter().zip(candles2.iter()).enumerate() {
        assert_eq!(c1, c2, "Candle {} differs between runs", i);
    }
}

/// Test 4: No state divergence after full event processing.
#[test]
fn test_no_state_divergence() {
    let (events, _) = build_scenario();

    // Run 1: using replay engine
    let engine = ReplayEngine::new();
    let replay_result = engine.replay(&events).unwrap();
    let replay_book = &replay_result.books["BTC/USDT"];

    // Run 2: manual event-by-event processing
    let mut manual_book = OrderBookState::new(MarketId::new("BTC/USDT"));
    for event in &events {
        match &event.payload {
            MarketEventPayload::OrderAccepted {
                order_id, side, price, quantity, ..
            } => {
                manual_book.apply_order_accepted(
                    *order_id, *side, *price, *quantity, event.sequence,
                );
            }
            MarketEventPayload::TradeExecuted {
                maker_order_id, quantity, ..
            } => {
                manual_book.apply_trade_executed(
                    *maker_order_id, *quantity, event.sequence,
                );
            }
            MarketEventPayload::OrderCanceled {
                order_id, remaining_quantity, ..
            } => {
                manual_book.apply_cancel(
                    *order_id, *remaining_quantity, event.sequence,
                );
            }
            _ => {}
        }
    }

    // Compare state
    assert_eq!(replay_book.bid_depth(), manual_book.bid_depth());
    assert_eq!(replay_book.ask_depth(), manual_book.ask_depth());
    assert_eq!(replay_book.best_bid(), manual_book.best_bid());
    assert_eq!(replay_book.best_ask(), manual_book.best_ask());

    // Compare snapshots
    let mut snap_builder = SnapshotBuilder::new();
    let replay_snap = snap_builder.build_full(replay_book, 0);
    let manual_snap = snap_builder.build_full(&manual_book, 0);

    assert_eq!(
        replay_snap.checksum, manual_snap.checksum,
        "Replay engine and manual processing must produce identical state"
    );
}

/// Test 5: No phantom deltas — unchanged levels produce no deltas.
#[test]
fn test_no_phantom_deltas_comprehensive() {
    let mut book = OrderBookState::new(MarketId::new("BTC/USDT"));
    let mut gen = DeltaGenerator::new();

    // Add some orders
    book.apply_order_accepted(
        OrderId::new(),
        Side::BUY,
        Price::from_u64(50000),
        Quantity::from_str("1.0").unwrap(),
        1,
    );
    book.apply_order_accepted(
        OrderId::new(),
        Side::SELL,
        Price::from_u64(51000),
        Quantity::from_str("1.0").unwrap(),
        2,
    );

    // Capture current state
    gen.capture_before(&book);

    // Do nothing — no events

    // Generate deltas
    let deltas = gen.generate_after(&book, 3, 1708123456789000000);
    assert!(
        deltas.is_empty(),
        "No phantom deltas should be generated when book is unchanged"
    );
}

/// Test 6: Conservation after delta — deltas accurately reflect book change.
#[test]
fn test_conservation_after_delta() {
    let mut book = OrderBookState::new(MarketId::new("BTC/USDT"));
    let mut gen = DeltaGenerator::new();

    let maker_id = OrderId::new();

    book.apply_order_accepted(
        maker_id,
        Side::SELL,
        Price::from_u64(51000),
        Quantity::from_str("5.0").unwrap(),
        1,
    );

    gen.capture_before(&book);
    book.apply_trade_executed(maker_id, Quantity::from_str("2.0").unwrap(), 2);
    let deltas = gen.generate_after(&book, 2, 1708123456789000000);

    assert_eq!(deltas.len(), 1);
    let delta = &deltas[0];
    assert_eq!(delta.old_quantity, Decimal::from(5));
    assert_eq!(delta.new_quantity, Decimal::from(3));
    assert_eq!(
        delta.quantity_change(),
        Decimal::from(-2),
        "Delta must exactly reflect the quantity removed by trade"
    );
}

/// Test 7: High volatility scenario — rapid price swings.
#[test]
fn test_high_volatility_scenario() {
    let mut book = OrderBookState::new(MarketId::new("BTC/USDT"));
    let mut trade_buffer = TradeBuffer::new(MarketId::new("BTC/USDT"), 10000);
    let mut gen = DeltaGenerator::new();

    // Simulate rapid order placement and fills across wide price range
    let mut seq = 1u64;
    let mut total_deltas = 0;

    for wave in 0..10 {
        let base_price = 50000 + (wave % 3) * 500; // Price oscillates

        // Add 5 orders per wave
        for i in 0..5 {
            let order_id = OrderId::new();
            let price = base_price + i * 10;

            gen.capture_before(&book);
            book.apply_order_accepted(
                order_id,
                if wave % 2 == 0 { Side::BUY } else { Side::SELL },
                Price::from_u64(price as u64),
                Quantity::from_str("0.5").unwrap(),
                seq,
            );
            let deltas = gen.generate_after(&book, seq, 1708123456789000000 + seq as i64);
            total_deltas += deltas.len();

            // Immediately fill some orders
            if i % 2 == 0 {
                seq += 1;
                gen.capture_before(&book);
                book.apply_trade_executed(
                    order_id,
                    Quantity::from_str("0.3").unwrap(),
                    seq,
                );
                let deltas = gen.generate_after(&book, seq, 1708123456789000000 + seq as i64);
                total_deltas += deltas.len();

                trade_buffer.record_trade(
                    TradeId::new(),
                    Price::from_u64(price as u64),
                    Quantity::from_str("0.3").unwrap(),
                    Side::BUY,
                    1708123456789000000 + seq as i64,
                );
            }

            seq += 1;
        }
    }

    // Verify no corruption
    assert!(book.bid_depth() > 0 || book.ask_depth() > 0);
    assert!(total_deltas > 0, "Deltas must be generated during volatility");
    assert!(
        trade_buffer.history_len() > 0,
        "Trades must be recorded during volatility"
    );

    // Verify trade sequence is monotonic
    let trades = trade_buffer.replay_history();
    for window in trades.windows(2) {
        assert!(
            window[1].trade_sequence > window[0].trade_sequence,
            "Trade sequences must be monotonically increasing"
        );
    }
}

/// Test 8: Missing event simulation — replay engine detects gaps.
#[test]
fn test_missing_event_simulation() {
    let id1 = OrderId::new();
    let id2 = OrderId::new();

    let events = vec![
        order_accepted(1, id1, Side::BUY, 50000, "1.0"),
        // sequence 2 is missing
        order_accepted(3, id2, Side::SELL, 51000, "1.0"),
    ];

    let engine = ReplayEngine::new();
    let result = engine.replay(&events);
    assert!(
        result.is_err(),
        "Replay must fail when events are missing"
    );
}
