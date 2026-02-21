//! Vault — Asset storage, deposits, balance tracking, and token whitelist
//!
//! Implements the custody layer per spec §16 (Custody Assumptions):
//! - Token whitelist (add/remove allowed assets)
//! - Deposit flow with detection and confirmation
//! - Balance tracking by (account, asset)
//! - Safe transfer wrapper with overflow protection
//! - Pause modifier, access control, reentrancy guard

use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use types::ids::AccountId;

use crate::errors::VaultError;
use crate::events::{ContractEvent, DepositConfirmed, DepositDetected};
use crate::security::{AccessControl, PauseGuard, ReentrancyGuard};

/// Core vault contract managing asset custody.
///
/// Balances are stored as `HashMap<AccountId, HashMap<String, Decimal>>` where
/// the inner map keys are asset symbol strings (e.g. "BTC", "ETH", "USDT").
///
/// All state-changing operations check:
/// 1. Reentrancy guard
/// 2. Pause state
/// 3. Access control (where applicable)
/// 4. Token whitelist
#[derive(Debug)]
pub struct Vault {
    /// Balances: account -> (asset -> amount)
    balances: HashMap<AccountId, HashMap<String, Decimal>>,
    /// Whitelisted token symbols
    whitelist: HashSet<String>,
    /// Security: reentrancy guard
    reentrancy_guard: ReentrancyGuard,
    /// Security: pause guard
    pause_guard: PauseGuard,
    /// Security: role-based access control
    access_control: AccessControl,
    /// Emitted events log (append-only)
    events: Vec<ContractEvent>,
}

impl Vault {
    /// Create a new vault with an admin caller.
    pub fn new(admin: impl Into<String>) -> Self {
        Self {
            balances: HashMap::new(),
            whitelist: HashSet::new(),
            reentrancy_guard: ReentrancyGuard::new(),
            pause_guard: PauseGuard::new(),
            access_control: AccessControl::new(admin),
            events: Vec::new(),
        }
    }

    // ───────────────────────── Token Whitelist ─────────────────────────

    /// Add a token to the whitelist. Admin-only.
    pub fn add_to_whitelist(
        &mut self,
        caller: &str,
        token: impl Into<String>,
    ) -> Result<(), VaultError> {
        if !self.access_control.is_admin(caller) {
            return Err(VaultError::Unauthorized);
        }
        self.whitelist.insert(token.into());
        Ok(())
    }

    /// Remove a token from the whitelist. Admin-only.
    pub fn remove_from_whitelist(
        &mut self,
        caller: &str,
        token: &str,
    ) -> Result<(), VaultError> {
        if !self.access_control.is_admin(caller) {
            return Err(VaultError::Unauthorized);
        }
        self.whitelist.remove(token);
        Ok(())
    }

    /// Check if a token is whitelisted.
    pub fn is_whitelisted(&self, token: &str) -> bool {
        self.whitelist.contains(token)
    }

    // ───────────────────────── Deposit ─────────────────────────

    /// Deposit assets into the vault for a given account.
    ///
    /// Validates: not paused, no reentrancy, token whitelisted, amount positive.
    /// Emits `DepositDetected` event.
    pub fn deposit(
        &mut self,
        account_id: AccountId,
        asset: &str,
        amount: Decimal,
        tx_id: &str,
    ) -> Result<ContractEvent, VaultError> {
        // Guard checks
        self.check_not_paused()?;
        self.check_reentrancy()?;

        // Validate token whitelist
        if !self.is_whitelisted(asset) {
            self.reentrancy_guard.release();
            return Err(VaultError::TokenNotWhitelisted {
                token: asset.to_string(),
            });
        }

        // Validate amount
        if amount <= Decimal::ZERO {
            self.reentrancy_guard.release();
            return Err(VaultError::InvalidAmount);
        }

        // Credit balance using safe_transfer
        self.safe_credit(account_id, asset, amount)?;

        // Build event
        let event = ContractEvent::DepositDetected(DepositDetected {
            account_id,
            asset: asset.to_string(),
            amount,
            tx_id: tx_id.to_string(),
            confirmations: 1,
        });

        self.events.push(event.clone());
        self.reentrancy_guard.release();
        Ok(event)
    }

