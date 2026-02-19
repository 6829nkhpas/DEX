//! Risk event definitions
//!
//! Events emitted by the risk engine for monitoring and alerting
//! per specs §8 (Event Taxonomy) and §6.9 (Liquidation States).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::AccountId;
use uuid::Uuid;

use crate::liquidation::HealthLevel;

/// Risk event emitted by the risk engine
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskEvent {
    pub event_id: Uuid,
    pub account_id: AccountId,
    pub event_type: RiskEventType,
    pub margin_ratio: Decimal,
    pub equity: Decimal,
    pub maintenance_margin: Decimal,
    pub timestamp: i64,
}

/// Risk event type classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskEventType {
    /// Margin ratio dropped below 2.0 — warning notification
    MarginWarning,
    /// Margin ratio dropped below 1.2 — block new orders
    MarginCall,
    /// Margin ratio dropped below 1.1 — initiate liquidation
    LiquidationTriggered,
    /// Pre-trade risk check rejected an order
    RiskCheckFailed { reason: String },
}

impl RiskEvent {
    /// Create a risk event from current account state
    pub fn new(
        account_id: AccountId,
        event_type: RiskEventType,
        margin_ratio: Decimal,
        equity: Decimal,
        maintenance_margin: Decimal,
        timestamp: i64,
    ) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            account_id,
            event_type,
            margin_ratio,
            equity,
            maintenance_margin,
            timestamp,
        }
    }
}

/// Generate risk events based on health level transition.
///
/// Returns events that should be emitted based on current margin ratio.
pub fn events_for_health(
    account_id: AccountId,
    health: HealthLevel,
    margin_ratio: Decimal,
    equity: Decimal,
    maintenance_margin: Decimal,
    timestamp: i64,
) -> Vec<RiskEvent> {
    let mut events = Vec::new();

    match health {
        HealthLevel::Healthy => {
            // No events needed
        }
        HealthLevel::Warning => {
            events.push(RiskEvent::new(
                account_id,
                RiskEventType::MarginWarning,
                margin_ratio,
                equity,
                maintenance_margin,
                timestamp,
            ));
        }
        HealthLevel::Danger => {
            events.push(RiskEvent::new(
                account_id,
                RiskEventType::MarginCall,
                margin_ratio,
                equity,
                maintenance_margin,
                timestamp,
            ));
        }
        HealthLevel::Liquidation => {
            events.push(RiskEvent::new(
                account_id,
                RiskEventType::LiquidationTriggered,
                margin_ratio,
                equity,
                maintenance_margin,
                timestamp,
            ));
        }
    }

    events
}

/// Create a risk check failed event.
pub fn risk_check_failed_event(
    account_id: AccountId,
    reason: String,
    timestamp: i64,
) -> RiskEvent {
    RiskEvent::new(
        account_id,
        RiskEventType::RiskCheckFailed { reason },
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        timestamp,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::AccountId;

    #[test]
    fn test_no_events_for_healthy() {
        let events = events_for_health(
            AccountId::new(),
            HealthLevel::Healthy,
            Decimal::from(3),
            Decimal::from(10_000),
            Decimal::from(3_000),
            1708123456789000000,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn test_warning_event() {
        let events = events_for_health(
            AccountId::new(),
            HealthLevel::Warning,
            Decimal::from_str_exact("1.8").unwrap(),
            Decimal::from(5_400),
            Decimal::from(3_000),
            1708123456789000000,
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, RiskEventType::MarginWarning);
    }

    #[test]
    fn test_margin_call_event() {
        let events = events_for_health(
            AccountId::new(),
            HealthLevel::Danger,
            Decimal::from_str_exact("1.15").unwrap(),
            Decimal::from(3_450),
            Decimal::from(3_000),
            1708123456789000000,
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, RiskEventType::MarginCall);
    }

    #[test]
    fn test_liquidation_event() {
        let events = events_for_health(
            AccountId::new(),
            HealthLevel::Liquidation,
            Decimal::from_str_exact("1.05").unwrap(),
            Decimal::from(3_150),
            Decimal::from(3_000),
            1708123456789000000,
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, RiskEventType::LiquidationTriggered);
    }

    #[test]
    fn test_risk_check_failed_event() {
        let event = risk_check_failed_event(
            AccountId::new(),
            "Insufficient margin".to_string(),
            1708123456789000000,
        );
        assert!(matches!(
            event.event_type,
            RiskEventType::RiskCheckFailed { .. }
        ));
    }

    #[test]
    fn test_event_has_unique_id() {
        let e1 = risk_check_failed_event(
            AccountId::new(), "a".into(), 1708123456789000000,
        );
        let e2 = risk_check_failed_event(
            AccountId::new(), "b".into(), 1708123456789000000,
        );
        assert_ne!(e1.event_id, e2.event_id);
    }
}
