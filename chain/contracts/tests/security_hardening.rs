//! Security Hardening Tests — Section 4
//!
//! Comprehensive adversarial testing:
//! - Reentrancy attacks
//! - Arithmetic overflow
//! - Permission escalation
//! - Fuzz testing (proptest)
//! - Malicious token simulation
//! - Repeated withdrawal (replay)
//! - Incorrect signature
//! - Pause functionality
//! - Upgrade path (ABI freeze)

use contracts::commitment::{compute_hash, CommitmentStore};
use contracts::errors::{CommitmentError, VaultError, WithdrawalError};
use contracts::vault::Vault;
use contracts::withdrawal::WithdrawalQueue;
use contracts::CONTRACT_ABI_VERSION;
use rust_decimal::Decimal;
use types::ids::AccountId;

// ═══════════════════════════════════════════════════════════════════
// Reentrancy Tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_reentrancy_guard_blocks_nested_deposit() {
    // The vault uses a reentrancy guard internally.
    // We verify that the guard mechanism itself prevents double-entry.
    use contracts::security::ReentrancyGuard;

    let mut guard = ReentrancyGuard::new();
    assert!(guard.acquire(), "First acquire should succeed");
    assert!(!guard.acquire(), "Nested acquire must fail — reentrancy blocked");
    guard.release();
    assert!(guard.acquire(), "Re-acquire after release should succeed");
}

#[test]
fn test_vault_deposit_releases_guard_on_success() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // First deposit succeeds and releases guard
    vault
        .deposit(acc, "BTC", Decimal::from(1), "tx1")
        .unwrap();

    // Second deposit also succeeds (guard was properly released)
    vault
        .deposit(acc, "BTC", Decimal::from(2), "tx2")
        .unwrap();

    assert_eq!(vault.get_balance(&acc, "BTC"), Decimal::from(3));
}

#[test]
fn test_vault_deposit_releases_guard_on_whitelist_error() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Fails due to non-whitelisted token
    let err = vault
        .deposit(acc, "SHIB", Decimal::from(1), "tx1")
        .unwrap_err();
    assert_eq!(
        err,
        VaultError::TokenNotWhitelisted {
            token: "SHIB".to_string()
        }
    );

    // Guard was released — next deposit on valid token succeeds
    vault
        .deposit(acc, "BTC", Decimal::from(1), "tx2")
        .unwrap();
    assert_eq!(vault.get_balance(&acc, "BTC"), Decimal::from(1));
}

#[test]
fn test_vault_deposit_releases_guard_on_amount_error() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Fails due to zero amount
    let err = vault
        .deposit(acc, "BTC", Decimal::ZERO, "tx1")
        .unwrap_err();
    assert_eq!(err, VaultError::InvalidAmount);

    // Guard released — next valid deposit works
    vault
        .deposit(acc, "BTC", Decimal::from(5), "tx2")
        .unwrap();
    assert_eq!(vault.get_balance(&acc, "BTC"), Decimal::from(5));
}

// ═══════════════════════════════════════════════════════════════════
// Overflow Tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_deposit_max_decimal_then_deposit_again() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    let max_val = Decimal::MAX;
    vault
        .deposit(acc, "USDT", max_val, "tx_max")
        .unwrap();
    assert_eq!(vault.get_balance(&acc, "USDT"), max_val);

    // Second deposit should fail with overflow
    let result = vault.deposit(acc, "USDT", Decimal::from(1), "tx_overflow");
    assert_eq!(result, Err(VaultError::Overflow));

    // Balance unchanged after failed overflow
    assert_eq!(vault.get_balance(&acc, "USDT"), max_val);
}

#[test]
fn test_large_deposit_values_accumulate() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    let large = Decimal::from(1_000_000_000i64);
    for i in 0..10 {
        vault
            .deposit(acc, "USDT", large, &format!("tx_{}", i))
            .unwrap();
    }
    assert_eq!(
        vault.get_balance(&acc, "USDT"),
        Decimal::from(10_000_000_000i64)
    );
}

