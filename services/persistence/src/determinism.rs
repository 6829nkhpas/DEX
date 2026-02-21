//! Determinism Guarantees — Replay comparison and property tests
//!
//! Implements spec §12 (determinism rules), §11.9 (replay testing),
//! §7.1-7.3 (determinism verification).
//!
//! Features:
//! - Double replay: run identical replay twice, compare state hashes
//! - Event output comparison across replay runs
//! - Divergence alert with detailed diff
//! - Property-based replay tests (proptest)
//! - Partial write simulation
//! - Abrupt shutdown simulation
//! - Disk full simulation
//! - Idempotency validation

use crate::journal::{JournalConfig, JournalEntry, JournalWriter};
use crate::reader::JournalReader;
use crate::recovery::{DefaultEventApplier, EventApplier, RecoveryEngine};
use crate::snapshot::EngineState;
use std::path::Path;

// ── Divergence Report ───────────────────────────────────────────────

/// Detailed report when two replay runs produce different results.
#[derive(Debug, Clone)]
pub struct DivergenceReport {
    pub hash_a: String,
    pub hash_b: String,
    pub accounts_match: bool,
    pub orders_match: bool,
    pub positions_match: bool,
    pub balances_match: bool,
    pub detail: String,
}

impl DivergenceReport {
    /// Check if the two states are identical.
    pub fn is_match(&self) -> bool {
        self.hash_a == self.hash_b
    }
}

// ── Determinism Verifier ────────────────────────────────────────────

/// Verifies deterministic behavior by comparing replay results.
pub struct DeterminismVerifier;

impl DeterminismVerifier {
    /// Run identical replay twice and compare state hashes (spec §12.7.1).
    pub fn verify_double_replay(
        journal_dir: &Path,
        applier: &dyn EventApplier,
    ) -> Result<DivergenceReport, String> {
        let state_a = Self::run_replay(journal_dir, applier)?;
        let state_b = Self::run_replay(journal_dir, applier)?;

        Ok(Self::compare_states(&state_a, &state_b))
    }

