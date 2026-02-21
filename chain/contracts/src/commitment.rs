//! State Commitment — periodic state roots, fraud proofs, disputes
//!
//! Implements the state commitment layer for the exchange:
//! - Authorized submitters post periodic state root hashes
//! - Fraud proof window allows challenges
//! - Dispute resolution by admin
//! - Admin override for emergency situations

use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

use crate::errors::CommitmentError;
use crate::events::{CommitmentSubmitted, ContractEvent, DisputeRaised};
use crate::security::AccessControl;

/// A single state commitment record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateCommitment {
    pub root_hash: [u8; 32],
    pub block_number: u64,
    pub submitted_at: i64,
    pub submitter: String,
}

/// Status of a dispute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeStatus {
    /// Dispute raised, awaiting resolution
    Pending,
    /// Dispute accepted — commitment invalidated
    Accepted,
    /// Dispute rejected — commitment stands
    Rejected,
}

/// A dispute against a committed state root.
#[derive(Debug, Clone)]
pub struct Dispute {
    pub root_hash: [u8; 32],
    pub challenger: String,
    pub reason: String,
    pub raised_at: i64,
    pub status: DisputeStatus,
}

/// State commitment store managing roots, fraud proofs, and disputes.
#[derive(Debug)]
pub struct CommitmentStore {
    /// Ordered history of commitments
    history: Vec<StateCommitment>,
    /// Active disputes
    disputes: Vec<Dispute>,
    /// Fraud proof window in seconds (default: 7200 = 2 hours)
    fraud_window_seconds: i64,
    /// Access control for admin/operator roles
    access_control: AccessControl,
    /// Emitted events
    events: Vec<ContractEvent>,
}

impl CommitmentStore {
    /// Create a new commitment store with an admin.
    pub fn new(admin: impl Into<String>, fraud_window_seconds: i64) -> Self {
        Self {
            history: Vec::new(),
            disputes: Vec::new(),
            fraud_window_seconds,
            access_control: AccessControl::new(admin),
            events: Vec::new(),
        }
    }

    /// Create with default 2-hour fraud window.
    pub fn with_default_window(admin: impl Into<String>) -> Self {
        Self::new(admin, 7200)
    }

    /// Submit a new state root. Only admin or operator can submit.
    pub fn submit_root(
        &mut self,
        caller: &str,
        root_hash: [u8; 32],
        block_number: u64,
        current_time: i64,
    ) -> Result<ContractEvent, CommitmentError> {
        if !self.access_control.is_admin(caller)
            && !self.access_control.has_role(caller, crate::security::Role::Operator)
        {
            return Err(CommitmentError::Unauthorized);
        }

        let commitment = StateCommitment {
            root_hash,
            block_number,
            submitted_at: current_time,
            submitter: caller.to_string(),
        };

        self.history.push(commitment);

        let event = ContractEvent::CommitmentSubmitted(CommitmentSubmitted {
            root_hash,
            block_number,
            submitter: caller.to_string(),
            submitted_at: current_time,
        });

        self.events.push(event.clone());
        Ok(event)
    }

    /// Get the latest committed state root.
    pub fn get_latest_root(&self) -> Result<&StateCommitment, CommitmentError> {
        self.history.last().ok_or(CommitmentError::NoCommitment)
    }

    /// Get the full commitment history.
    pub fn history(&self) -> &[StateCommitment] {
        &self.history
    }

    /// Validate a proof stub against a given root.
    ///
    /// This is a placeholder for full Merkle proof validation.
    /// In production, this would verify a Merkle branch from leaf to root.
    /// Currently: recomputes the hash of `data` and checks it matches `expected_hash`.
    pub fn validate_proof_stub(
        data: &[u8],
        expected_hash: &[u8; 32],
    ) -> Result<bool, CommitmentError> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();

