//! Persistence & Deterministic Replay Service
//!
//! Provides append-only journal writing, sequential reading with corruption
//! detection, state snapshots, crash recovery, and determinism guarantees
//! for the distributed exchange.
//!
//! # Spec Compliance
//! - §08 Event Taxonomy (event structure)
//! - §10 Failure Recovery (checksums, snapshots, WAL)
//! - §11 Replay Requirements (deterministic replay, snapshots)
//! - §12 Determinism Rules (no side effects, sorted iteration)
//! - §14 Sequence Numbering (gapless, monotonic)

pub mod journal;
pub mod reader;
pub mod snapshot;
pub mod recovery;
pub mod determinism;