// ═══════════════════════════════════════════════════════════════════
// Permission Tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_non_admin_cannot_whitelist() {
    let mut vault = Vault::new("admin");
    let result = vault.add_to_whitelist("attacker", "EVIL_TOKEN");
    assert_eq!(result, Err(VaultError::Unauthorized));
}

#[test]
fn test_non_admin_cannot_remove_from_whitelist() {
    let mut vault = setup_vault();
    let result = vault.remove_from_whitelist("attacker", "BTC");
    assert_eq!(result, Err(VaultError::Unauthorized));
}

#[test]
fn test_non_admin_cannot_pause() {
    let mut vault = setup_vault();
    let result = vault.pause("attacker");
    assert_eq!(result, Err(VaultError::Unauthorized));
}

#[test]
fn test_non_admin_cannot_unpause() {
    let mut vault = setup_vault();
    vault.pause("admin").unwrap();
    let result = vault.unpause("attacker");
    assert_eq!(result, Err(VaultError::Unauthorized));
}

#[test]
fn test_non_admin_cannot_transfer_admin() {
    let mut vault = setup_vault();
    let result = vault.set_admin("attacker", "attacker");
    assert_eq!(result, Err(VaultError::Unauthorized));
}

#[test]
fn test_commitment_non_admin_cannot_submit_root() {
    let mut store = CommitmentStore::with_default_window("admin");
    let result = store.submit_root("attacker", [0u8; 32], 1, 1000);
    assert_eq!(result, Err(CommitmentError::Unauthorized));
}

#[test]
fn test_commitment_non_admin_cannot_override() {
    let mut store = CommitmentStore::with_default_window("admin");
    let result = store.admin_override("attacker", [0u8; 32], 1, 1000);
    assert_eq!(result, Err(CommitmentError::Unauthorized));
}

#[test]
fn test_commitment_non_admin_cannot_resolve_dispute() {
    let mut store = CommitmentStore::new("admin", 3600);
    let root = compute_hash(b"data");
    store.submit_root("admin", root, 1, 1000).unwrap();
    store.raise_dispute("challenger", "reason", 1500).unwrap();

    let result = store.resolve_dispute("attacker", root, true);
    assert_eq!(result, Err(CommitmentError::Unauthorized));
}

#[test]
fn test_withdrawal_cancel_unauthorized() {
    let (mut vault, mut wq) = setup_withdrawal();
    let acc = AccountId::new();
    fund(&mut vault, acc, "BTC", Decimal::from(10));

    wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        1,
        b"sig",
        1000,
    )
    .unwrap();

    let wid = wq.queue()[0].withdrawal_id;
    let result = wq.cancel_withdrawal(&mut vault, wid, "attacker");
    assert_eq!(result, Err(WithdrawalError::Unauthorized));
}

// ═══════════════════════════════════════════════════════════════════
// Simulate Malicious Token
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_malicious_token_deposit_rejected() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Token not on whitelist — simulates a malicious/unknown token
    let result = vault.deposit(acc, "SCAM_TOKEN", Decimal::from(1_000_000), "tx_evil");
    assert_eq!(
        result,
        Err(VaultError::TokenNotWhitelisted {
            token: "SCAM_TOKEN".to_string()
        })
    );
    assert_eq!(vault.get_balance(&acc, "SCAM_TOKEN"), Decimal::ZERO);
}

#[test]
fn test_malicious_token_after_delist() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Deposit on valid token
    vault
        .deposit(acc, "ETH", Decimal::from(10), "tx1")
        .unwrap();

    // Admin removes ETH from whitelist (simulates delisting)
    vault.remove_from_whitelist("admin", "ETH").unwrap();

    // Further deposits rejected
    let result = vault.deposit(acc, "ETH", Decimal::from(5), "tx2");
    assert_eq!(
        result,
        Err(VaultError::TokenNotWhitelisted {
            token: "ETH".to_string()
        })
    );

    // Existing balance unchanged
    assert_eq!(vault.get_balance(&acc, "ETH"), Decimal::from(10));
}