        let computed: [u8; 32] = result.into();
        if computed == *expected_hash {
            Ok(true)
        } else {
            Err(CommitmentError::InvalidProof)
        }
    }

    /// Raise a dispute against the latest commitment.
    ///
    /// Anyone can dispute within the fraud proof window.
    pub fn raise_dispute(
        &mut self,
        challenger: &str,
        reason: &str,
        current_time: i64,
    ) -> Result<ContractEvent, CommitmentError> {
        let latest = self
            .history
            .last()
            .ok_or(CommitmentError::NoCommitment)?;

        // Check fraud window
        let elapsed = current_time - latest.submitted_at;
        if elapsed >= self.fraud_window_seconds {
            return Err(CommitmentError::FraudWindowExpired);
        }

        // Check for duplicate dispute on same root by same challenger
        let duplicate = self.disputes.iter().any(|d| {
            d.root_hash == latest.root_hash
                && d.challenger == challenger
                && d.status == DisputeStatus::Pending
        });
        if duplicate {
            return Err(CommitmentError::DuplicateDispute);
        }

        let dispute = Dispute {
            root_hash: latest.root_hash,
            challenger: challenger.to_string(),
            reason: reason.to_string(),
            raised_at: current_time,
            status: DisputeStatus::Pending,
        };

        self.disputes.push(dispute);

        let event = ContractEvent::DisputeRaised(DisputeRaised {
            root_hash: latest.root_hash,
            challenger: challenger.to_string(),
            reason: reason.to_string(),
            raised_at: current_time,
        });

        self.events.push(event.clone());
        Ok(event)
    }

    /// Resolve a pending dispute. Admin-only.
    ///
    /// If accepted, the disputed commitment is invalidated (removed from history).
    /// If rejected, the dispute is marked as rejected.
    pub fn resolve_dispute(
        &mut self,
        caller: &str,
        root_hash: [u8; 32],
        accept: bool,
    ) -> Result<(), CommitmentError> {
        if !self.access_control.is_admin(caller) {
            return Err(CommitmentError::Unauthorized);
        }

        let dispute = self
            .disputes
            .iter_mut()
            .find(|d| d.root_hash == root_hash && d.status == DisputeStatus::Pending)
            .ok_or(CommitmentError::DisputeNotFound)?;

        if accept {
            dispute.status = DisputeStatus::Accepted;
            // Remove the disputed commitment from history
            self.history.retain(|c| c.root_hash != root_hash);
        } else {
            dispute.status = DisputeStatus::Rejected;
        }

        Ok(())
    }

    /// Admin override to force-set a new state root.
    ///
    /// Used in emergency situations. Bypasses normal submission flow.
    pub fn admin_override(
        &mut self,
        caller: &str,
        root_hash: [u8; 32],
        block_number: u64,
        current_time: i64,
    ) -> Result<ContractEvent, CommitmentError> {
        if !self.access_control.is_admin(caller) {
            return Err(CommitmentError::Unauthorized);
        }

        let commitment = StateCommitment {
            root_hash,
            block_number,
            submitted_at: current_time,
            submitter: format!("{} (override)", caller),
        };

        self.history.push(commitment);

        let event = ContractEvent::CommitmentSubmitted(CommitmentSubmitted {
            root_hash,
            block_number,
            submitter: format!("{} (override)", caller),
            submitted_at: current_time,
        });

        self.events.push(event.clone());
        Ok(event)
    }

    /// Get active disputes.
    pub fn disputes(&self) -> &[Dispute] {
        &self.disputes
    }

    /// Get emitted events.
    pub fn events(&self) -> &[ContractEvent] {
        &self.events
    }

    /// Drain emitted events.
    pub fn drain_events(&mut self) -> Vec<ContractEvent> {
        std::mem::take(&mut self.events)
    }

    /// Grant operator role for root submission.
    pub fn grant_operator(&mut self, admin: &str, operator: impl Into<String>) -> bool {
        self.access_control
            .grant_role(admin, operator, crate::security::Role::Operator)
    }
}