    /// Confirm a deposit after required blockchain confirmations.
    ///
    /// Emits `DepositConfirmed` event. This is an idempotent status update.
    pub fn confirm_deposit(
        &mut self,
        account_id: AccountId,
        asset: &str,
        amount: Decimal,
        tx_id: &str,
        confirmations: u64,
    ) -> Result<ContractEvent, VaultError> {
        self.check_not_paused()?;

        let event = ContractEvent::DepositConfirmed(DepositConfirmed {
            account_id,
            asset: asset.to_string(),
            amount,
            tx_id: tx_id.to_string(),
            confirmations,
        });

        self.events.push(event.clone());
        Ok(event)
    }

    // ───────────────────────── Balance Queries ─────────────────────────

    /// Get balance for a specific account and asset.
    pub fn get_balance(&self, account_id: &AccountId, asset: &str) -> Decimal {
        self.balances
            .get(account_id)
            .and_then(|assets| assets.get(asset))
            .copied()
            .unwrap_or(Decimal::ZERO)
    }

    /// Get all balances for an account.
    pub fn get_account_balances(
        &self,
        account_id: &AccountId,
    ) -> Option<&HashMap<String, Decimal>> {
        self.balances.get(account_id)
    }

    // ───────────────────────── Safe Transfer ─────────────────────────

    /// Internal credit with overflow protection.
    ///
    /// Adds `amount` to the account's asset balance, checking for arithmetic overflow.
    fn safe_credit(
        &mut self,
        account_id: AccountId,
        asset: &str,
        amount: Decimal,
    ) -> Result<(), VaultError> {
        let account_balances = self.balances.entry(account_id).or_default();
        let current = account_balances.entry(asset.to_string()).or_insert(Decimal::ZERO);

        let new_balance = current
            .checked_add(amount)
            .ok_or(VaultError::Overflow)?;

        *current = new_balance;
        Ok(())
    }

    /// Internal debit with underflow protection.
    ///
    /// Subtracts `amount` from the account's asset balance.
    pub fn safe_debit(
        &mut self,
        account_id: &AccountId,
        asset: &str,
        amount: Decimal,
    ) -> Result<(), VaultError> {
        let account_balances = self
            .balances
            .get_mut(account_id)
            .ok_or_else(|| VaultError::AccountNotFound {
                account_id: account_id.to_string(),
            })?;

        let current = account_balances.get_mut(asset).ok_or_else(|| {
            VaultError::InsufficientBalance {
                asset: asset.to_string(),
                required: amount.to_string(),
                available: "0".to_string(),
            }
        })?;

        if *current < amount {
            return Err(VaultError::InsufficientBalance {
                asset: asset.to_string(),
                required: amount.to_string(),
                available: current.to_string(),
            });
        }

        let new_balance = current
            .checked_sub(amount)
            .ok_or(VaultError::Overflow)?;

        *current = new_balance;
        Ok(())
    }

    // ───────────────────────── Pause ─────────────────────────

    /// Pause the vault. Admin-only.
    pub fn pause(&mut self, caller: &str) -> Result<(), VaultError> {
        if !self.access_control.is_admin(caller) {
            return Err(VaultError::Unauthorized);
        }
        self.pause_guard.pause();
        Ok(())
    }

    /// Unpause the vault. Admin-only.
    pub fn unpause(&mut self, caller: &str) -> Result<(), VaultError> {
        if !self.access_control.is_admin(caller) {
            return Err(VaultError::Unauthorized);
        }
        self.pause_guard.unpause();
        Ok(())
    }

    /// Check if the vault is paused.
    pub fn is_paused(&self) -> bool {
        self.pause_guard.is_paused()
    }

    // ───────────────────────── Access Control ─────────────────────────

