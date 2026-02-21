//! Shared security primitives for contract modules
//!
//! Provides reusable guards and access control used across vault,
//! withdrawal, and commitment modules.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use types::ids::AccountId;

/// Reentrancy guard preventing nested calls into protected functions.
///
/// A contract function acquires the guard before executing state-changing
/// logic and releases it on completion. Any nested call attempt fails.
#[derive(Debug, Clone)]
pub struct ReentrancyGuard {
    locked: bool,
}

impl ReentrancyGuard {
    /// Create a new unlocked guard.
    pub fn new() -> Self {
        Self { locked: false }
    }

    /// Acquire the guard. Returns `true` if successfully acquired.
    /// Returns `false` if already locked (reentrancy attempt).
    pub fn acquire(&mut self) -> bool {
        if self.locked {
            return false;
        }
        self.locked = true;
        true
    }

    /// Release the guard.
    pub fn release(&mut self) {
        self.locked = false;
    }

    /// Check if currently locked.
    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

impl Default for ReentrancyGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Access control roles per spec §17 (Governance Hooks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Full system control
    Admin,
    /// Operational tasks (e.g., submitting roots)
    Operator,
    /// Regular user
    User,
}

/// Role-based access control manager.
///
/// Maps callers (identified by string) to their assigned roles.
/// Admin role is required for sensitive operations like pause, whitelist, etc.
#[derive(Debug, Clone)]
pub struct AccessControl {
    roles: HashMap<String, Role>,
    admin: String,
}

impl AccessControl {
    /// Create access control with an initial admin.
    pub fn new(admin: impl Into<String>) -> Self {
        let admin_str = admin.into();
        let mut roles = HashMap::new();
        roles.insert(admin_str.clone(), Role::Admin);
        Self {
            roles,
            admin: admin_str,
        }
    }

    /// Check if a caller has the specified role.
    pub fn has_role(&self, caller: &str, role: Role) -> bool {
        self.roles.get(caller).map_or(false, |r| *r == role)
    }

    /// Check if a caller is admin.
    pub fn is_admin(&self, caller: &str) -> bool {
        self.has_role(caller, Role::Admin)
    }

    /// Assign a role to a caller. Only admin can assign roles.
    pub fn grant_role(&mut self, admin_caller: &str, target: impl Into<String>, role: Role) -> bool {
        if !self.is_admin(admin_caller) {
            return false;
        }
        self.roles.insert(target.into(), role);
        true
    }

    /// Remove a role from a caller. Only admin can revoke.
    pub fn revoke_role(&mut self, admin_caller: &str, target: &str) -> bool {
        if !self.is_admin(admin_caller) {
            return false;
        }
        // Cannot revoke the primary admin
        if target == self.admin {
            return false;
        }
        self.roles.remove(target);
        true
    }

    /// Transfer admin to a new address.
    pub fn transfer_admin(&mut self, current_admin: &str, new_admin: impl Into<String>) -> bool {
        if !self.is_admin(current_admin) {
            return false;
        }
        let new_admin_str = new_admin.into();
        self.roles.remove(current_admin);
        self.roles.insert(new_admin_str.clone(), Role::Admin);
        self.admin = new_admin_str;
        true
    }

    /// Get the current admin identifier.
    pub fn admin(&self) -> &str {
        &self.admin
    }
}

/// Composable pause modifier.
///
/// When paused, protected operations must be rejected.
#[derive(Debug, Clone)]
pub struct PauseGuard {
    paused: bool,
}

impl PauseGuard {
    /// Create a new unpaused guard.
    pub fn new() -> Self {
        Self { paused: false }
    }

    /// Pause operations.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Unpause operations.
    pub fn unpause(&mut self) {
        self.paused = false;
    }

    /// Check if currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

impl Default for PauseGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-account nonce tracker for replay protection.
///
/// Each account has a monotonically increasing nonce.
/// A nonce can only be used once per account.
#[derive(Debug, Clone)]
pub struct NonceTracker {
    used_nonces: HashSet<(AccountId, u64)>,
}

impl NonceTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            used_nonces: HashSet::new(),
        }
    }

    /// Check if a nonce has been used for an account.
    pub fn is_used(&self, account_id: &AccountId, nonce: u64) -> bool {
        self.used_nonces.contains(&(*account_id, nonce))
    }

    /// Mark a nonce as used. Returns `false` if already used (replay attempt).
    pub fn use_nonce(&mut self, account_id: AccountId, nonce: u64) -> bool {
        self.used_nonces.insert((account_id, nonce))
    }

    /// Number of tracked nonces.
    pub fn count(&self) -> usize {
        self.used_nonces.len()
    }
}

impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ReentrancyGuard tests ---

    #[test]
    fn test_reentrancy_guard_acquire_release() {
        let mut guard = ReentrancyGuard::new();
        assert!(!guard.is_locked());
        assert!(guard.acquire());
        assert!(guard.is_locked());
        guard.release();
        assert!(!guard.is_locked());
    }

    #[test]
    fn test_reentrancy_guard_double_acquire_fails() {
        let mut guard = ReentrancyGuard::new();
        assert!(guard.acquire());
        assert!(!guard.acquire(), "Second acquire must fail");
    }

    #[test]
    fn test_reentrancy_guard_reacquire_after_release() {
        let mut guard = ReentrancyGuard::new();
        assert!(guard.acquire());
        guard.release();
        assert!(guard.acquire(), "Should succeed after release");
    }

    // --- AccessControl tests ---

    #[test]
    fn test_access_control_admin() {
        let ac = AccessControl::new("alice");
        assert!(ac.is_admin("alice"));
        assert!(!ac.is_admin("bob"));
    }

    #[test]
    fn test_access_control_grant_role() {
        let mut ac = AccessControl::new("alice");
        assert!(ac.grant_role("alice", "bob", Role::Operator));
        assert!(ac.has_role("bob", Role::Operator));
    }

    #[test]
    fn test_access_control_non_admin_cannot_grant() {
        let mut ac = AccessControl::new("alice");
        assert!(!ac.grant_role("bob", "charlie", Role::Operator));
    }

    #[test]
    fn test_access_control_revoke_role() {
        let mut ac = AccessControl::new("alice");
        ac.grant_role("alice", "bob", Role::Operator);
        assert!(ac.revoke_role("alice", "bob"));
        assert!(!ac.has_role("bob", Role::Operator));
    }

    #[test]
    fn test_access_control_cannot_revoke_primary_admin() {
        let mut ac = AccessControl::new("alice");
        assert!(!ac.revoke_role("alice", "alice"));
    }

    #[test]
    fn test_access_control_transfer_admin() {
        let mut ac = AccessControl::new("alice");
        assert!(ac.transfer_admin("alice", "bob"));
        assert!(ac.is_admin("bob"));
        assert!(!ac.is_admin("alice"));
        assert_eq!(ac.admin(), "bob");
    }

    // --- PauseGuard tests ---

    #[test]
    fn test_pause_guard() {
        let mut pg = PauseGuard::new();
        assert!(!pg.is_paused());
        pg.pause();
        assert!(pg.is_paused());
        pg.unpause();
        assert!(!pg.is_paused());
    }

    // --- NonceTracker tests ---

    #[test]
    fn test_nonce_tracker_use_once() {
        let mut tracker = NonceTracker::new();
        let acc = AccountId::new();
        assert!(tracker.use_nonce(acc, 1));
        assert!(tracker.is_used(&acc, 1));
    }

    #[test]
    fn test_nonce_tracker_replay_rejected() {
        let mut tracker = NonceTracker::new();
        let acc = AccountId::new();
        assert!(tracker.use_nonce(acc, 1));
        assert!(!tracker.use_nonce(acc, 1), "Second use must return false");
    }

    #[test]
    fn test_nonce_tracker_different_accounts() {
        let mut tracker = NonceTracker::new();
        let acc1 = AccountId::new();
        let acc2 = AccountId::new();
        assert!(tracker.use_nonce(acc1, 1));
        assert!(tracker.use_nonce(acc2, 1), "Same nonce on different account is OK");
    }

    #[test]
    fn test_nonce_tracker_count() {
        let mut tracker = NonceTracker::new();
        let acc = AccountId::new();
        tracker.use_nonce(acc, 1);
        tracker.use_nonce(acc, 2);
        assert_eq!(tracker.count(), 2);
    }
}