/// Compute a SHA-256 hash of arbitrary data (utility for tests/proofs).
pub fn compute_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> [u8; 32] {
        compute_hash(b"test_state_data")
    }

    #[test]
    fn test_submit_root_success() {
        let mut store = CommitmentStore::with_default_window("admin");
        let root = test_root();

        let event = store.submit_root("admin", root, 1, 1000).unwrap();
        assert!(matches!(event, ContractEvent::CommitmentSubmitted(_)));
        assert_eq!(store.history().len(), 1);
    }

    #[test]
    fn test_submit_root_unauthorized() {
        let mut store = CommitmentStore::with_default_window("admin");
        let result = store.submit_root("eve", test_root(), 1, 1000);
        assert_eq!(result, Err(CommitmentError::Unauthorized));
    }

    #[test]
    fn test_submit_root_by_operator() {
        let mut store = CommitmentStore::with_default_window("admin");
        store.grant_operator("admin", "operator1");

        let event = store.submit_root("operator1", test_root(), 1, 1000).unwrap();
        assert!(matches!(event, ContractEvent::CommitmentSubmitted(_)));
    }

    #[test]
    fn test_get_latest_root() {
        let mut store = CommitmentStore::with_default_window("admin");
        store.submit_root("admin", test_root(), 1, 1000).unwrap();

        let root2 = compute_hash(b"state_2");
        store.submit_root("admin", root2, 2, 2000).unwrap();

        let latest = store.get_latest_root().unwrap();
        assert_eq!(latest.root_hash, root2);
        assert_eq!(latest.block_number, 2);
    }

    #[test]
    fn test_get_latest_root_empty() {
        let store = CommitmentStore::with_default_window("admin");
        let result = store.get_latest_root();
        assert_eq!(result, Err(CommitmentError::NoCommitment));
    }

    #[test]
    fn test_validate_proof_stub_valid() {
        let data = b"hello world";
        let hash = compute_hash(data);
        let result = CommitmentStore::validate_proof_stub(data, &hash).unwrap();
        assert!(result);
    }

    #[test]
    fn test_validate_proof_stub_invalid() {
        let data = b"hello world";
        let wrong_hash = [0u8; 32];
        let result = CommitmentStore::validate_proof_stub(data, &wrong_hash);
        assert_eq!(result, Err(CommitmentError::InvalidProof));
    }

    #[test]
    fn test_raise_dispute_within_window() {
        let mut store = CommitmentStore::new("admin", 3600);
        store.submit_root("admin", test_root(), 1, 1000).unwrap();

        let event = store
            .raise_dispute("challenger", "invalid balances", 2000)
            .unwrap();
        assert!(matches!(event, ContractEvent::DisputeRaised(_)));
        assert_eq!(store.disputes().len(), 1);
    }

    #[test]
    fn test_raise_dispute_after_window_expires() {
        let mut store = CommitmentStore::new("admin", 3600);
        store.submit_root("admin", test_root(), 1, 1000).unwrap();

        let result = store.raise_dispute("challenger", "too late", 5000);
        assert_eq!(result, Err(CommitmentError::FraudWindowExpired));
    }

    #[test]
    fn test_raise_dispute_duplicate() {
        let mut store = CommitmentStore::new("admin", 3600);
        store.submit_root("admin", test_root(), 1, 1000).unwrap();

        store
            .raise_dispute("challenger", "reason 1", 2000)
            .unwrap();
        let result = store.raise_dispute("challenger", "reason 2", 2500);
        assert_eq!(result, Err(CommitmentError::DuplicateDispute));
    }

    #[test]
    fn test_resolve_dispute_accept() {
        let mut store = CommitmentStore::new("admin", 3600);
        let root = test_root();
        store.submit_root("admin", root, 1, 1000).unwrap();
        store.raise_dispute("challenger", "bad root", 2000).unwrap();

        store.resolve_dispute("admin", root, true).unwrap();

        // Commitment removed from history
        assert!(store.history().is_empty());
        assert_eq!(store.disputes()[0].status, DisputeStatus::Accepted);
    }

    #[test]
    fn test_resolve_dispute_reject() {
        let mut store = CommitmentStore::new("admin", 3600);
        let root = test_root();
        store.submit_root("admin", root, 1, 1000).unwrap();
        store.raise_dispute("challenger", "bad root", 2000).unwrap();

        store.resolve_dispute("admin", root, false).unwrap();

        // Commitment still in history
        assert_eq!(store.history().len(), 1);
        assert_eq!(store.disputes()[0].status, DisputeStatus::Rejected);
    }

    #[test]
    fn test_resolve_dispute_unauthorized() {
        let mut store = CommitmentStore::new("admin", 3600);
        let root = test_root();
        store.submit_root("admin", root, 1, 1000).unwrap();
        store.raise_dispute("challenger", "bad root", 2000).unwrap();

        let result = store.resolve_dispute("eve", root, true);
        assert_eq!(result, Err(CommitmentError::Unauthorized));
    }

    #[test]
    fn test_admin_override() {
        let mut store = CommitmentStore::with_default_window("admin");
        let root = compute_hash(b"emergency_state");

        let event = store.admin_override("admin", root, 99, 5000).unwrap();
        assert!(matches!(event, ContractEvent::CommitmentSubmitted(_)));

        let latest = store.get_latest_root().unwrap();
        assert_eq!(latest.root_hash, root);
        assert!(latest.submitter.contains("override"));
    }

    #[test]
    fn test_admin_override_unauthorized() {
        let mut store = CommitmentStore::with_default_window("admin");
        let result = store.admin_override("eve", [0u8; 32], 1, 1000);
        assert_eq!(result, Err(CommitmentError::Unauthorized));
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let h1 = compute_hash(b"same input");
        let h2 = compute_hash(b"same input");
        assert_eq!(h1, h2);

        let h3 = compute_hash(b"different input");
        assert_ne!(h1, h3);
    }
}