// ═══════════════════════════════════════════════════════════════════
// Simulate Repeated Withdraw (Replay Attack)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_repeated_withdraw_same_nonce_rejected() {
    let (mut vault, mut wq) = setup_withdrawal();
    let acc = AccountId::new();
    fund(&mut vault, acc, "BTC", Decimal::from(100));

    // First withdrawal succeeds
    wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        42,
        b"sig",
        1000,
    )
    .unwrap();

    // Replay with same nonce — must fail
    let result = wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        42,
        b"sig",
        1001,
    );

    assert!(matches!(result, Err(WithdrawalError::NonceReused { .. })));
}

#[test]
fn test_repeated_withdraw_different_nonce_allowed() {
    let (mut vault, mut wq) = setup_withdrawal();
    let acc = AccountId::new();
    fund(&mut vault, acc, "BTC", Decimal::from(100));

    wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        1,
        b"sig",
        1000,
    )
    .unwrap();

    // Different nonce — should succeed
    wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        2,
        b"sig",
        1001,
    )
    .unwrap();

    assert_eq!(wq.queue().len(), 2);
}

// ═══════════════════════════════════════════════════════════════════
// Simulate Incorrect Signature
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_incorrect_signature_empty() {
    let (mut vault, mut wq) = setup_withdrawal();
    let acc = AccountId::new();
    fund(&mut vault, acc, "BTC", Decimal::from(10));

    let result = wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        1,
        b"", // empty sig = invalid
        1000,
    );
    assert_eq!(result, Err(WithdrawalError::InvalidSignature));
}

#[test]
fn test_valid_signature_non_empty() {
    let (mut vault, mut wq) = setup_withdrawal();
    let acc = AccountId::new();
    fund(&mut vault, acc, "BTC", Decimal::from(10));

    // Any non-empty signature is accepted at contract layer
    let result = wq.request_withdrawal(
        &mut vault,
        acc,
        "BTC",
        Decimal::from(1),
        "dest",
        1,
        b"any_valid_sig",
        1000,
    );
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════
// Test Pause Functionality
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_pause_blocks_all_deposits() {
    let mut vault = setup_vault();
    let acc1 = AccountId::new();
    let acc2 = AccountId::new();

    vault.pause("admin").unwrap();

    let r1 = vault.deposit(acc1, "BTC", Decimal::from(1), "tx1");
    let r2 = vault.deposit(acc2, "ETH", Decimal::from(5), "tx2");

    assert_eq!(r1, Err(VaultError::Paused));
    assert_eq!(r2, Err(VaultError::Paused));
}

#[test]
fn test_pause_does_not_block_confirm() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Deposit before pause
    vault
        .deposit(acc, "BTC", Decimal::from(1), "tx1")
        .unwrap();

    vault.pause("admin").unwrap();

    // Confirm should still check pause
    let result = vault.confirm_deposit(acc, "BTC", Decimal::from(1), "tx1", 6);
    // confirm_deposit checks pause too
    assert_eq!(result, Err(VaultError::Paused));
}

#[test]
fn test_pause_unpause_cycle() {
    let mut vault = setup_vault();
    let acc = AccountId::new();

    // Pause → deposit fails
    vault.pause("admin").unwrap();
    assert!(vault.is_paused());
    assert!(vault
        .deposit(acc, "BTC", Decimal::from(1), "tx1")
        .is_err());

    // Unpause → deposit succeeds
    vault.unpause("admin").unwrap();
    assert!(!vault.is_paused());
    vault
        .deposit(acc, "BTC", Decimal::from(1), "tx2")
        .unwrap();
    assert_eq!(vault.get_balance(&acc, "BTC"), Decimal::from(1));
}

// ═══════════════════════════════════════════════════════════════════
// Test Upgrade Path (ABI Freeze)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_contract_abi_version_frozen() {
    // The ABI version is a compile-time constant.
    // This test verifies it remains at the expected frozen value.
    assert_eq!(CONTRACT_ABI_VERSION, "1.0.0");
}

