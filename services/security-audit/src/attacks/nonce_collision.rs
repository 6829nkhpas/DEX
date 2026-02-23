//! Nonce collision test.
//! Ensures that requests with duplicated or old sequence numbers (nonces) are rejected
//! to prevent replay attacks over the API.

use std::collections::HashMap;

pub struct NonceTracker {
    /// Maps Account/API Key to the highest seen nonce
    last_nonces: HashMap<String, u64>,
}

#[derive(Debug, PartialEq)]
pub enum NonceError {
    NonceTooLow,
    NonceReused,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self {
            last_nonces: HashMap::new(),
        }
    }

    pub fn process(&mut self, account_id: &str, nonce: u64) -> Result<(), NonceError> {
        let last = self.last_nonces.get(account_id).unwrap_or(&0);
        
        if nonce < *last {
            return Err(NonceError::NonceTooLow);
        }
        if nonce == *last && *last != 0 {
            return Err(NonceError::NonceReused);
        }
        
        self.last_nonces.insert(account_id.to_string(), nonce);
        Ok(())
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

    #[test]
    fn test_nonce_collision_mitigation() {
        let mut tracker = NonceTracker::new();
        let account = "acc_123";

        // Initial request works
        assert_eq!(tracker.process(account, 100), Ok(()));
        
        // Next sequential request works
        assert_eq!(tracker.process(account, 101), Ok(()));

        // Reused nonce fails
        assert_eq!(tracker.process(account, 101), Err(NonceError::NonceReused));

        // Old nonce fails
        assert_eq!(tracker.process(account, 99), Err(NonceError::NonceTooLow));

        // Future nonce works (skipping is typically allowed, just must be strictly increasing)
        assert_eq!(tracker.process(account, 150), Ok(()));
    }
}