    /// Transfer admin to a new address.
    pub fn set_admin(&mut self, current_admin: &str, new_admin: &str) -> Result<(), VaultError> {
        if !self.access_control.transfer_admin(current_admin, new_admin) {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Get the current admin.
    pub fn admin(&self) -> &str {
        self.access_control.admin()
    }

    /// Get reference to access control (for withdrawal module).
    pub(crate) fn access_control(&self) -> &AccessControl {
        &self.access_control
    }

    // ───────────────────────── Events ─────────────────────────

    /// Get all emitted events.
    pub fn events(&self) -> &[ContractEvent] {
        &self.events
    }

    /// Drain all events (consume and clear).
    pub fn drain_events(&mut self) -> Vec<ContractEvent> {
        std::mem::take(&mut self.events)
    }

    // ───────────────────────── Internal Guards ─────────────────────────

    fn check_not_paused(&self) -> Result<(), VaultError> {
        if self.pause_guard.is_paused() {
            return Err(VaultError::Paused);
        }
        Ok(())
    }

    fn check_reentrancy(&mut self) -> Result<(), VaultError> {
        if !self.reentrancy_guard.acquire() {
            return Err(VaultError::Reentrancy);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_vault() -> Vault {
        let mut vault = Vault::new("admin");
        vault.add_to_whitelist("admin", "BTC").unwrap();
        vault.add_to_whitelist("admin", "ETH").unwrap();
        vault.add_to_whitelist("admin", "USDT").unwrap();
        vault
    }

    // ─── Whitelist tests ───

    #[test]
    fn test_whitelist_add_and_check() {
        let mut vault = Vault::new("admin");
        vault.add_to_whitelist("admin", "BTC").unwrap();
        assert!(vault.is_whitelisted("BTC"));
        assert!(!vault.is_whitelisted("SHIB"));
    }

    #[test]
    fn test_whitelist_remove() {
        let mut vault = setup_vault();
        vault.remove_from_whitelist("admin", "ETH").unwrap();
        assert!(!vault.is_whitelisted("ETH"));
    }

    #[test]
    fn test_whitelist_unauthorized() {
        let mut vault = Vault::new("admin");
        let result = vault.add_to_whitelist("eve", "BTC");
        assert_eq!(result, Err(VaultError::Unauthorized));
    }

    // ─── Deposit tests ───

    #[test]
    fn test_deposit_success() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let amount = Decimal::new(100_000_000, 8); // 1.0 BTC

        let event = vault.deposit(account, "BTC", amount, "tx_001").unwrap();
        assert!(matches!(event, ContractEvent::DepositDetected(_)));
        assert_eq!(vault.get_balance(&account, "BTC"), amount);
    }

    #[test]
    fn test_deposit_multiple_assets() {
        let mut vault = setup_vault();
        let account = AccountId::new();

        vault.deposit(account, "BTC", Decimal::from(2), "tx_01").unwrap();
        vault.deposit(account, "ETH", Decimal::from(10), "tx_02").unwrap();

        assert_eq!(vault.get_balance(&account, "BTC"), Decimal::from(2));
        assert_eq!(vault.get_balance(&account, "ETH"), Decimal::from(10));
    }

    #[test]
    fn test_deposit_accumulates() {
        let mut vault = setup_vault();
        let account = AccountId::new();

        vault.deposit(account, "USDT", Decimal::from(1000), "tx_01").unwrap();
        vault.deposit(account, "USDT", Decimal::from(500), "tx_02").unwrap();

        assert_eq!(vault.get_balance(&account, "USDT"), Decimal::from(1500));
    }

    #[test]
    fn test_deposit_non_whitelisted_token() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let result = vault.deposit(account, "SHIB", Decimal::from(1), "tx_01");
        assert_eq!(
            result,
            Err(VaultError::TokenNotWhitelisted {
                token: "SHIB".to_string()
            })
        );
    }

    #[test]
    fn test_deposit_zero_amount() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let result = vault.deposit(account, "BTC", Decimal::ZERO, "tx_01");
        assert_eq!(result, Err(VaultError::InvalidAmount));
    }

    #[test]
    fn test_deposit_negative_amount() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let result = vault.deposit(account, "BTC", Decimal::from(-1), "tx_01");
        assert_eq!(result, Err(VaultError::InvalidAmount));
    }

    // ─── Confirm deposit tests ───

