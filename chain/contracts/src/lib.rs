//! Smart Contract Logic for Custody & Settlement
//!
//! This crate implements the on-chain contract layer for the distributed exchange,
//! covering asset custody (vault), withdrawal processing, and state commitment.
//!
//! # Modules
//! - `events`: Contract events matching spec §08 event taxonomy
//! - `errors`: Contract-specific error types
//! - `security`: Shared security primitives (reentrancy guard, access control, pause)
//! - `vault`: Asset storage, deposits, balance tracking, token whitelist
//! - `withdrawal`: Withdrawal requests, signature verification, batch processing
//! - `commitment`: State root commitment, fraud proofs, dispute resolution
//!
//! # Version
//! v0.1.0 — Spec-compliant initial implementation

pub mod errors;
pub mod events;
pub mod security;
pub mod vault;
pub mod withdrawal;
pub mod commitment;

/// Contract ABI version — frozen after release
pub const CONTRACT_ABI_VERSION: &str = "1.0.0";
