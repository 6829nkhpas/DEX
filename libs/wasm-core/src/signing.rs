//! Signing Module — Transaction signing and verification
//!
//! Provides deterministic message serialization, SHA-256 hashing,
//! Ed25519 signing/verification, nonce tracking, and replay protection.
//! Implements spec §19 (Security Invariants).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Signing schema version (frozen).
pub const SIGNING_SCHEMA_VERSION: &str = "1.0.0";

/// Maximum age of a signable message (replay protection window, 5 minutes).
const MAX_MESSAGE_AGE_NS: i64 = 5 * 60 * 1_000_000_000;

// ---------------------------------------------------------------------------
// Signable message
// ---------------------------------------------------------------------------

/// A message payload prepared for signing.
///
/// Uses `BTreeMap` for deterministic serialization (spec §12.3).
/// The canonical byte representation is the UTF-8 encoded JSON string
/// produced by `serde_json` over this struct's sorted fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignableMessage {
    /// Schema version (always `SIGNING_SCHEMA_VERSION`)
    pub version: String,
    /// Action name, e.g. "CreateOrder", "CancelOrder", "Withdraw"
    pub action: String,
    /// Deterministic payload (sorted keys)
    pub payload: BTreeMap<String, String>,
    /// Exchange timestamp (unix nanos) from the request
    pub timestamp: i64,
    /// Monotonic nonce for the signing account
    pub nonce: u64,
}

impl SignableMessage {
    /// Create a new signable message.
    pub fn new(
        action: impl Into<String>,
        payload: BTreeMap<String, String>,
        timestamp: i64,
        nonce: u64,
    ) -> Self {
        Self {
            version: SIGNING_SCHEMA_VERSION.to_owned(),
            action: action.into(),
            payload,
            timestamp,
            nonce,
        }
    }

    /// Serialize to canonical JSON bytes (deterministic, spec §12.8.1).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        // serde_json serializes BTreeMap in sorted-key order
        serde_json::to_vec(self).expect("SignableMessage serialization must not fail")
    }

    /// SHA-256 hash of the canonical bytes.
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.canonical_bytes());
        hasher.finalize().into()
    }

    /// SHA-256 hash as hex string.
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

// ---------------------------------------------------------------------------
// Signing / Verification
// ---------------------------------------------------------------------------

/// Signed message: message + signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedMessage {
    pub message: SignableMessage,
    /// Ed25519 signature as hex string
    pub signature: String,
    /// Public key of the signer as hex string
    pub public_key: String,
}

/// Sign a message with an Ed25519 private key.
///
/// Returns a `SignedMessage` containing the original message,
/// the hex-encoded signature, and the hex-encoded public key.
pub fn sign_message(message: &SignableMessage, signing_key: &SigningKey) -> SignedMessage {
    let hash = message.hash();
    let signature = signing_key.sign(&hash);
    let verifying_key = signing_key.verifying_key();

    SignedMessage {
        message: message.clone(),
        signature: hex::encode(signature.to_bytes()),
        public_key: hex::encode(verifying_key.to_bytes()),
    }
}

/// Verify a signed message.
///
/// Returns `Ok(())` if the signature is valid, `Err` otherwise.
pub fn verify_signature(signed: &SignedMessage) -> Result<(), SigningError> {
    let pub_bytes = hex::decode(&signed.public_key)
        .map_err(|_| SigningError::InvalidPublicKey)?;
    let sig_bytes = hex::decode(&signed.signature)
        .map_err(|_| SigningError::InvalidSignature)?;

    let pub_key_bytes: [u8; 32] = pub_bytes
        .try_into()
        .map_err(|_| SigningError::InvalidPublicKey)?;
    let sig_key_bytes: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| SigningError::InvalidSignature)?;

    let verifying_key = VerifyingKey::from_bytes(&pub_key_bytes)
        .map_err(|_| SigningError::InvalidPublicKey)?;
    let signature = Signature::from_bytes(&sig_key_bytes);

    let hash = signed.message.hash();
    verifying_key
        .verify(&hash, &signature)
        .map_err(|_| SigningError::VerificationFailed)
}

// ---------------------------------------------------------------------------
// Nonce tracking (replay protection)
// ---------------------------------------------------------------------------

/// Nonce validator for replay protection.
///
/// Tracks the last-seen nonce per account and rejects replayed messages.
#[derive(Debug, Clone, Default)]
pub struct NonceTracker {
    /// Last seen nonce per account (account_id hex → nonce)
    last_nonce: BTreeMap<String, u64>,
}