    #[test]
    fn test_confirm_deposit() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let event = vault
            .confirm_deposit(account, "BTC", Decimal::from(1), "tx_01", 6)
            .unwrap();
        assert!(matches!(event, ContractEvent::DepositConfirmed(_)));
    }

    // ─── Balance query tests ───

    #[test]
    fn test_get_balance_empty() {
        let vault = setup_vault();
        let account = AccountId::new();
        assert_eq!(vault.get_balance(&account, "BTC"), Decimal::ZERO);
    }

    #[test]
    fn test_get_account_balances() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.deposit(account, "BTC", Decimal::from(5), "tx_01").unwrap();
        vault.deposit(account, "ETH", Decimal::from(20), "tx_02").unwrap();

        let balances = vault.get_account_balances(&account).unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances["BTC"], Decimal::from(5));
    }

    // ─── Safe debit tests ───

    #[test]
    fn test_safe_debit_success() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.deposit(account, "BTC", Decimal::from(10), "tx_01").unwrap();
        vault.safe_debit(&account, "BTC", Decimal::from(3)).unwrap();
        assert_eq!(vault.get_balance(&account, "BTC"), Decimal::from(7));
    }

    #[test]
    fn test_safe_debit_insufficient() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.deposit(account, "BTC", Decimal::from(1), "tx_01").unwrap();
        let result = vault.safe_debit(&account, "BTC", Decimal::from(5));
        assert!(matches!(result, Err(VaultError::InsufficientBalance { .. })));
    }

    #[test]
    fn test_safe_debit_no_account() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        let result = vault.safe_debit(&account, "BTC", Decimal::from(1));
        assert!(matches!(result, Err(VaultError::AccountNotFound { .. })));
    }

    // ─── Pause tests ───

    #[test]
    fn test_pause_blocks_deposit() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.pause("admin").unwrap();
        let result = vault.deposit(account, "BTC", Decimal::from(1), "tx_01");
        assert_eq!(result, Err(VaultError::Paused));
    }

    #[test]
    fn test_unpause_allows_deposit() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.pause("admin").unwrap();
        vault.unpause("admin").unwrap();
        assert!(vault.deposit(account, "BTC", Decimal::from(1), "tx_01").is_ok());
    }

    #[test]
    fn test_pause_unauthorized() {
        let mut vault = setup_vault();
        let result = vault.pause("eve");
        assert_eq!(result, Err(VaultError::Unauthorized));
    }

    // ─── Access control tests ───

    #[test]
    fn test_set_admin() {
        let mut vault = Vault::new("alice");
        vault.set_admin("alice", "bob").unwrap();
        assert_eq!(vault.admin(), "bob");
    }

    #[test]
    fn test_set_admin_unauthorized() {
        let mut vault = Vault::new("alice");
        let result = vault.set_admin("eve", "bob");
        assert_eq!(result, Err(VaultError::Unauthorized));
    }

    // ─── Events tests ───

    #[test]
    fn test_events_emitted() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.deposit(account, "BTC", Decimal::from(1), "tx_01").unwrap();
        vault.deposit(account, "ETH", Decimal::from(5), "tx_02").unwrap();

        assert_eq!(vault.events().len(), 2);
    }

    #[test]
    fn test_drain_events() {
        let mut vault = setup_vault();
        let account = AccountId::new();
        vault.deposit(account, "BTC", Decimal::from(1), "tx_01").unwrap();

        let events = vault.drain_events();
        assert_eq!(events.len(), 1);
        assert!(vault.events().is_empty());
    }

    // ─── Multiple accounts ───

    #[test]
    fn test_multiple_accounts_isolated() {
        let mut vault = setup_vault();
        let acc1 = AccountId::new();
        let acc2 = AccountId::new();

        vault.deposit(acc1, "BTC", Decimal::from(10), "tx_01").unwrap();
        vault.deposit(acc2, "BTC", Decimal::from(5), "tx_02").unwrap();

        assert_eq!(vault.get_balance(&acc1, "BTC"), Decimal::from(10));
        assert_eq!(vault.get_balance(&acc2, "BTC"), Decimal::from(5));
    }
}
