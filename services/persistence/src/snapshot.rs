//! Snapshot System — State snapshots with integrity and compression
//!
//! Implements spec §11.6 (snapshots), §10.8.1 (checksums),
//! §12 (determinism: BTreeMap for sorted iteration).
//!
//! Features:
//! - Full engine state serialization (accounts, orders, positions, balances)
//! - BTreeMap-based state for deterministic serialization (spec §12.3.5)
//! - SHA-256 integrity hash over serialized state
//! - Optional zstd compression (spec §11.8.3)
//! - Snapshot versioning for forward compatibility
//! - Interval policy (every N events or time-based)
//! - Cleanup policy (keep last N snapshots)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum SnapshotError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Integrity check failed: expected {expected}, got {actual}")]
    IntegrityFailure { expected: String, actual: String },

    #[error("Unsupported snapshot version: {0}")]
    UnsupportedVersion(u32),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("No snapshots found")]
    NoSnapshots,
}

use std::io;

// ── Engine State ────────────────────────────────────────────────────

/// Full engine state for snapshot serialization.
///
/// Uses `BTreeMap` for deterministic iteration order (spec §12.3.5).
/// All fields are serializable and deserializable with bincode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineState {
    /// Account states keyed by account ID string.
    pub accounts: BTreeMap<String, AccountSnapshot>,
    /// Active orders keyed by order ID string.
    pub orders: BTreeMap<String, OrderSnapshot>,
    /// Open positions keyed by position ID string.
    pub positions: BTreeMap<String, PositionSnapshot>,
    /// Balance records keyed by "account_id:asset".
    pub balances: BTreeMap<String, BalanceSnapshot>,
}

impl EngineState {
    /// Create a new empty engine state.
    pub fn empty() -> Self {
        Self {
            accounts: BTreeMap::new(),
            orders: BTreeMap::new(),
            positions: BTreeMap::new(),
            balances: BTreeMap::new(),
        }
    }

    /// Compute a deterministic SHA-256 hash of the state.
    pub fn compute_hash(&self) -> String {
        let bytes = bincode::serialize(self)
            .expect("EngineState serialization should never fail");
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    }
}

/// Minimal account snapshot for persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub account_type: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: u64,
}

/// Minimal order snapshot for persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderSnapshot {
    pub order_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub price: String,
    pub quantity: String,
    pub filled_quantity: String,
    pub remaining_quantity: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Minimal position snapshot for persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub position_id: String,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub size: String,
    pub entry_price: String,
    pub unrealized_pnl: String,
}

/// Minimal balance snapshot for persistence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceSnapshot {
    pub account_id: String,
    pub asset: String,
    pub total: String,
    pub available: String,
    pub locked: String,
}

// ── Snapshot ────────────────────────────────────────────────────────

/// Current snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 1;

/// A complete snapshot of the engine state at a given sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version for forward compatibility.
    pub version: u32,
    /// Last applied event sequence number.
    pub sequence: u64,
    /// Unix nanosecond timestamp when snapshot was taken.
    pub timestamp: i64,
    /// Full engine state.
    pub state: EngineState,
    /// SHA-256 hash of the serialized state.
    pub checksum: String,
    /// Whether the data on disk is zstd-compressed.
    pub compressed: bool,
}

impl Snapshot {
    /// Create a new snapshot with computed integrity hash.
    pub fn new(sequence: u64, timestamp: i64, state: EngineState, compressed: bool) -> Self {
        let checksum = state.compute_hash();
        Self {
            version: SNAPSHOT_VERSION,
            sequence,
            timestamp,
            state,
            checksum,
            compressed,
        }
    }

    /// Verify the snapshot's integrity hash.
    pub fn verify_integrity(&self) -> bool {
        let computed = self.state.compute_hash();
        self.checksum == computed
    }
}

// ── Snapshot Writer ─────────────────────────────────────────────────

/// Writes snapshots to disk with optional zstd compression.
pub struct SnapshotWriter {
    dir: PathBuf,
    compress: bool,
}

impl SnapshotWriter {
    /// Create a new writer. `compress` enables zstd compression.
    pub fn new(dir: impl Into<PathBuf>, compress: bool) -> Self {
        Self {
            dir: dir.into(),
            compress,
        }
    }