impl NonceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate and advance the nonce for an account.
    ///
    /// Returns `Ok(())` if the nonce is strictly greater than the last seen.
    pub fn validate_and_advance(
        &mut self,
        account_id: &str,
        nonce: u64,
    ) -> Result<(), SigningError> {
        let last = self.last_nonce.get(account_id).copied().unwrap_or(0);
        if nonce <= last {
            return Err(SigningError::NonceReplay {
                provided: nonce,
                last_seen: last,
            });
        }
        self.last_nonce.insert(account_id.to_owned(), nonce);
        Ok(())
    }

    /// Check if a message timestamp is within the replay protection window.
    pub fn validate_timestamp(
        &self,
        message_timestamp: i64,
        current_timestamp: i64,
    ) -> Result<(), SigningError> {
        let age = current_timestamp - message_timestamp;
        if age < 0 {
            return Err(SigningError::FutureTimestamp);
        }
        if age > MAX_MESSAGE_AGE_NS {
            return Err(SigningError::ExpiredMessage);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hardware wallet stub
// ---------------------------------------------------------------------------

/// Trait for hardware wallet integration (stub).
pub trait HardwareWallet {
    /// Sign a message hash with the hardware wallet.
    fn sign(&self, message_hash: &[u8; 32]) -> Result<Vec<u8>, SigningError>;

    /// Get the public key from the hardware wallet.
    fn public_key(&self) -> Result<Vec<u8>, SigningError>;
}

/// Stub implementation that always returns an error.
#[derive(Debug, Clone)]
pub struct StubHardwareWallet;

impl HardwareWallet for StubHardwareWallet {
    fn sign(&self, _message_hash: &[u8; 32]) -> Result<Vec<u8>, SigningError> {
        Err(SigningError::HardwareWalletUnsupported)
    }

    fn public_key(&self) -> Result<Vec<u8>, SigningError> {
        Err(SigningError::HardwareWalletUnsupported)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Signing module errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SigningError {
    #[error("Invalid public key")]
    InvalidPublicKey,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Signature verification failed")]
    VerificationFailed,

    #[error("Nonce replay: provided {provided}, last seen {last_seen}")]
    NonceReplay { provided: u64, last_seen: u64 },

    #[error("Message timestamp is in the future")]
    FutureTimestamp,

    #[error("Message has expired (outside replay window)")]
    ExpiredMessage,

    #[error("Hardware wallet not supported")]
    HardwareWalletUnsupported,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn sample_message(nonce: u64) -> SignableMessage {
        let mut payload = BTreeMap::new();
        payload.insert("symbol".to_owned(), "BTC/USDT".to_owned());
        payload.insert("side".to_owned(), "BUY".to_owned());
        payload.insert("quantity".to_owned(), "1.5".to_owned());
        payload.insert("price".to_owned(), "50000.00".to_owned());

        SignableMessage::new("CreateOrder", payload, 1_708_123_456_789_000_000, nonce)
    }

    fn test_keypair() -> SigningKey {
        // Deterministic seed for repeatable test vectors
        let seed: [u8; 32] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
        ];
        SigningKey::from_bytes(&seed)
    }

    #[test]
    fn test_canonical_serialization_deterministic() {
        let msg1 = sample_message(1);
        let msg2 = sample_message(1);
        assert_eq!(msg1.canonical_bytes(), msg2.canonical_bytes());
    }

    #[test]
    fn test_canonical_serialization_sorted_keys() {
        let msg = sample_message(1);
        let json = String::from_utf8(msg.canonical_bytes()).unwrap();
        // BTreeMap ensures sorted order: price < quantity < side < symbol
        assert!(json.contains("\"price\":\"50000.00\""));
        let price_pos = json.find("\"price\"").unwrap();
        let qty_pos = json.find("\"quantity\"").unwrap();
        let side_pos = json.find("\"side\"").unwrap();
        let sym_pos = json.find("\"symbol\"").unwrap();
        assert!(price_pos < qty_pos);
        assert!(qty_pos < side_pos);
        assert!(side_pos < sym_pos);
    }

    #[test]
    fn test_hash_deterministic() {
        let msg1 = sample_message(1);
        let msg2 = sample_message(1);
        assert_eq!(msg1.hash(), msg2.hash());
        assert_eq!(msg1.hash_hex(), msg2.hash_hex());
    }

    #[test]
    fn test_hash_changes_with_content() {
        let msg1 = sample_message(1);
        let msg2 = sample_message(2);
        assert_ne!(msg1.hash(), msg2.hash());
    }

    #[test]
    fn test_sign_and_verify() {
        let key = test_keypair();
        let msg = sample_message(1);
        let signed = sign_message(&msg, &key);
        assert!(verify_signature(&signed).is_ok());
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let key = test_keypair();
        let msg = sample_message(1);
        let mut signed = sign_message(&msg, &key);

        // Tamper with the public key
        let other_key = SigningKey::generate(&mut OsRng);
        signed.public_key = hex::encode(other_key.verifying_key().to_bytes());

        assert_eq!(
            verify_signature(&signed),
            Err(SigningError::VerificationFailed)
        );
    }

    #[test]
    fn test_verify_tampered_message_fails() {
        let key = test_keypair();
        let msg = sample_message(1);
        let mut signed = sign_message(&msg, &key);

        // Tamper with the message
        signed.message.nonce = 999;

        assert_eq!(
            verify_signature(&signed),
            Err(SigningError::VerificationFailed)
        );
    }

    #[test]
    fn test_verify_invalid_signature_hex() {
        let key = test_keypair();
        let msg = sample_message(1);
        let mut signed = sign_message(&msg, &key);
        signed.signature = "not_hex".to_owned();
        assert_eq!(
            verify_signature(&signed),
            Err(SigningError::InvalidSignature)
        );
    }

    #[test]
    fn test_verify_invalid_public_key_hex() {
        let key = test_keypair();
        let msg = sample_message(1);
        let mut signed = sign_message(&msg, &key);
        signed.public_key = "not_hex".to_owned();
        assert_eq!(
            verify_signature(&signed),
            Err(SigningError::InvalidPublicKey)
        );
    }

    #[test]
    fn test_nonce_tracker_valid() {
        let mut tracker = NonceTracker::new();
        assert!(tracker.validate_and_advance("acc1", 1).is_ok());
        assert!(tracker.validate_and_advance("acc1", 2).is_ok());
        assert!(tracker.validate_and_advance("acc1", 10).is_ok());
    }

    #[test]
    fn test_nonce_tracker_replay_rejected() {
        let mut tracker = NonceTracker::new();
        tracker.validate_and_advance("acc1", 5).unwrap();

        assert_eq!(
            tracker.validate_and_advance("acc1", 5),
            Err(SigningError::NonceReplay {
                provided: 5,
                last_seen: 5
            })
        );
        assert_eq!(
            tracker.validate_and_advance("acc1", 3),
            Err(SigningError::NonceReplay {
                provided: 3,
                last_seen: 5
            })
        );
    }

    #[test]
    fn test_nonce_tracker_per_account() {
        let mut tracker = NonceTracker::new();
        tracker.validate_and_advance("acc1", 5).unwrap();
        // Different account starts fresh
        assert!(tracker.validate_and_advance("acc2", 1).is_ok());
    }

    #[test]
    fn test_timestamp_within_window() {
        let tracker = NonceTracker::new();
        let now = 1_708_123_456_789_000_000i64;
        let one_minute_ago = now - 60_000_000_000;
        assert!(tracker.validate_timestamp(one_minute_ago, now).is_ok());
    }

    #[test]
    fn test_timestamp_expired() {
        let tracker = NonceTracker::new();
        let now = 1_708_123_456_789_000_000i64;
        let ten_minutes_ago = now - 10 * 60 * 1_000_000_000;
        assert_eq!(
            tracker.validate_timestamp(ten_minutes_ago, now),
            Err(SigningError::ExpiredMessage)
        );
    }

    #[test]
    fn test_timestamp_future() {
        let tracker = NonceTracker::new();
        let now = 1_708_123_456_789_000_000i64;
        let future = now + 1_000_000_000;
        assert_eq!(
            tracker.validate_timestamp(future, now),
            Err(SigningError::FutureTimestamp)
        );
    }

    #[test]
    fn test_hardware_wallet_stub() {
        let hw = StubHardwareWallet;
        let hash = [0u8; 32];
        assert_eq!(
            hw.sign(&hash),
            Err(SigningError::HardwareWalletUnsupported)
        );
        assert_eq!(
            hw.public_key(),
            Err(SigningError::HardwareWalletUnsupported)
        );
    }

    #[test]
    fn test_signing_schema_version() {
        let msg = sample_message(1);
        assert_eq!(msg.version, "1.0.0");
    }

    #[test]
    fn test_signed_message_serialization() {
        let key = test_keypair();
        let msg = sample_message(1);
        let signed = sign_message(&msg, &key);
        let json = serde_json::to_string(&signed).unwrap();
        let restored: SignedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(signed, restored);
    }

    // -- Test vectors (deterministic with fixed seed) ----------------------

    #[test]
    fn test_vector_hash() {
        let msg = sample_message(1);
        let hash_hex = msg.hash_hex();
        // With the same message, hash must always be the same
        let msg2 = sample_message(1);
        assert_eq!(hash_hex, msg2.hash_hex());
        // Non-empty 64-char hex string (SHA-256)
        assert_eq!(hash_hex.len(), 64);
    }

    #[test]
    fn test_vector_signature_stable() {
        let key = test_keypair();
        let msg = sample_message(1);
        let signed1 = sign_message(&msg, &key);
        let signed2 = sign_message(&msg, &key);
        // Ed25519 with deterministic key is deterministic
        assert_eq!(signed1.signature, signed2.signature);
        assert_eq!(signed1.public_key, signed2.public_key);
    }

    #[test]
    fn test_vector_signature_length() {
        let key = test_keypair();
        let msg = sample_message(1);
        let signed = sign_message(&msg, &key);
        // Ed25519 signature = 64 bytes = 128 hex chars
        assert_eq!(signed.signature.len(), 128);
        // Public key = 32 bytes = 64 hex chars
        assert_eq!(signed.public_key.len(), 64);
    }
}
