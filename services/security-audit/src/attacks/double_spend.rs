//! Double spend attack simulation.
//! Simulates concurrent attempts to withdraw and place an order using the same balance,
//! verifying that optimistic locking prevents negative balances.

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use types::account::{Account, AccountType, Balance};
use types::ids::AccountId;

/// Simulates a data store that enforces optimistic locking on Account updates.
pub struct AccountStore {
    accounts: Mutex<HashMap<AccountId, Account>>,
}

#[derive(Debug, PartialEq)]
pub enum StoreError {
    NotFound,
    VersionConflict,
    InsufficientBalance,
}

impl AccountStore {
    pub fn new() -> Self {
        Self {
            accounts: Mutex::new(HashMap::new()),
        }
    }

    pub fn insert(&self, account: Account) {
        self.accounts
            .lock()
            .unwrap()
            .insert(account.account_id, account);
    }

    pub fn get(&self, id: &AccountId) -> Option<Account> {
        self.accounts.lock().unwrap().get(id).cloned()
    }

    /// Attempts to update the account, enforcing the version check.
    pub fn update(&self, mut account: Account) -> Result<(), StoreError> {
        let mut map = self.accounts.lock().unwrap();
        let existing = map.get(&account.account_id).ok_or(StoreError::NotFound)?;

        // Optimistic locking check
        if existing.version != account.version {
            return Err(StoreError::VersionConflict);
        }

        // Increment version on update
        account.version += 1;
        map.insert(account.account_id, account);
        Ok(())
    }
}

impl Default for AccountStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Simulated handler for locking margin (e.g. for placing an order)
pub fn handle_order_placement(
    store: Arc<AccountStore>,
    account_id: AccountId,
    amount: Decimal,
) -> Result<(), StoreError> {
    let mut current_account = store.get(&account_id).ok_or(StoreError::NotFound)?;

    // Check balance
    let balance = current_account
        .get_balance_mut("USDC")
        .ok_or(StoreError::NotFound)?;
    if balance.available < amount {
        return Err(StoreError::InsufficientBalance);
    }

    // Mutate state (lock balance)
    balance.lock(amount);

    // Attempt to persist
    store.update(current_account)
}

/// Simulated handler for withdrawing balance
pub fn handle_withdrawal(
    store: Arc<AccountStore>,
    account_id: AccountId,
    amount: Decimal,
) -> Result<(), StoreError> {
    let mut current_account = store.get(&account_id).ok_or(StoreError::NotFound)?;

    let balance = current_account
        .get_balance_mut("USDC")
        .ok_or(StoreError::NotFound)?;
    if balance.available < amount {
        return Err(StoreError::InsufficientBalance);
    }

    // Mutate state (deduct available)
    // We simulate deduction by locking and then deducting locked
    balance.lock(amount);
    balance.deduct_locked(amount);

    // Attempt to persist
    store.update(current_account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_double_spend_mitigation() {
        let store = Arc::new(AccountStore::new());
        let mut account = Account::new(AccountType::SPOT, 1708123456789000000);
        let account_id = account.account_id;

        // Start with 100 USDC
        account.set_balance(
            Balance::new("USDC", Decimal::from(100)),
            1708123456789000000,
        );
        // set_balance increments version to 1.

        store.insert(account);

        let amount_to_spend = Decimal::from(100);

        // We'll mimic two threads trying to spend the same 100 USDC exactly at the same time.
        // We will just fetch the state manually to guarantee the race condition scenario without
        // relying on thread schedules.

        let account_view_for_order = store.get(&account_id).unwrap();
        let account_view_for_withdraw = store.get(&account_id).unwrap();

        // 1. Thread A processes order placement logic purely in memory
        let mut mutated_for_order = account_view_for_order.clone();
        mutated_for_order
            .get_balance_mut("USDC")
            .unwrap()
            .lock(amount_to_spend);

        // 2. Thread B processes withdrawal logic purely in memory
        let mut mutated_for_withdraw = account_view_for_withdraw.clone();
        let bal = mutated_for_withdraw.get_balance_mut("USDC").unwrap();
        bal.lock(amount_to_spend);
        bal.deduct_locked(amount_to_spend);

        // 3. Thread A commits the change successfully
        assert_eq!(store.update(mutated_for_order), Ok(()));

        // 4. Thread B attempts to commit its change expecting version 1, but version is now 2.
        assert_eq!(
            store.update(mutated_for_withdraw),
            Err(StoreError::VersionConflict)
        );

        // Let's simulate a retry loop for thread B. It fetches latest and tries again:
        let result_on_retry = handle_withdrawal(store.clone(), account_id, amount_to_spend);

        // It correctly fails due to insufficient balance instead of double spending.
        assert_eq!(result_on_retry, Err(StoreError::InsufficientBalance));

        // Verify the final state is sound
        let final_account = store.get(&account_id).unwrap();
        let final_balance = final_account.get_balance("USDC").unwrap();

        assert_eq!(final_balance.total, Decimal::from(100)); // order hasn't filled or deducted yet
        assert_eq!(final_balance.available, Decimal::ZERO); // all locked by the order
        assert_eq!(final_balance.locked, Decimal::from(100));
        assert!(final_balance.check_invariant());
    }
}
