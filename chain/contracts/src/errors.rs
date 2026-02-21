//! Contract-specific error types
//!
//! Comprehensive error taxonomy for vault, withdrawal, and commitment operations.

use thiserror::Error;

/// Vault-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum VaultError {
    #[error("Token not whitelisted: {token}")]
    TokenNotWhitelisted { token: String },

    #[error("Vault is paused")]
    Paused,

    #[error("Reentrancy detected")]
    Reentrancy,

    #[error("Insufficient balance for {asset}: required {required}, available {available}")]
    InsufficientBalance {
        asset: String,
        required: String,
        available: String,
    },

    #[error("Unauthorized: caller is not admin")]
    Unauthorized,

    #[error("Account not found: {account_id}")]
    AccountNotFound { account_id: String },

    #[error("Deposit amount must be positive")]
    InvalidAmount,

    #[error("Arithmetic overflow in balance calculation")]
    Overflow,
}

/// Withdrawal-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum WithdrawalError {
    #[error("Invalid signature for withdrawal")]
    InvalidSignature,

    #[error("Nonce already used: account {account_id}, nonce {nonce}")]
    NonceReused { account_id: String, nonce: u64 },

    #[error("Withdrawal delay not elapsed: available at {available_at}")]
    DelayNotElapsed { available_at: i64 },

    #[error("Withdrawal not found: {withdrawal_id}")]
    NotFound { withdrawal_id: String },

    #[error("Withdrawal already cancelled")]
    AlreadyCancelled,

    #[error("Withdrawal already processed")]
    AlreadyProcessed,

    #[error("Insufficient balance for withdrawal")]
    InsufficientBalance,

    #[error("Unauthorized: only owner or admin can cancel")]
    Unauthorized,

    #[error("Vault error: {0}")]
    Vault(#[from] VaultError),

    #[error("Invalid withdrawal amount: must be positive")]
    InvalidAmount,

    #[error("Empty batch: no withdrawals to process")]
    EmptyBatch,
}

/// Commitment-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CommitmentError {
    #[error("Invalid proof: verification failed")]
    InvalidProof,

    #[error("Within fraud proof window: {remaining_seconds}s remaining")]
    WithinFraudWindow { remaining_seconds: i64 },

    #[error("Unauthorized: caller is not authorized submitter")]
    Unauthorized,

    #[error("No commitment found")]
    NoCommitment,

    #[error("Dispute already raised for this root")]
    DuplicateDispute,

    #[error("Fraud proof window expired: dispute no longer allowed")]
    FraudWindowExpired,

    #[error("Dispute not found")]
    DisputeNotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_error_display() {
        let err = VaultError::TokenNotWhitelisted {
            token: "SHIB".to_string(),
        };
        assert_eq!(err.to_string(), "Token not whitelisted: SHIB");
    }

    #[test]
    fn test_withdrawal_error_display() {
        let err = WithdrawalError::NonceReused {
            account_id: "acc-1".to_string(),
            nonce: 42,
        };
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn test_commitment_error_display() {
        let err = CommitmentError::WithinFraudWindow {
            remaining_seconds: 3600,
        };
        assert!(err.to_string().contains("3600"));
    }

    #[test]
    fn test_withdrawal_error_from_vault() {
        let vault_err = VaultError::Paused;
        let withdrawal_err: WithdrawalError = vault_err.into();
        assert!(matches!(withdrawal_err, WithdrawalError::Vault(_)));
    }
}