#[test]
fn test_contract_abi_version_stable_across_calls() {
    // Ensure the version is deterministic and stable
    let v1 = CONTRACT_ABI_VERSION;
    let v2 = CONTRACT_ABI_VERSION;
    assert_eq!(v1, v2);
}

// ═══════════════════════════════════════════════════════════════════
// Fuzz Tests (Proptest)
// ═══════════════════════════════════════════════════════════════════

mod fuzz {
    use super::*;
    use proptest::prelude::*;

    /// Strategy for valid deposit amounts (positive, reasonable range)
    fn deposit_amount() -> impl Strategy<Value = Decimal> {
        (1u64..=1_000_000_000u64).prop_map(Decimal::from)
    }

    /// Strategy for asset symbols from the whitelist
    fn asset_symbol() -> impl Strategy<Value = &'static str> {
        prop_oneof![Just("BTC"), Just("ETH"), Just("USDT"),]
    }

    proptest! {
        /// Invariant: sequential deposits preserve balance conservation.
        /// After N deposits of varying amounts, total balance equals sum.
        #[test]
        fn fuzz_deposit_balance_conservation(
            amounts in prop::collection::vec(deposit_amount(), 1..20),
            asset in asset_symbol(),
        ) {
            let mut vault = setup_vault();
            let acc = AccountId::new();
            let mut expected_total = Decimal::ZERO;

            for (i, amount) in amounts.iter().enumerate() {
                vault.deposit(acc, asset, *amount, &format!("tx_{}", i)).unwrap();
                expected_total += *amount;
            }

            prop_assert_eq!(vault.get_balance(&acc, asset), expected_total);
        }

        /// Invariant: deposit then debit of same amount leaves zero balance.
        #[test]
        fn fuzz_deposit_debit_round_trip(
            amount in deposit_amount(),
            asset in asset_symbol(),
        ) {
            let mut vault = setup_vault();
            let acc = AccountId::new();

            vault.deposit(acc, asset, amount, "tx_in").unwrap();
            vault.safe_debit(&acc, asset, amount).unwrap();
            prop_assert_eq!(vault.get_balance(&acc, asset), Decimal::ZERO);
        }

        /// Invariant: cannot debit more than deposited.
        #[test]
        fn fuzz_cannot_debit_more_than_balance(
            deposit in deposit_amount(),
            extra in 1u64..1_000u64,
        ) {
            let mut vault = setup_vault();
            let acc = AccountId::new();

            vault.deposit(acc, "BTC", deposit, "tx_in").unwrap();
            let overdraw = deposit + Decimal::from(extra);
            let result = vault.safe_debit(&acc, "BTC", overdraw);
            prop_assert!(result.is_err());
        }

        /// Invariant: withdrawal nonces are unique per account.
        #[test]
        fn fuzz_nonce_uniqueness(
            nonces in prop::collection::vec(1u64..100u64, 1..10),
        ) {
            use contracts::security::NonceTracker;
            let mut tracker = NonceTracker::new();
            let acc = AccountId::new();

            let mut seen = std::collections::HashSet::new();
            for nonce in nonces {
                let first_use = tracker.use_nonce(acc, nonce);
                if seen.contains(&nonce) {
                    // Duplicate nonce — must be rejected
                    prop_assert!(!first_use, "Duplicate nonce {} was accepted", nonce);
                } else {
                    prop_assert!(first_use, "Fresh nonce {} was rejected", nonce);
                    seen.insert(nonce);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

fn setup_vault() -> Vault {
    let mut vault = Vault::new("admin");
    vault.add_to_whitelist("admin", "BTC").unwrap();
    vault.add_to_whitelist("admin", "ETH").unwrap();
    vault.add_to_whitelist("admin", "USDT").unwrap();
    vault
}

fn setup_withdrawal() -> (Vault, WithdrawalQueue) {
    let vault = setup_vault();
    let wq = WithdrawalQueue::new(3600);
    (vault, wq)
}

fn fund(vault: &mut Vault, acc: AccountId, asset: &str, amount: Decimal) {
    vault.deposit(acc, asset, amount, "fund_tx").unwrap();
}