    /// Write a snapshot atomically: serialize → compress → compute hash → write.
    pub fn write(&self, snapshot: &Snapshot) -> Result<PathBuf, SnapshotError> {
        fs::create_dir_all(&self.dir)?;

        let data = bincode::serialize(snapshot)
            .map_err(|e| SnapshotError::Serialization(e.to_string()))?;

        let (final_data, ext) = if self.compress {
            let compressed = zstd::encode_all(data.as_slice(), 3)
                .map_err(|e| SnapshotError::Compression(e.to_string()))?;
            (compressed, "snap.zst")
        } else {
            (data, "snap")
        };

        let filename = format!("snapshot-{:012}.{}", snapshot.sequence, ext);
        let path = self.dir.join(&filename);
        let tmp_path = self.dir.join(format!("{}.tmp", filename));

        // Atomic write: write to tmp, fsync, rename
        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(&final_data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &path)?;

        Ok(path)
    }
}

// ── Snapshot Loader ─────────────────────────────────────────────────

/// Loads snapshots from disk, verifying integrity.
pub struct SnapshotLoader {
    dir: PathBuf,
}

impl SnapshotLoader {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Load a specific snapshot file.
    pub fn load(&self, path: &Path) -> Result<Snapshot, SnapshotError> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        let is_compressed = path
            .extension()
            .map(|e| e == "zst")
            .unwrap_or(false);

        let decompressed = if is_compressed {
            zstd::decode_all(data.as_slice())
                .map_err(|e| SnapshotError::Compression(e.to_string()))?
        } else {
            data
        };

        let snapshot: Snapshot = bincode::deserialize(&decompressed)
            .map_err(|e| SnapshotError::Serialization(e.to_string()))?;

        // Verify version
        if snapshot.version > SNAPSHOT_VERSION {
            return Err(SnapshotError::UnsupportedVersion(snapshot.version));
        }

        // Verify integrity
        if !snapshot.verify_integrity() {
            let actual = snapshot.state.compute_hash();
            return Err(SnapshotError::IntegrityFailure {
                expected: snapshot.checksum.clone(),
                actual,
            });
        }

        Ok(snapshot)
    }

    /// Load the latest snapshot (highest sequence number).
    pub fn load_latest(&self) -> Result<Snapshot, SnapshotError> {
        let path = self.find_latest()?;
        self.load(&path)
    }

    /// Find the path to the latest snapshot.
    pub fn find_latest(&self) -> Result<PathBuf, SnapshotError> {
        let mut snapshots = self.list_snapshots()?;
        snapshots.sort_by(|a, b| b.0.cmp(&a.0)); // Descending by sequence
        snapshots
            .into_iter()
            .next()
            .map(|(_, path)| path)
            .ok_or(SnapshotError::NoSnapshots)
    }

    /// List all snapshots as (sequence, path) pairs.
    pub fn list_snapshots(&self) -> Result<Vec<(u64, PathBuf)>, SnapshotError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("snapshot-") && (name.ends_with(".snap") || name.ends_with(".snap.zst"))
            {
                if let Some(seq) = Self::parse_sequence(&name) {
                    results.push((seq, entry.path()));
                }
            }
        }
        results.sort_by_key(|(seq, _)| *seq);
        Ok(results)
    }

    fn parse_sequence(filename: &str) -> Option<u64> {
        let stripped = filename
            .trim_start_matches("snapshot-")
            .trim_end_matches(".snap.zst")
            .trim_end_matches(".snap");
        stripped.parse::<u64>().ok()
    }
}

// ── Snapshot Interval Policy ────────────────────────────────────────

/// Policy that determines when to create a new snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotIntervalPolicy {
    /// Create snapshot every N events (spec §11.6.2: default 1,000,000).
    pub event_interval: u64,
    /// Last sequence at which a snapshot was taken.
    pub last_snapshot_seq: u64,
}

impl SnapshotIntervalPolicy {
    /// Create with default interval (1,000,000 per spec §11.6.2).
    pub fn default_policy() -> Self {
        Self {
            event_interval: 1_000_000,
            last_snapshot_seq: 0,
        }
    }

    /// Create with custom interval.
    pub fn with_interval(interval: u64) -> Self {
        Self {
            event_interval: interval,
            last_snapshot_seq: 0,
        }
    }

