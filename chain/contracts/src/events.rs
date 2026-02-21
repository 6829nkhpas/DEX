//! Contract events matching spec §08 (Event Taxonomy)
//!
//! Events are immutable records emitted by contract operations.
//! All event types here directly correspond to the frozen event taxonomy.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::ids::AccountId;
use uuid::Uuid;

/// Deposit detected on-chain (awaiting confirmations)
///
/// Spec §08 §3.7: DepositDetected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositDetected {
    pub account_id: AccountId,
    pub asset: String,
    pub amount: Decimal,
    pub tx_id: String,
    pub confirmations: u64,
}

/// Deposit confirmed after required confirmations
///
/// Spec §08 §3.7: DepositConfirmed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositConfirmed {
    pub account_id: AccountId,
    pub asset: String,
    pub amount: Decimal,
    pub tx_id: String,
    pub confirmations: u64,
}

/// Withdrawal requested by user
///
/// Spec §08 §3.7: WithdrawalRequested
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithdrawalRequested {
    pub withdrawal_id: Uuid,
    pub account_id: AccountId,
    pub asset: String,
    pub amount: Decimal,
    pub destination: String,
}

/// Withdrawal completed and broadcast on-chain
///
/// Spec §08 §3.7: WithdrawalCompleted
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithdrawalCompleted {
    pub withdrawal_id: Uuid,
    pub tx_id: String,
    pub fee: Decimal,
}

/// State commitment root submitted
///
/// Emitted when a new state root is committed by an authorized submitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitmentSubmitted {
    pub root_hash: [u8; 32],
    pub block_number: u64,
    pub submitter: String,
    pub submitted_at: i64,
}

/// Dispute raised against a committed state root
///
/// Emitted when a challenger contests a state root within the fraud proof window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputeRaised {
    pub root_hash: [u8; 32],
    pub challenger: String,
    pub reason: String,
    pub raised_at: i64,
}

/// Enum wrapper for all contract events, enabling uniform handling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractEvent {
    DepositDetected(DepositDetected),
    DepositConfirmed(DepositConfirmed),
    WithdrawalRequested(WithdrawalRequested),
    WithdrawalCompleted(WithdrawalCompleted),
    CommitmentSubmitted(CommitmentSubmitted),
    DisputeRaised(DisputeRaised),
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::AccountId;

    #[test]
    fn test_deposit_detected_serialization() {
        let event = DepositDetected {
            account_id: AccountId::new(),
            asset: "BTC".to_string(),
            amount: Decimal::new(100_000_000, 8), // 1.0 BTC
            tx_id: "abc123".to_string(),
            confirmations: 1,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: DepositDetected = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deser);
    }

    #[test]
    fn test_contract_event_enum_variant() {
        let event = ContractEvent::DepositDetected(DepositDetected {
            account_id: AccountId::new(),
            asset: "ETH".to_string(),
            amount: Decimal::new(5, 0),
            tx_id: "tx_001".to_string(),
            confirmations: 3,
        });
        assert!(matches!(event, ContractEvent::DepositDetected(_)));
    }

    #[test]
    fn test_withdrawal_requested_serialization() {
        let event = WithdrawalRequested {
            withdrawal_id: Uuid::now_v7(),
            account_id: AccountId::new(),
            asset: "USDT".to_string(),
            amount: Decimal::new(500_000, 2), // 5000.00
            destination: "0xabc...".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: WithdrawalRequested = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deser);
    }

    #[test]
    fn test_commitment_submitted_serialization() {
        let event = CommitmentSubmitted {
            root_hash: [0u8; 32],
            block_number: 42,
            submitter: "admin".to_string(),
            submitted_at: 1708123456789,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deser: CommitmentSubmitted = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deser);
    }
}
