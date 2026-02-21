//! Recovery Flow — Boot from snapshot + journal replay
//!
//! Implements spec §10.4 (crash recovery), §11.6 (snapshot restore),
//! §11.7 (checksum verification), §12 (determinism).
//!
//! Recovery process:
//! 1. Find latest snapshot (if any)
//! 2. Load snapshot → engine state
//! 3. Open journal reader, seek to snapshot.sequence + 1
//! 4. Replay all subsequent events, applying them to state
//! 5. Validate final state hash matches expected
//! 6. Abort on divergence with detailed diagnostics

use crate::journal::JournalEntry;
use crate::reader::JournalReader;
use crate::snapshot::{
    EngineState, Snapshot, SnapshotError, SnapshotLoader, SnapshotWriter,
};
use std::path::PathBuf;
use std::time::Instant;
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("Snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),

    #[error("Reader error: {0}")]
    Reader(#[from] crate::reader::ReaderError),

    #[error("State hash divergence: expected {expected}, got {actual} at sequence {sequence}")]
    HashDivergence {
        expected: String,
        actual: String,
        sequence: u64,
    },

    #[error("Recovery failed: {0}")]
    Failed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Recovery Metrics ────────────────────────────────────────────────

/// Metrics collected during the recovery process.
#[derive(Debug, Clone)]
pub struct RecoveryMetrics {
    /// Time to load the snapshot (if any).
    pub snapshot_load_time_ms: u64,
    /// Sequence number of the loaded snapshot (0 if none).
    pub snapshot_sequence: u64,
    /// Number of journal entries replayed.
    pub replay_count: u64,
    /// Time spent replaying journal entries.
    pub replay_time_ms: u64,
    /// Total recovery time (snapshot load + replay + validation).
    pub total_recovery_time_ms: u64,
    /// Final state hash after recovery.
    pub final_state_hash: String,
    /// Final sequence number after recovery.
    pub final_sequence: u64,
    /// Whether recovery completed successfully.
    pub success: bool,
}

impl RecoveryMetrics {
    fn new() -> Self {
        Self {
            snapshot_load_time_ms: 0,
            snapshot_sequence: 0,
            replay_count: 0,
            replay_time_ms: 0,
            total_recovery_time_ms: 0,
            final_state_hash: String::new(),
            final_sequence: 0,
            success: false,
        }
    }
}

// ── Recovery Log Entry ──────────────────────────────────────────────

/// Structured recovery log entry for diagnostics.
#[derive(Debug, Clone)]
pub struct RecoveryLogEntry {
    pub stage: RecoveryStage,
    pub message: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStage {
    Start,
    SnapshotSearch,
    SnapshotLoad,
    JournalOpen,
    JournalSeek,
    Replay,
    Validation,
    Complete,
    Error,
}

// ── Event Applier ───────────────────────────────────────────────────

/// Trait for applying journal entries to engine state.
///
/// The matching engine or other consumers implement this trait
/// to define how events transform state.
pub trait EventApplier {
    /// Apply a journal entry to the engine state.
    fn apply(&self, state: &mut EngineState, entry: &JournalEntry) -> Result<(), String>;
}

/// Default no-op event applier (for testing / cold-start scenarios).
/// Tracks entries by storing their sequence numbers in the state.
pub struct DefaultEventApplier;

impl EventApplier for DefaultEventApplier {
    fn apply(&self, state: &mut EngineState, entry: &JournalEntry) -> Result<(), String> {
        // Default applier: store entry metadata in balances as a record
        // This is a minimal implementation for recovery testing
        let key = format!("__replay_seq_{}", entry.sequence);
        state.balances.insert(
            key,
            crate::snapshot::BalanceSnapshot {
                account_id: "__replay".to_string(),
                asset: entry.event_type.clone(),
                total: entry.sequence.to_string(),
                available: entry.timestamp.to_string(),
                locked: "0".to_string(),
            },
        );
        Ok(())
    }
}

// ── Recovery Engine ─────────────────────────────────────────────────

/// Main recovery engine. Orchestrates snapshot loading + journal replay.
pub struct RecoveryEngine {
    snapshot_dir: PathBuf,
    journal_dir: PathBuf,
    log: Vec<RecoveryLogEntry>,
}

impl RecoveryEngine {
    /// Create a new recovery engine.
    pub fn new(snapshot_dir: impl Into<PathBuf>, journal_dir: impl Into<PathBuf>) -> Self {
        Self {
            snapshot_dir: snapshot_dir.into(),
            journal_dir: journal_dir.into(),
            log: Vec::new(),
        }
    }

    /// Execute full recovery: snapshot load + journal replay + validation.
    pub fn recover(
        &mut self,
        applier: &dyn EventApplier,
        expected_hash: Option<&str>,
    ) -> Result<(EngineState, RecoveryMetrics), RecoveryError> {
        let total_start = Instant::now();
        let mut metrics = RecoveryMetrics::new();

        self.log_stage(RecoveryStage::Start, "Recovery started", 0);

        // Step 1: Load snapshot (if available)
        let (mut state, snapshot_seq) = self.load_snapshot(&mut metrics)?;

        // Step 2: Open journal and seek past snapshot
        self.log_stage(RecoveryStage::JournalOpen, "Opening journal", 0);
        let mut reader = JournalReader::open(&self.journal_dir)?;

        if snapshot_seq > 0 {
            self.log_stage(
                RecoveryStage::JournalSeek,
                &format!("Seeking to sequence {}", snapshot_seq + 1),
                0,
            );
            reader.seek_to_sequence(snapshot_seq + 1)?;
        }

        // Step 3: Replay journal entries
        let replay_start = Instant::now();
        self.log_stage(RecoveryStage::Replay, "Starting journal replay", 0);

        let mut last_seq = snapshot_seq;
        loop {
            match reader.next_entry() {
                Ok(Some(entry)) => {
                    applier
                        .apply(&mut state, &entry)
                        .map_err(|e| RecoveryError::Failed(format!("Apply error: {}", e)))?;
                    last_seq = entry.sequence;
                    metrics.replay_count += 1;
                }
                Ok(None) => break,
                Err(e) => {
                    self.log_stage(
                        RecoveryStage::Error,
                        &format!("Replay error: {}", e),
                        replay_start.elapsed().as_millis() as u64,
                    );
                    return Err(RecoveryError::Reader(e));
                }
            }
        }

        metrics.replay_time_ms = replay_start.elapsed().as_millis() as u64;
        metrics.final_sequence = last_seq;

        self.log_stage(
            RecoveryStage::Replay,
            &format!(
                "Replayed {} entries in {}ms",
                metrics.replay_count, metrics.replay_time_ms
            ),
            metrics.replay_time_ms,
        );

        // Step 4: Validate state hash
        let final_hash = state.compute_hash();
        metrics.final_state_hash = final_hash.clone();

        if let Some(expected) = expected_hash {
            self.log_stage(RecoveryStage::Validation, "Validating state hash", 0);
            if final_hash != expected {
                self.log_stage(
                    RecoveryStage::Error,
                    &format!("Hash divergence: expected={}, actual={}", expected, final_hash),
                    0,
                );
                return Err(RecoveryError::HashDivergence {
                    expected: expected.to_string(),
                    actual: final_hash,
                    sequence: last_seq,
                });
            }
        }

        metrics.total_recovery_time_ms = total_start.elapsed().as_millis() as u64;
        metrics.success = true;

        self.log_stage(
            RecoveryStage::Complete,
            &format!(
                "Recovery complete: {} events in {}ms, final seq={}",
                metrics.replay_count, metrics.total_recovery_time_ms, last_seq
            ),
            metrics.total_recovery_time_ms,
        );

        Ok((state, metrics))
    }

    /// Execute recovery without hash validation (for cold starts).
    pub fn recover_without_validation(
        &mut self,
        applier: &dyn EventApplier,
    ) -> Result<(EngineState, RecoveryMetrics), RecoveryError> {
        self.recover(applier, None)
    }

    /// Take a snapshot of the current state.
    pub fn take_snapshot(
        &self,
        state: &EngineState,
        sequence: u64,
        timestamp: i64,
        compress: bool,
    ) -> Result<PathBuf, RecoveryError> {
        let writer = SnapshotWriter::new(&self.snapshot_dir, compress);
        let snapshot = Snapshot::new(sequence, timestamp, state.clone(), compress);
        let path = writer.write(&snapshot)?;
        Ok(path)
    }

    /// Get recovery log entries.
    pub fn log(&self) -> &[RecoveryLogEntry] {
        &self.log
    }

    // ── Internal ────────────────────────────────────────────────────

    fn load_snapshot(
        &mut self,
        metrics: &mut RecoveryMetrics,
    ) -> Result<(EngineState, u64), RecoveryError> {
        self.log_stage(RecoveryStage::SnapshotSearch, "Searching for snapshots", 0);
        let loader = SnapshotLoader::new(&self.snapshot_dir);

        match loader.load_latest() {
            Ok(snapshot) => {
                let start = Instant::now();
                self.log_stage(
                    RecoveryStage::SnapshotLoad,
                    &format!(
                        "Loading snapshot at sequence {}",
                        snapshot.sequence
                    ),
                    0,
                );
                metrics.snapshot_load_time_ms = start.elapsed().as_millis() as u64;
                metrics.snapshot_sequence = snapshot.sequence;

                self.log_stage(
                    RecoveryStage::SnapshotLoad,
                    &format!(
                        "Snapshot loaded: seq={}, hash={}",
                        snapshot.sequence,
                        &snapshot.checksum[..16]
                    ),
                    metrics.snapshot_load_time_ms,
                );

                Ok((snapshot.state, snapshot.sequence))
            }
            Err(SnapshotError::NoSnapshots) => {
                self.log_stage(
                    RecoveryStage::SnapshotSearch,
                    "No snapshots found, starting from empty state",
                    0,
                );
                Ok((EngineState::empty(), 0))
            }
            Err(e) => Err(RecoveryError::Snapshot(e)),
        }
    }

    fn log_stage(&mut self, stage: RecoveryStage, message: &str, elapsed_ms: u64) {
        self.log.push(RecoveryLogEntry {
            stage,
            message: message.to_string(),
            elapsed_ms,
        });
    }
}

// ── Replay Contract ─────────────────────────────────────────────────

/// Frozen replay contract: defines the exact recovery behavior.
///
/// This struct encodes the invariants that must hold for recovery:
/// 1. Snapshot state + replayed events = final state (deterministic)
/// 2. State hash at sequence S must be identical across all replays
/// 3. Events are applied in strict sequence order
/// 4. Missing or corrupted events abort recovery
pub struct ReplayContract;

impl ReplayContract {
    /// Validate that two states are identical (for determinism verification).
    pub fn verify_determinism(state_a: &EngineState, state_b: &EngineState) -> bool {
        state_a.compute_hash() == state_b.compute_hash()
    }

    /// Validate that a recovery produced the expected state.
    pub fn verify_recovery(state: &EngineState, expected_hash: &str) -> bool {
        state.compute_hash() == expected_hash
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{JournalConfig, JournalEntry, JournalWriter};
    use crate::snapshot::EngineState;
    use std::path::Path;
    use tempfile::TempDir;

    fn write_journal(dir: &Path, start_seq: u64, count: u64) {
        let config = JournalConfig::new(dir);
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(start_seq);
        for seq in start_seq..start_seq + count {
            let entry = JournalEntry::new(
                seq,
                1_000_000 * seq as i64,
                "OrderSubmitted".to_string(),
                vec![seq as u8; 4],
            );
            writer.append(&entry).unwrap();
        }
        writer.sync().unwrap();
    }

    #[test]
    fn test_recovery_without_snapshot() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 50);

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (state, metrics) = engine.recover_without_validation(&applier).unwrap();

        assert_eq!(metrics.replay_count, 50);
        assert_eq!(metrics.final_sequence, 50);
        assert!(metrics.success);
        assert_eq!(metrics.snapshot_sequence, 0);
        assert!(!state.balances.is_empty());
    }

    #[test]
    fn test_recovery_with_snapshot() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        // Write journal with entries 1..100
        write_journal(&journal_dir, 1, 100);

        // Create a snapshot at sequence 50
        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;

        // First: recover first 50 entries to build state for snapshot
        let (state_at_50, _) = engine.recover_without_validation(&applier).unwrap();

        // Take snapshot at seq 50
        let snap_state = {
            let mut s = EngineState::empty();
            // Copy only first 50 replay entries
            for seq in 1..=50 {
                let key = format!("__replay_seq_{}", seq);
                if let Some(b) = state_at_50.balances.get(&key) {
                    s.balances.insert(key, b.clone());
                }
            }
            s
        };
        let writer = SnapshotWriter::new(&snap_dir, false);
        let snap = Snapshot::new(50, 50_000_000, snap_state, false);
        writer.write(&snap).unwrap();

        // Now recover from snapshot + remaining journal
        let mut engine2 = RecoveryEngine::new(&snap_dir, &journal_dir);
        let (final_state, metrics) = engine2.recover_without_validation(&applier).unwrap();

        assert_eq!(metrics.snapshot_sequence, 50);
        assert_eq!(metrics.replay_count, 50); // 51..100
        assert_eq!(metrics.final_sequence, 100);
        assert!(metrics.success);
        assert!(final_state.balances.len() >= 50);
    }

    #[test]
    fn test_recovery_abort_on_divergence() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 10);

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;

        // Pass wrong expected hash
        let result = engine.recover(&applier, Some("wrong_hash_value"));
        assert!(result.is_err());
        match result.unwrap_err() {
            RecoveryError::HashDivergence { expected, .. } => {
                assert_eq!(expected, "wrong_hash_value");
            }
            other => panic!("Expected HashDivergence, got: {:?}", other),
        }
    }

    #[test]
    fn test_recovery_metrics_populated() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 25);

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (_, metrics) = engine.recover_without_validation(&applier).unwrap();

        assert!(metrics.success);
        assert_eq!(metrics.replay_count, 25);
        assert_eq!(metrics.final_sequence, 25);
        assert!(!metrics.final_state_hash.is_empty());
        assert!(metrics.total_recovery_time_ms < 10000); // sanity bound
    }

    #[test]
    fn test_crash_simulation_partial_journal() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 20);

        // Simulate crash by truncating the journal file
        let files: Vec<_> = std::fs::read_dir(&journal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bin"))
            .collect();
        if let Some(f) = files.first() {
            let data = std::fs::read(f.path()).unwrap();
            // Truncate: keep only 80% of the data
            let truncated_len = (data.len() * 80) / 100;
            std::fs::write(f.path(), &data[..truncated_len]).unwrap();
        }

        // Recovery should still succeed with partial data
        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (_, metrics) = engine.recover_without_validation(&applier).unwrap();

        assert!(metrics.success);
        // Should have recovered fewer entries due to truncation
        assert!(metrics.replay_count < 20);
        assert!(metrics.replay_count > 0);
    }

    #[test]
    fn test_recovery_logging() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 5);

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        engine.recover_without_validation(&applier).unwrap();

        let log = engine.log();
        assert!(!log.is_empty());
        assert!(log.iter().any(|e| e.stage == RecoveryStage::Start));
        assert!(log.iter().any(|e| e.stage == RecoveryStage::Complete));
    }

    #[test]
    fn test_replay_contract_determinism() {
        let state_a = EngineState::empty();
        let state_b = EngineState::empty();
        assert!(ReplayContract::verify_determinism(&state_a, &state_b));
    }

    #[test]
    fn test_replay_contract_verify_recovery() {
        let state = EngineState::empty();
        let hash = state.compute_hash();
        assert!(ReplayContract::verify_recovery(&state, &hash));
        assert!(!ReplayContract::verify_recovery(&state, "wrong"));
    }

    #[test]
    fn test_cold_restart_empty() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");
        std::fs::create_dir_all(&journal_dir).unwrap();

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (state, metrics) = engine.recover_without_validation(&applier).unwrap();

        assert!(metrics.success);
        assert_eq!(metrics.replay_count, 0);
        assert_eq!(state, EngineState::empty());
    }

    #[test]
    fn test_take_snapshot_during_recovery() {
        let tmp = TempDir::new().unwrap();
        let snap_dir = tmp.path().join("snapshots");
        let journal_dir = tmp.path().join("journal");

        write_journal(&journal_dir, 1, 10);

        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (state, _) = engine.recover_without_validation(&applier).unwrap();

        // Take a snapshot
        let path = engine
            .take_snapshot(&state, 10, 10_000_000, false)
            .unwrap();
        assert!(path.exists());
    }
}
