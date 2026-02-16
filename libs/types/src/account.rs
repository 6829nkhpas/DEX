//! Account and balance types
//!
//! Implements spec §4 (Account State Model)

use crate::ids::AccountId;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Account type per spec §4.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccountType {
    /// Spot trading only
    SPOT,
    /// Margin trading with leverage
    MARGIN,
    /// Perpetual futures trading
    FUTURES,
}

/// Account status per spec §4.2.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccountStatus {
    /// Active and can trade
    ACTIVE,
    /// Temporarily suspended
    SUSPENDED,
    /// Permanently closed
    CLOSED,
    /// Liquidation in progress
    LIQUIDATING,
}

/// Balance for a single asset per spec §4.3
///
/// Invariant: total = available + locked
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub total: Decimal,
    pub available: Decimal,
    pub locked: Decimal,
}

impl Balance {
    /// Create a new balance
    pub fn new(asset: impl Into<String>, total: Decimal) -> Self {
        Self {
            asset: asset.into(),
            total,
            available: total,
            locked: Decimal::ZERO,
        }
    }

    /// Check balance invariant: total = available + locked
    pub fn check_invariant(&self) -> bool {
        self.total == self.available + self.locked
    }

    /// Lock a portion of available balance
    ///
    /// # Panics
    /// Panics if amount exceeds available or violates invariant
    pub fn lock(&mut self, amount: Decimal) {
        assert!(amount >= Decimal::ZERO, "Lock amount must be non-negative");
        assert!(amount <= self.available, "Insufficient available balance");
        
        self.available -= amount;
        self.locked += amount;
        
        assert!(self.check_invariant(), "Invariant violated after lock");
    }

    /// Unlock a portion of locked balance
    ///
    /// # Panics
    /// Panics if amount exceeds locked or violates invariant
    pub fn unlock(&mut self, amount: Decimal) {
        assert!(amount >= Decimal::ZERO, "Unlock amount must be non-negative");
        assert!(amount <= self.locked, "Insufficient locked balance");
        
        self.locked -= amount;
        self.available += amount;
        
        assert!(self.check_invariant(), "Invariant violated after unlock");
    }

    /// Deduct from locked balance (e.g., after order fill)
    pub fn deduct_locked(&mut self, amount: Decimal) {
        assert!(amount >= Decimal::ZERO, "Deduct amount must be non-negative");
        assert!(amount <= self.locked, "Insufficient locked balance");
        
        self.locked -= amount;
        self.total -= amount;
        
        assert!(self.check_invariant(), "Invariant violated after deduct");
    }

    /// Credit to available balance (e.g., deposit, trade settlement)
    pub fn credit(&mut self, amount: Decimal) {
        assert!(amount >= Decimal::ZERO, "Credit amount must be non-negative");
        
        self.available += amount;
        self.total += amount;
        
        assert!(self.check_invariant(), "Invariant violated after credit");
    }
}

/// Account structure per spec §4.2
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub account_id: AccountId,
    pub account_type: AccountType,
    pub status: AccountStatus,
    pub balances: HashMap<String, Balance>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: u64,
}

impl Account {
    /// Create a new account
    pub fn new(account_type: AccountType, timestamp: i64) -> Self {
        Self {
            account_id: AccountId::new(),
            account_type,
            status: AccountStatus::ACTIVE,
            balances: HashMap::new(),
            created_at: timestamp,
            updated_at: timestamp,
            version: 0,
        }
    }

    /// Get balance for an asset
    pub fn get_balance(&self, asset: &str) -> Option<&Balance> {
        self.balances.get(asset)
    }

    /// Get mutable balance for an asset
    pub fn get_balance_mut(&mut self, asset: &str) -> Option<&mut Balance> {
        self.balances.get_mut(asset)
    }

    /// Add or update balance for an asset
    pub fn set_balance(&mut self, balance: Balance, timestamp: i64) {
        self.balances.insert(balance.asset.clone(), balance);
        self.updated_at = timestamp;
        self.version += 1;
    }

    /// Check if account is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, AccountStatus::ACTIVE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_creation() {
        let balance = Balance::new("USDT", Decimal::from(10000));
        assert_eq!(balance.total, Decimal::from(10000));
        assert_eq!(balance.available, Decimal::from(10000));
        assert_eq!(balance.locked, Decimal::ZERO);
        assert!(balance.check_invariant());
    }

    #[test]
    fn test_balance_lock() {
        let mut balance = Balance::new("USDT", Decimal::from(10000));
        balance.lock(Decimal::from(3000));
        
        assert_eq!(balance.total, Decimal::from(10000));
        assert_eq!(balance.available, Decimal::from(7000));
        assert_eq!(balance.locked, Decimal::from(3000));
        assert!(balance.check_invariant());
    }

    #[test]
    fn test_balance_unlock() {
        let mut balance = Balance::new("USDT", Decimal::from(10000));
        balance.lock(Decimal::from(3000));
        balance.unlock(Decimal::from(1000));
        
        assert_eq!(balance.available, Decimal::from(8000));
        assert_eq!(balance.locked, Decimal::from(2000));
        assert!(balance.check_invariant());
    }

    #[test]
    fn test_balance_deduct() {
        let mut balance = Balance::new("USDT", Decimal::from(10000));
        balance.lock(Decimal::from(3000));
        balance.deduct_locked(Decimal::from(1000));
        
        assert_eq!(balance.total, Decimal::from(9000));
        assert_eq!(balance.locked, Decimal::from(2000));
        assert!(balance.check_invariant());
    }

    #[test]
    fn test_balance_credit() {
        let mut balance = Balance::new("USDT", Decimal::from(10000));
        balance.credit(Decimal::from(5000));
        
        assert_eq!(balance.total, Decimal::from(15000));
        assert_eq!(balance.available, Decimal::from(15000));
        assert!(balance.check_invariant());
    }

    #[test]
    #[should_panic(expected = "Insufficient available balance")]
    fn test_balance_overlock_panics() {
        let mut balance = Balance::new("USDT", Decimal::from(10000));
        balance.lock(Decimal::from(15000));
    }

    #[test]
    fn test_account_creation() {
        let account = Account::new(AccountType::SPOT, 1708123456789000000);
        assert_eq!(account.account_type, AccountType::SPOT);
        assert_eq!(account.status, AccountStatus::ACTIVE);
        assert!(account.is_active());
        assert!(account.balances.is_empty());
    }

    #[test]
    fn test_account_balance_management() {
        let mut account = Account::new(AccountType::MARGIN, 1708123456789000000);
        
        let balance = Balance::new("BTC", Decimal::from(5));
        account.set_balance(balance, 1708123456790000000);
        
        assert!(account.get_balance("BTC").is_some());
        assert_eq!(account.get_balance("BTC").unwrap().total, Decimal::from(5));
    }
}