    /// Check if a snapshot should be taken at the given sequence.
    pub fn should_snapshot(&self, current_seq: u64) -> bool {
        current_seq >= self.last_snapshot_seq + self.event_interval
    }

    /// Record that a snapshot was taken at the given sequence.
    pub fn record_snapshot(&mut self, seq: u64) {
        self.last_snapshot_seq = seq;
    }
}

// ── Snapshot Cleanup Policy ─────────────────────────────────────────

/// Policy for cleaning up old snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotCleanupPolicy {
    /// Maximum number of snapshots to retain.
    pub max_snapshots: usize,
}

impl SnapshotCleanupPolicy {
    pub fn new(max_snapshots: usize) -> Self {
        Self { max_snapshots }
    }

    /// Remove old snapshots, keeping only the most recent `max_snapshots`.
    pub fn cleanup(&self, dir: &Path) -> Result<Vec<PathBuf>, SnapshotError> {
        let loader = SnapshotLoader::new(dir);
        let mut snapshots = loader.list_snapshots()?;
        snapshots.sort_by_key(|(seq, _)| *seq);

        let mut removed = Vec::new();
        if snapshots.len() > self.max_snapshots {
            let to_remove = snapshots.len() - self.max_snapshots;
            for (_, path) in snapshots.iter().take(to_remove) {
                fs::remove_file(path)?;
                removed.push(path.clone());
            }
        }
        Ok(removed)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_state() -> EngineState {
        let mut state = EngineState::empty();
        state.accounts.insert(
            "acc-001".to_string(),
            AccountSnapshot {
                account_id: "acc-001".to_string(),
                account_type: "MARGIN".to_string(),
                status: "ACTIVE".to_string(),
                created_at: 1_000_000,
                updated_at: 2_000_000,
                version: 5,
            },
        );
        state.balances.insert(
            "acc-001:USDT".to_string(),
            BalanceSnapshot {
                account_id: "acc-001".to_string(),
                asset: "USDT".to_string(),
                total: "10000.00".to_string(),
                available: "7000.00".to_string(),
                locked: "3000.00".to_string(),
            },
        );
        state.orders.insert(
            "ord-001".to_string(),
            OrderSnapshot {
                order_id: "ord-001".to_string(),
                account_id: "acc-001".to_string(),
                symbol: "BTC/USDT".to_string(),
                side: "BUY".to_string(),
                price: "50000.00".to_string(),
                quantity: "1.0".to_string(),
                filled_quantity: "0.3".to_string(),
                remaining_quantity: "0.7".to_string(),
                status: "PARTIAL".to_string(),
                created_at: 1_500_000,
                updated_at: 1_700_000,
            },
        );
        state
    }

    #[test]
    fn test_snapshot_write_and_load_uncompressed() {
        let tmp = TempDir::new().unwrap();
        let state = sample_state();
        let snapshot = Snapshot::new(5_000_000, 1_708_123_456_789_000_000, state.clone(), false);

        let writer = SnapshotWriter::new(tmp.path(), false);
        let path = writer.write(&snapshot).unwrap();

        let loader = SnapshotLoader::new(tmp.path());
        let loaded = loader.load(&path).unwrap();

        assert_eq!(loaded.version, SNAPSHOT_VERSION);
        assert_eq!(loaded.sequence, 5_000_000);
        assert_eq!(loaded.state, state);
        assert!(loaded.verify_integrity());
    }

    #[test]
    fn test_snapshot_write_and_load_compressed() {
        let tmp = TempDir::new().unwrap();
        let state = sample_state();
        let snapshot = Snapshot::new(5_000_000, 1_708_123_456_789_000_000, state.clone(), true);

        let writer = SnapshotWriter::new(tmp.path(), true);
        let path = writer.write(&snapshot).unwrap();

        assert!(path.to_string_lossy().ends_with(".snap.zst"));

        let loader = SnapshotLoader::new(tmp.path());
        let loaded = loader.load(&path).unwrap();

        assert_eq!(loaded.state, state);
        assert!(loaded.verify_integrity());
    }

    #[test]
    fn test_snapshot_integrity_hash() {
        let state = sample_state();
        let hash1 = state.compute_hash();
        let hash2 = state.compute_hash();
        assert_eq!(hash1, hash2, "Hash must be deterministic");
        assert_eq!(hash1.len(), 64, "SHA-256 hex digest is 64 chars");
    }

    #[test]
    fn test_snapshot_integrity_detects_tamper() {
        let state = sample_state();
        let mut snapshot = Snapshot::new(100, 1000, state, false);
        // Tamper with state after creation
        snapshot.state.accounts.insert(
            "acc-hacked".to_string(),
            AccountSnapshot {
                account_id: "hacked".to_string(),
                account_type: "SPOT".to_string(),
                status: "ACTIVE".to_string(),
                created_at: 0,
                updated_at: 0,
                version: 0,
            },
        );
        assert!(!snapshot.verify_integrity());
    }

    #[test]
    fn test_snapshot_versioning() {
        let state = sample_state();
        let snapshot = Snapshot::new(100, 1000, state, false);
        assert_eq!(snapshot.version, SNAPSHOT_VERSION);
    }

    #[test]
    fn test_snapshot_interval_policy() {
        let mut policy = SnapshotIntervalPolicy::with_interval(100);
        assert!(!policy.should_snapshot(50));
        assert!(policy.should_snapshot(100));
        assert!(policy.should_snapshot(200));

        policy.record_snapshot(100);
        assert!(!policy.should_snapshot(150));
        assert!(policy.should_snapshot(200));
    }

    #[test]
    fn test_snapshot_interval_default() {
        let policy = SnapshotIntervalPolicy::default_policy();
        assert_eq!(policy.event_interval, 1_000_000);
    }

    #[test]
    fn test_snapshot_cleanup_policy() {
        let tmp = TempDir::new().unwrap();
        let writer = SnapshotWriter::new(tmp.path(), false);

        // Write 5 snapshots
        for i in 1..=5 {
            let state = EngineState::empty();
            let snap = Snapshot::new(i * 1000, i as i64 * 1_000_000, state, false);
            writer.write(&snap).unwrap();
        }

        let cleanup = SnapshotCleanupPolicy::new(2);
        let removed = cleanup.cleanup(tmp.path()).unwrap();
        assert_eq!(removed.len(), 3, "Should remove 3 of 5 snapshots");

        let loader = SnapshotLoader::new(tmp.path());
        let remaining = loader.list_snapshots().unwrap();
        assert_eq!(remaining.len(), 2);
        // Kept the two with highest sequences
        assert_eq!(remaining[0].0, 4000);
        assert_eq!(remaining[1].0, 5000);
    }

    #[test]
    fn test_load_latest_snapshot() {
        let tmp = TempDir::new().unwrap();
        let writer = SnapshotWriter::new(tmp.path(), false);

        for i in [100u64, 500, 300] {
            let state = EngineState::empty();
            let snap = Snapshot::new(i, i as i64, state, false);
            writer.write(&snap).unwrap();
        }

        let loader = SnapshotLoader::new(tmp.path());
        let latest = loader.load_latest().unwrap();
        assert_eq!(latest.sequence, 500);
    }

    #[test]
    fn test_no_snapshots_returns_error() {
        let tmp = TempDir::new().unwrap();
        let loader = SnapshotLoader::new(tmp.path());
        assert!(matches!(
            loader.load_latest(),
            Err(SnapshotError::NoSnapshots)
        ));
    }

    #[test]
    fn test_engine_state_deterministic_hash() {
        // Same data inserted in different order should produce same hash
        // because BTreeMap sorts by key
        let mut s1 = EngineState::empty();
        s1.balances.insert("a:X".into(), BalanceSnapshot {
            account_id: "a".into(), asset: "X".into(),
            total: "100".into(), available: "80".into(), locked: "20".into(),
        });
        s1.balances.insert("b:Y".into(), BalanceSnapshot {
            account_id: "b".into(), asset: "Y".into(),
            total: "200".into(), available: "200".into(), locked: "0".into(),
        });

        let mut s2 = EngineState::empty();
        // Insert in reverse order
        s2.balances.insert("b:Y".into(), BalanceSnapshot {
            account_id: "b".into(), asset: "Y".into(),
            total: "200".into(), available: "200".into(), locked: "0".into(),
        });
        s2.balances.insert("a:X".into(), BalanceSnapshot {
            account_id: "a".into(), asset: "X".into(),
            total: "100".into(), available: "80".into(), locked: "20".into(),
        });

        assert_eq!(s1.compute_hash(), s2.compute_hash());
    }
}
