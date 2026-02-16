//! Error types for the matching engine
//!
//! Comprehensive error taxonomy using thiserror

use thiserror::Error;

/// Top-level engine error
#[derive(Error, Debug, Clone, PartialEq)]
pub enum EngineError {
    #[error("Order error: {0}")]
    Order(#[from] OrderError),
    
    #[error("Trade error: {0}")]
    Trade(#[from] TradeError),
    
    #[error("Account error: {0}")]
    Account(#[from] AccountError),
    
    #[error("Liquidation error: {0}")]
    Liquidation(#[from] LiquidationError),
    
    #[error("Invalid market: {symbol}")]
    InvalidMarket { symbol: String },
    
    #[error("System error: {message}")]
    System { message: String },
}

/// Order-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum OrderError {
    #[error("Invalid price: {0}")]
    InvalidPrice(String),
    
    #[error("Invalid quantity: {0}")]
    InvalidQuantity(String),
    
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: String, available: String },
    
    #[error("Order not found: {order_id}")]
    NotFound { order_id: String },
    
    #[error("Order already in terminal state: {status}")]
    AlreadyTerminal { status: String },
    
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },
    
    #[error("Self-trade prevention triggered")]
    SelfTrade,
    
    #[error("Post-only order would take liquidity")]
    PostOnlyReject,
}

/// Trade-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TradeError {
    #[error("Trade not found: {trade_id}")]
    NotFound { trade_id: String },
    
    #[error("Trade already settled")]
    AlreadySettled,
    
    #[error("Settlement failed: {reason}")]
    SettlementFailed { reason: String },
    
    #[error("Invalid trade: {reason}")]
    InvalidTrade { reason: String },
}

/// Account-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AccountError {
    #[error("Account not found: {account_id}")]
    NotFound { account_id: String },
    
    #[error("Account suspended")]
    Suspended,
    
    #[error("Account closed")]
    Closed,
    
    #[error("Insufficient balance for asset {asset}: required {required}, available {available}")]
    InsufficientBalance {
        asset: String,
        required: String,
        available: String,
    },
    
    #[error("Balance invariant violated for asset {asset}")]
    InvariantViolation { asset: String },
    
    #[error("Asset not found: {asset}")]
    AssetNotFound { asset: String },
}

/// Liquidation-specific errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum LiquidationError {
    #[error("Position not found")]
    PositionNotFound,
    
    #[error("Insufficient insurance fund: required {required}, available {available}")]
    InsufficientInsurance { required: String, available: String },
    
    #[error("Liquidation already in progress")]
    AlreadyLiquidating,
    
    #[error("Position not eligible for liquidation: margin_ratio {margin_ratio}")]
    NotEligible { margin_ratio: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_error_display() {
        let err = OrderError::InvalidPrice("negative".to_string());
        assert_eq!(err.to_string(), "Invalid price: negative");
    }

    #[test]
    fn test_account_error_insufficient_balance() {
        let err = AccountError::InsufficientBalance {
            asset: "BTC".to_string(),
            required: "1.5".to_string(),
            available: "1.0".to_string(),
        };
        assert!(err.to_string().contains("BTC"));
        assert!(err.to_string().contains("1.5"));
    }

    #[test]
    fn test_engine_error_from_order_error() {
        let order_err = OrderError::SelfTrade;
        let engine_err: EngineError = order_err.into();
        assert!(matches!(engine_err, EngineError::Order(_)));
    }
}