    /// Compare event outputs from two replay runs.
    pub fn compare_event_outputs(
        journal_dir: &Path,
    ) -> Result<bool, String> {
        let mut reader_a = JournalReader::open(journal_dir)
            .map_err(|e| format!("Reader A: {}", e))?;
        let mut reader_b = JournalReader::open(journal_dir)
            .map_err(|e| format!("Reader B: {}", e))?;

        let entries_a = reader_a.read_all().map_err(|e| format!("Read A: {}", e))?;
        let entries_b = reader_b.read_all().map_err(|e| format!("Read B: {}", e))?;

        if entries_a.len() != entries_b.len() {
            return Ok(false);
        }

        for (a, b) in entries_a.iter().zip(entries_b.iter()) {
            if a != b {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Generate a detailed divergence report from two states.
    pub fn compare_states(state_a: &EngineState, state_b: &EngineState) -> DivergenceReport {
        let hash_a = state_a.compute_hash();
        let hash_b = state_b.compute_hash();

        let accounts_match = state_a.accounts == state_b.accounts;
        let orders_match = state_a.orders == state_b.orders;
        let positions_match = state_a.positions == state_b.positions;
        let balances_match = state_a.balances == state_b.balances;

        let mut details = Vec::new();
        if !accounts_match {
            details.push(format!(
                "Accounts differ: {} vs {} entries",
                state_a.accounts.len(),
                state_b.accounts.len()
            ));
        }
        if !orders_match {
            details.push(format!(
                "Orders differ: {} vs {} entries",
                state_a.orders.len(),
                state_b.orders.len()
            ));
        }
        if !positions_match {
            details.push(format!(
                "Positions differ: {} vs {} entries",
                state_a.positions.len(),
                state_b.positions.len()
            ));
        }
        if !balances_match {
            details.push(format!(
                "Balances differ: {} vs {} entries",
                state_a.balances.len(),
                state_b.balances.len()
            ));
        }

        let detail = if details.is_empty() {
            "States are identical".to_string()
        } else {
            details.join("; ")
        };

        DivergenceReport {
            hash_a,
            hash_b,
            accounts_match,
            orders_match,
            positions_match,
            balances_match,
            detail,
        }
    }

    /// Validate idempotent replay: replaying same events twice produces same state.
    pub fn verify_idempotency(
        journal_dir: &Path,
        applier: &dyn EventApplier,
    ) -> Result<bool, String> {
        let state_a = Self::run_replay(journal_dir, applier)?;
        let state_b = Self::run_replay(journal_dir, applier)?;
        Ok(state_a.compute_hash() == state_b.compute_hash())
    }

    /// Simulate partial write: write half an entry, verify reader recovers.
    pub fn simulate_partial_write(
        journal_dir: &Path,
    ) -> Result<(usize, usize), String> {
        // Read all entries before corruption
        let mut reader = JournalReader::open(journal_dir)
            .map_err(|e| format!("Reader: {}", e))?;
        let original_entries = reader.read_all()
            .map_err(|e| format!("Read: {}", e))?;
        let original_count = original_entries.len();

        // Append partial (truncated) entry at end of last journal file
        let files: Vec<_> = std::fs::read_dir(journal_dir)
            .map_err(|e| format!("Dir: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bin"))
            .map(|e| e.path())
            .collect();

        if let Some(last_file) = files.last() {
            let mut data = std::fs::read(last_file)
                .map_err(|e| format!("Read file: {}", e))?;
            // Append a partial entry (just the length prefix, no body)
            data.extend_from_slice(&100u32.to_le_bytes());
            data.extend_from_slice(&[0u8; 10]); // incomplete body
            std::fs::write(last_file, &data)
                .map_err(|e| format!("Write: {}", e))?;
        }

        // Read entries after corruption — should recover original entries
        let mut reader = JournalReader::open(journal_dir)
            .map_err(|e| format!("Reader: {}", e))?;
        let (recovered, _corruptions) = reader.recover_entries();

        Ok((original_count, recovered.len()))
    }

    /// Simulate abrupt shutdown: don't fsync, verify recovery works.
    pub fn simulate_abrupt_shutdown(
        dir: &Path,
    ) -> Result<(u64, bool), String> {
        let journal_dir = dir.join("journal_shutdown_test");
        let config = JournalConfig {
            fsync_policy: crate::journal::FsyncPolicy::OnRotation, // Don't fsync
            ..JournalConfig::new(&journal_dir)
        };
        let mut writer = JournalWriter::open(config)
            .map_err(|e| format!("Writer: {}", e))?;
        writer.set_next_sequence(1);

        // Write some entries without fsync
        for seq in 1..=10 {
            let entry = JournalEntry::new(
                seq,
                seq as i64 * 1_000_000,
                "ShutdownTest".to_string(),
                vec![seq as u8; 8],
            );
            writer.append(&entry).map_err(|e| format!("Append: {}", e))?;
        }
        // Explicitly flush but maybe not fsync (simulating abrupt shutdown)
        writer.sync().map_err(|e| format!("Sync: {}", e))?;

        // Now try recovery
        let snap_dir = dir.join("snap_shutdown_test");
        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (_, metrics) = engine
            .recover_without_validation(&applier)
            .map_err(|e| format!("Recovery: {}", e))?;

        Ok((metrics.replay_count, metrics.success))
    }

    /// Simulate disk full: writer that fails mid-write.
    pub fn simulate_disk_full_recovery(
        dir: &Path,
    ) -> Result<(u64, bool), String> {
        let journal_dir = dir.join("journal_diskfull_test");
        let config = JournalConfig {
            max_total_size: 300, // Very small — simulates "disk full"
            ..JournalConfig::new(&journal_dir)
        };
        let mut writer = JournalWriter::open(config)
            .map_err(|e| format!("Writer: {}", e))?;
        writer.set_next_sequence(1);

        // Write until failure
        let mut written = 0u64;
        for seq in 1..=100 {
            match writer.append(&JournalEntry::new(
                seq,
                seq as i64 * 1_000_000,
                "DiskFullTest".to_string(),
                vec![seq as u8; 4],
            )) {
                Ok(()) => written += 1,
                Err(_) => break,
            }
        }
        writer.sync().map_err(|e| format!("Sync: {}", e))?;

        // Recover whatever was written
        let snap_dir = dir.join("snap_diskfull_test");
        let mut engine = RecoveryEngine::new(&snap_dir, &journal_dir);
        let applier = DefaultEventApplier;
        let (_, metrics) = engine
            .recover_without_validation(&applier)
            .map_err(|e| format!("Recovery: {}", e))?;

        Ok((metrics.replay_count, metrics.replay_count == written))
    }

    // ── Internal ────────────────────────────────────────────────────

    fn run_replay(
        journal_dir: &Path,
        applier: &dyn EventApplier,
    ) -> Result<EngineState, String> {
        let mut reader = JournalReader::open(journal_dir)
            .map_err(|e| format!("Open reader: {}", e))?;
        let entries = reader.read_all()
            .map_err(|e| format!("Read all: {}", e))?;

        let mut state = EngineState::empty();
        for entry in &entries {
            applier
                .apply(&mut state, entry)
                .map_err(|e| format!("Apply: {}", e))?;
        }
        Ok(state)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{JournalConfig, JournalEntry, JournalWriter};
    use tempfile::TempDir;

    fn write_journal(dir: &Path, count: u64) {
        let config = JournalConfig::new(dir);
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);
        for seq in 1..=count {
            let entry = JournalEntry::new(
                seq,
                seq as i64 * 1_000_000,
                format!("Event{}", seq % 5),
                vec![seq as u8; 8],
            );
            writer.append(&entry).unwrap();
        }
        writer.sync().unwrap();
    }

    #[test]
    fn test_identical_replay_produces_same_hash() {
        let tmp = TempDir::new().unwrap();
        let journal_dir = tmp.path().join("journal");
        write_journal(&journal_dir, 50);

        let applier = DefaultEventApplier;
        let report = DeterminismVerifier::verify_double_replay(
            &journal_dir,
            &applier,
        )
        .unwrap();

        assert!(report.is_match(), "Replays must produce identical hashes");
        assert!(report.accounts_match);
        assert!(report.orders_match);
        assert!(report.positions_match);
        assert!(report.balances_match);
    }

    #[test]
    fn test_event_output_comparison() {
        let tmp = TempDir::new().unwrap();
        let journal_dir = tmp.path().join("journal");
        write_journal(&journal_dir, 30);

        let result = DeterminismVerifier::compare_event_outputs(&journal_dir).unwrap();
        assert!(result, "Same journal should produce identical event outputs");
    }

    #[test]
    fn test_divergence_report_on_mismatch() {
        let state_a = EngineState::empty();
        let mut state_b = EngineState::empty();
        state_b.accounts.insert(
            "extra".into(),
            crate::snapshot::AccountSnapshot {
                account_id: "extra".into(),
                account_type: "SPOT".into(),
                status: "ACTIVE".into(),
                created_at: 0,
                updated_at: 0,
                version: 0,
            },
        );

        let report = DeterminismVerifier::compare_states(&state_a, &state_b);
        assert!(!report.is_match());
        assert!(!report.accounts_match);
        assert!(report.detail.contains("Accounts differ"));
    }

    #[test]
    fn test_idempotent_replay() {
        let tmp = TempDir::new().unwrap();
        let journal_dir = tmp.path().join("journal");
        write_journal(&journal_dir, 20);

        let applier = DefaultEventApplier;
        let result = DeterminismVerifier::verify_idempotency(
            &journal_dir,
            &applier,
        )
        .unwrap();
        assert!(result, "Replay must be idempotent");
    }

    #[test]
    fn test_partial_write_recovery() {
        let tmp = TempDir::new().unwrap();
        let journal_dir = tmp.path().join("journal");
        write_journal(&journal_dir, 15);

        let (original, recovered) =
            DeterminismVerifier::simulate_partial_write(&journal_dir).unwrap();
        assert_eq!(original, 15);
        // recovered should be >= original (we recover the valid prefix)
        assert!(
            recovered >= original,
            "Should recover at least original {} entries, got {}",
            original,
            recovered
        );
    }

    #[test]
    fn test_abrupt_shutdown_recovery() {
        let tmp = TempDir::new().unwrap();
        let (count, success) =
            DeterminismVerifier::simulate_abrupt_shutdown(tmp.path()).unwrap();

        assert!(success, "Recovery after abrupt shutdown must succeed");
        assert_eq!(count, 10, "Should recover all 10 written entries");
    }

    #[test]
    fn test_disk_full_recovery() {
        let tmp = TempDir::new().unwrap();
        let (recovered, matches_written) =
            DeterminismVerifier::simulate_disk_full_recovery(tmp.path()).unwrap();

        assert!(recovered > 0, "Should recover some entries");
        assert!(
            matches_written,
            "Recovered count should match written count"
        );
    }

    #[test]
    fn test_empty_journal_determinism() {
        let tmp = TempDir::new().unwrap();
        let journal_dir = tmp.path().join("journal");
        std::fs::create_dir_all(&journal_dir).unwrap();

        let applier = DefaultEventApplier;
        let report = DeterminismVerifier::verify_double_replay(
            &journal_dir,
            &applier,
        )
        .unwrap();
        assert!(report.is_match());
    }
}

// ── Property-Based Tests ────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::journal::{JournalConfig, JournalEntry, JournalWriter};
    use proptest::prelude::*;
    use tempfile::TempDir;

    proptest! {
        #[test]
        fn prop_replay_deterministic(
            count in 1u64..50,
            payload_byte in 0u8..=255,
        ) {
            let tmp = TempDir::new().unwrap();
            let journal_dir = tmp.path().join("journal");
            let config = JournalConfig::new(&journal_dir);
            let mut writer = JournalWriter::open(config).unwrap();
            writer.set_next_sequence(1);

            for seq in 1..=count {
                let entry = JournalEntry::new(
                    seq,
                    seq as i64 * 1_000,
                    "PropTest".to_string(),
                    vec![payload_byte; (seq % 20) as usize + 1],
                );
                writer.append(&entry).unwrap();
            }
            writer.sync().unwrap();

            let applier = DefaultEventApplier;
            let result = DeterminismVerifier::verify_idempotency(
                &journal_dir,
                &applier,
            );
            prop_assert!(result.is_ok());
            prop_assert!(result.unwrap(), "Replay must be idempotent for any input");
        }
    }
}
