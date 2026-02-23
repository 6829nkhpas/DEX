//! Event log and deterministic replay validation
//!
//! Per spec §11 (Replay Requirements) and §12 (Determinism Rules):
//! same events → same final state.

use crate::engine::{SimEngine, SimEvent};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::fee::FeeTier;
use types::ids::MarketId;

/// A snapshot of engine state for comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub bid_depth: String,
    pub ask_depth: String,
    pub order_count: usize,
    pub trade_count: usize,
    pub sequence: u64,
}

/// Capture a snapshot of the engine state.
pub fn capture_snapshot(engine: &SimEngine) -> EngineSnapshot {
    EngineSnapshot {
        bid_depth: engine.bid_depth().to_string(),
        ask_depth: engine.ask_depth().to_string(),
        order_count: engine.order_count(),
        trade_count: engine.trade_count(),
        sequence: engine.sequence,
    }
}

/// Replay events into a fresh engine and return the resulting snapshot.
///
/// This replays OrderPlaced events by resubmitting orders and
/// verifies that the final state matches the original.
pub fn replay_and_snapshot(
    symbol: MarketId,
    fee_tier: FeeTier,
    events: &[SimEvent],
) -> EngineSnapshot {
    let mut engine = SimEngine::new(symbol, fee_tier);

    for event in events {
        if let SimEvent::OrderPlaced {
            account_id, side, price, quantity, timestamp, ..
        } = event {
            engine.submit_order(*account_id, *side, *price, *quantity, *timestamp);
        }
    }

    capture_snapshot(&engine)
}

/// Validate replay determinism: run events through a fresh engine
/// and compare snapshots.
pub fn validate_replay(
    symbol: MarketId,
    fee_tier: FeeTier,
    events: &[SimEvent],
    expected: &EngineSnapshot,
) -> ReplayValidation {
    let replayed = replay_and_snapshot(symbol, fee_tier, events);

    let matches = replayed == *expected;

    ReplayValidation {
        matches,
        original: expected.clone(),
        replayed,
    }
}

/// Result of replay validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayValidation {
    pub matches: bool,
    pub original: EngineSnapshot,
    pub replayed: EngineSnapshot,
}

/// Export event log as JSON.
pub fn export_event_log(events: &[SimEvent]) -> String {
    serde_json::to_string_pretty(events).unwrap_or_default()
}

/// Import event log from JSON.
pub fn import_event_log(json: &str) -> Result<Vec<SimEvent>, serde_json::Error> {
    serde_json::from_str(json)
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
    fn test_snapshot_capture() {
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), test_fee());
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::ONE, 100);

        let snap = capture_snapshot(&engine);
        assert_eq!(snap.order_count, 1);
        assert_eq!(snap.bid_depth, "1");
    }

    #[test]
    fn test_replay_determinism() {
        let fee = test_fee();
        let symbol = MarketId::new("BTC/USDT");
        let acc1 = AccountId::new();
        let acc2 = AccountId::new();

        // Run original simulation
        let mut engine = SimEngine::new(symbol.clone(), fee.clone());
        engine.submit_order(acc1, Side::SELL, Price::from_u64(50000), Decimal::from(2), 100);
        engine.submit_order(acc1, Side::SELL, Price::from_u64(50100), Decimal::from(3), 101);
        engine.submit_order(acc2, Side::BUY, Price::from_u64(50100), Decimal::from(4), 102);
        engine.submit_order(acc2, Side::BUY, Price::from_u64(49900), Decimal::from(1), 103);

        let original_snap = capture_snapshot(&engine);
        let events = engine.events.clone();

        // Replay
        let validation = validate_replay(symbol, fee, &events, &original_snap);
        assert!(validation.matches, "Replay produced different state");
    }

    #[test]
    fn test_event_log_roundtrip() {
        let mut engine = SimEngine::new(MarketId::new("BTC/USDT"), test_fee());
        let acc = AccountId::new();
        engine.submit_order(acc, Side::BUY, Price::from_u64(49900), Decimal::ONE, 100);

        let json = export_event_log(&engine.events);
        let imported = import_event_log(&json).unwrap();
        assert_eq!(engine.events.len(), imported.len());
    }

    #[test]
    fn test_empty_replay() {
        let fee = test_fee();
        let snap = replay_and_snapshot(MarketId::new("BTC/USDT"), fee, &[]);
        assert_eq!(snap.order_count, 0);
        assert_eq!(snap.trade_count, 0);
    }
}
