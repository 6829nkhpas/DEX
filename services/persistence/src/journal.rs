//! Journal Writer — Append-only event journal with checksums
//!
//! Implements spec §10.8 (WAL), §10.8.1 (CRC32C checksums),
//! §14 (sequence numbering), §08 (event structure).
//!
//! # Binary Format (per entry)
//! ```text
//! [total_len: u32]
//! [sequence:  u64]
//! [timestamp: i64]
//! [event_type_len: u16][event_type: bytes]
//! [payload_len: u32][payload: bytes]
//! [checksum: u32]  // CRC32C over sequence+timestamp+event_type+payload
//! ```

use crc32c::crc32c;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum JournalError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Sequence error: expected {expected}, got {got}")]
    SequenceError { expected: u64, got: u64 },

    #[error("Journal size limit exceeded: {current} >= {limit}")]
    SizeLimitExceeded { current: u64, limit: u64 },

    #[error("File rotation required")]
    RotationRequired,
}

// ── Journal Entry ───────────────────────────────────────────────────

/// A single journal entry representing one persisted event.
///
/// Per spec §08: events have sequence, timestamp, event_type, payload.
/// Per spec §10.8.1: CRC32C checksum for integrity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Global monotonic sequence number (spec §14)
    pub sequence: u64,
    /// Unix nanosecond timestamp — exchange time, NOT wall clock (spec §12, §13)
    pub timestamp: i64,
    /// Event type string from taxonomy (spec §08)
    pub event_type: String,
    /// Bincode-serialized event payload
    pub payload: Vec<u8>,
    /// CRC32C checksum over (sequence ++ timestamp ++ event_type ++ payload)
    pub checksum: u32,
}

impl JournalEntry {
    /// Create a new entry, computing the CRC32C checksum automatically.
    pub fn new(sequence: u64, timestamp: i64, event_type: String, payload: Vec<u8>) -> Self {
        let checksum = Self::compute_checksum(sequence, timestamp, &event_type, &payload);
        Self {
            sequence,
            timestamp,
            event_type,
            payload,
            checksum,
        }
    }

    /// Compute CRC32C over the concatenation of (sequence, timestamp, event_type, payload).
    pub fn compute_checksum(
        sequence: u64,
        timestamp: i64,
        event_type: &str,
        payload: &[u8],
    ) -> u32 {
        let mut buf = Vec::with_capacity(8 + 8 + event_type.len() + payload.len());
        buf.extend_from_slice(&sequence.to_le_bytes());
        buf.extend_from_slice(&timestamp.to_le_bytes());
        buf.extend_from_slice(event_type.as_bytes());
        buf.extend_from_slice(payload);
        crc32c(&buf)
    }

    /// Validate the stored checksum against recomputed value.
    pub fn verify_checksum(&self) -> bool {
        let expected =
            Self::compute_checksum(self.sequence, self.timestamp, &self.event_type, &self.payload);
        self.checksum == expected
    }

    /// Serialize entry to the binary wire format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let event_type_bytes = self.event_type.as_bytes();
        let event_type_len = event_type_bytes.len() as u16;
        let payload_len = self.payload.len() as u32;

        // total_len = 8 (seq) + 8 (ts) + 2 (et_len) + et_bytes + 4 (pl_len) + pl_bytes + 4 (crc)
        let body_len: u32 =
            8 + 8 + 2 + (event_type_len as u32) + 4 + payload_len + 4;

        let mut buf = Vec::with_capacity(4 + body_len as usize);
        buf.extend_from_slice(&body_len.to_le_bytes());
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&event_type_len.to_le_bytes());
        buf.extend_from_slice(event_type_bytes);
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&self.payload);
        buf.extend_from_slice(&self.checksum.to_le_bytes());
        buf
    }

    /// Deserialize entry from the binary wire format.
    ///
    /// Returns `(entry, bytes_consumed)` on success.
    /// Handles corrupted data gracefully by returning errors instead of panicking.
    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize), JournalError> {
        if data.len() < 4 {
            return Err(JournalError::Serialization(
                "Not enough data for length prefix".into(),
            ));
        }

        let body_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        // Sanity check: reject absurdly large body_len (likely corruption)
        if body_len > 100_000_000 {
            return Err(JournalError::Serialization(format!(
                "Implausible body length: {} (likely corruption)",
                body_len
            )));
        }

        let total = 4 + body_len;

        if data.len() < total {
            return Err(JournalError::Serialization(format!(
                "Incomplete entry: need {} bytes, have {}",
                total,
                data.len()
            )));
        }

        // Minimum body size: 8 (seq) + 8 (ts) + 2 (et_len) + 0 (et) + 4 (pl_len) + 0 (pl) + 4 (crc) = 26
        if body_len < 26 {
            return Err(JournalError::Serialization(format!(
                "Body too small: {} bytes, minimum is 26",
                body_len
            )));
        }

        let body = &data[4..total];
        let mut pos: usize = 0;

        // sequence (u64)
        let sequence = u64::from_le_bytes(body[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // timestamp (i64)
        let timestamp = i64::from_le_bytes(body[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // event_type_len (u16) + event_type
        let event_type_len =
            u16::from_le_bytes(body[pos..pos + 2].try_into().unwrap()) as usize;
        pos += 2;

        if pos + event_type_len > body.len() {
            return Err(JournalError::Serialization(format!(
                "event_type_len {} exceeds remaining body ({} bytes)",
                event_type_len,
                body.len() - pos
            )));
        }
        let event_type = String::from_utf8(body[pos..pos + event_type_len].to_vec())
            .map_err(|e| JournalError::Serialization(e.to_string()))?;
        pos += event_type_len;

        // payload_len (u32) + payload
        if pos + 4 > body.len() {
            return Err(JournalError::Serialization(
                "Not enough data for payload length".into(),
            ));
        }
        let payload_len =
            u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if pos + payload_len > body.len() {
            return Err(JournalError::Serialization(format!(
                "payload_len {} exceeds remaining body ({} bytes)",
                payload_len,
                body.len() - pos
            )));
        }
        let payload = body[pos..pos + payload_len].to_vec();
        pos += payload_len;

        // checksum (u32)
        if pos + 4 > body.len() {
            return Err(JournalError::Serialization(
                "Not enough data for checksum".into(),
            ));
        }
        let checksum = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());

        let entry = Self {
            sequence,
            timestamp,
            event_type,
            payload,
            checksum,
        };

        Ok((entry, total))
    }
}

// ── Flush / Fsync Policies ──────────────────────────────────────────

/// Controls when buffered data is flushed to OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlushPolicy {
    /// Flush after every write.
    EveryWrite,
    /// Flush every N writes.
    EveryN(usize),
}

/// Controls when `fsync` (durable write) is called.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FsyncPolicy {
    /// Fsync after every write.
    EveryWrite,
    /// Fsync every N writes.
    EveryN(usize),
    /// Fsync only on file rotation.
    OnRotation,
}

// ── Journal Writer Configuration ────────────────────────────────────

/// Configuration for the journal writer.
#[derive(Debug, Clone)]
pub struct JournalConfig {
    /// Directory for journal files.
    pub dir: PathBuf,
    /// Maximum file size in bytes before rotation (default 64 MiB).
    pub max_file_size: u64,
    /// Maximum total journal size in bytes (0 = unlimited).
    pub max_total_size: u64,
    /// Flush policy.
    pub flush_policy: FlushPolicy,
    /// Fsync policy.
    pub fsync_policy: FsyncPolicy,
}

impl JournalConfig {
    /// Create a config with sensible defaults.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            max_file_size: 64 * 1024 * 1024, // 64 MiB
            max_total_size: 0,                // unlimited
            flush_policy: FlushPolicy::EveryWrite,
            fsync_policy: FsyncPolicy::EveryWrite,
        }
    }
}

// ── Journal Writer ──────────────────────────────────────────────────

/// Append-only journal writer with checksums, rotation, and fsync control.
pub struct JournalWriter {
    config: JournalConfig,
    writer: BufWriter<File>,
    current_file: PathBuf,
    current_file_size: u64,
    next_sequence: u64,
    writes_since_flush: usize,
    writes_since_fsync: usize,
    file_index: u64,
    total_size: u64,
}

impl JournalWriter {
    /// Open a new journal writer, creating the directory if needed.
    pub fn open(config: JournalConfig) -> Result<Self, JournalError> {
        fs::create_dir_all(&config.dir)?;

        // Scan existing journal files to find the latest index
        let file_index = Self::find_latest_index(&config.dir);
        let current_file = Self::journal_path(&config.dir, file_index);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current_file)?;

        let current_file_size = file.metadata()?.len();
        let total_size = Self::compute_total_size(&config.dir)?;

        Ok(Self {
            config,
            writer: BufWriter::new(file),
            current_file,
            current_file_size,
            next_sequence: 0, // Will be set by caller or via recovery
            writes_since_flush: 0,
            writes_since_fsync: 0,
            file_index,
            total_size,
        })
    }

    /// Set the next expected sequence number (used after recovery).
    pub fn set_next_sequence(&mut self, seq: u64) {
        self.next_sequence = seq;
    }

    /// Get the next expected sequence number.
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Get the current file path.
    pub fn current_file_path(&self) -> &Path {
        &self.current_file
    }

    /// Append a journal entry. Validates sequence monotonicity.
    pub fn append(&mut self, entry: &JournalEntry) -> Result<(), JournalError> {
        // Validate sequence ordering (spec §14.6)
        if self.next_sequence > 0 && entry.sequence != self.next_sequence {
            return Err(JournalError::SequenceError {
                expected: self.next_sequence,
                got: entry.sequence,
            });
        }

        // Check total size limit
        if self.config.max_total_size > 0 && self.total_size >= self.config.max_total_size {
            return Err(JournalError::SizeLimitExceeded {
                current: self.total_size,
                limit: self.config.max_total_size,
            });
        }

        // Check if rotation is needed
        if self.current_file_size >= self.config.max_file_size {
            self.rotate()?;
        }

        let bytes = entry.to_bytes();
        self.write_atomic(&bytes)?;

        let written = bytes.len() as u64;
        self.current_file_size += written;
        self.total_size += written;
        self.next_sequence = entry.sequence + 1;
        self.writes_since_flush += 1;
        self.writes_since_fsync += 1;

        self.apply_flush_policy()?;
        self.apply_fsync_policy()?;

        Ok(())
    }

    /// Create a new entry and append it in one call.
    pub fn write_event(
        &mut self,
        sequence: u64,
        timestamp: i64,
        event_type: String,
        payload: Vec<u8>,
    ) -> Result<JournalEntry, JournalError> {
        let entry = JournalEntry::new(sequence, timestamp, event_type, payload);
        self.append(&entry)?;
        Ok(entry)
    }

    /// Force flush + fsync (used before shutdown / rotation).
    pub fn sync(&mut self) -> Result<(), JournalError> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        self.writes_since_flush = 0;
        self.writes_since_fsync = 0;
        Ok(())
    }

    // ── Internal Helpers ────────────────────────────────────────────

    fn write_atomic(&mut self, data: &[u8]) -> Result<(), JournalError> {
        self.writer.write_all(data)?;
        Ok(())
    }

    fn apply_flush_policy(&mut self) -> Result<(), JournalError> {
        let should_flush = match self.config.flush_policy {
            FlushPolicy::EveryWrite => true,
            FlushPolicy::EveryN(n) => self.writes_since_flush >= n,
        };
        if should_flush {
            self.writer.flush()?;
            self.writes_since_flush = 0;
        }
        Ok(())
    }

    fn apply_fsync_policy(&mut self) -> Result<(), JournalError> {
        let should_fsync = match self.config.fsync_policy {
            FsyncPolicy::EveryWrite => true,
            FsyncPolicy::EveryN(n) => self.writes_since_fsync >= n,
            FsyncPolicy::OnRotation => false,
        };
        if should_fsync {
            self.writer.get_ref().sync_all()?;
            self.writes_since_fsync = 0;
        }
        Ok(())
    }

    fn rotate(&mut self) -> Result<(), JournalError> {
        // Fsync current file before rotating
        self.sync()?;

        self.file_index += 1;
        self.current_file = Self::journal_path(&self.config.dir, self.file_index);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current_file)?;

        self.writer = BufWriter::new(file);
        self.current_file_size = 0;
        Ok(())
    }

    fn journal_path(dir: &Path, index: u64) -> PathBuf {
        dir.join(format!("journal-{:06}.bin", index))
    }

    fn find_latest_index(dir: &Path) -> u64 {
        fs::read_dir(dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.starts_with("journal-") && name.ends_with(".bin") {
                            name.trim_start_matches("journal-")
                                .trim_end_matches(".bin")
                                .parse::<u64>()
                                .ok()
                        } else {
                            None
                        }
                    })
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    fn compute_total_size(dir: &Path) -> Result<u64, JournalError> {
        let mut total = 0u64;
        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    total += entry.metadata()?.len();
                }
            }
        }
        Ok(total)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(dir: &Path) -> JournalConfig {
        JournalConfig::new(dir)
    }

    fn sample_entry(seq: u64) -> JournalEntry {
        JournalEntry::new(
            seq,
            1_708_123_456_789_000_000 + (seq as i64),
            "OrderSubmitted".to_string(),
            vec![1, 2, 3, 4, 5],
        )
    }

    #[test]
    fn test_journal_entry_checksum_computation() {
        let entry = sample_entry(1);
        assert!(entry.verify_checksum());
    }

    #[test]
    fn test_journal_entry_checksum_detects_tamper() {
        let mut entry = sample_entry(1);
        entry.payload = vec![99, 98, 97]; // tamper payload
        assert!(!entry.verify_checksum());
    }

    #[test]
    fn test_journal_entry_serialization_roundtrip() {
        let entry = sample_entry(42);
        let bytes = entry.to_bytes();
        let (decoded, consumed) = JournalEntry::from_bytes(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(entry, decoded);
    }

    #[test]
    fn test_append_single_entry() {
        let tmp = TempDir::new().unwrap();
        let mut writer = JournalWriter::open(test_config(tmp.path())).unwrap();
        writer.set_next_sequence(1);

        let entry = sample_entry(1);
        writer.append(&entry).unwrap();

        assert_eq!(writer.next_sequence(), 2);
    }

    #[test]
    fn test_append_multiple_entries() {
        let tmp = TempDir::new().unwrap();
        let mut writer = JournalWriter::open(test_config(tmp.path())).unwrap();
        writer.set_next_sequence(1);

        for seq in 1..=100 {
            writer.append(&sample_entry(seq)).unwrap();
        }
        assert_eq!(writer.next_sequence(), 101);
    }

    #[test]
    fn test_sequence_error_on_gap() {
        let tmp = TempDir::new().unwrap();
        let mut writer = JournalWriter::open(test_config(tmp.path())).unwrap();
        writer.set_next_sequence(1);

        writer.append(&sample_entry(1)).unwrap();
        let result = writer.append(&sample_entry(5)); // gap: expected 2
        assert!(result.is_err());
        match result.unwrap_err() {
            JournalError::SequenceError { expected, got } => {
                assert_eq!(expected, 2);
                assert_eq!(got, 5);
            }
            other => panic!("Unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_write_event_convenience() {
        let tmp = TempDir::new().unwrap();
        let mut writer = JournalWriter::open(test_config(tmp.path())).unwrap();
        writer.set_next_sequence(1);

        let entry = writer
            .write_event(
                1,
                1_708_123_456_789_000_000,
                "TradeExecuted".to_string(),
                vec![10, 20, 30],
            )
            .unwrap();

        assert_eq!(entry.sequence, 1);
        assert!(entry.verify_checksum());
    }

    #[test]
    fn test_flush_policy_every_write() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            flush_policy: FlushPolicy::EveryWrite,
            ..test_config(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);

        // After each write the file should have data on disk
        writer.append(&sample_entry(1)).unwrap();
        let size = fs::metadata(writer.current_file_path()).unwrap().len();
        assert!(size > 0);
    }

    #[test]
    fn test_file_rotation_on_size_limit() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            max_file_size: 100, // Very small limit to trigger rotation quickly
            ..test_config(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);

        // Write entries until rotation happens
        for seq in 1..=20 {
            writer.append(&sample_entry(seq)).unwrap();
        }

        // Should have rotated at least once
        let files: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("journal-")
            })
            .collect();
        assert!(files.len() > 1, "Expected rotation to create multiple files");
    }

    #[test]
    fn test_journal_size_limit() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            max_total_size: 200, // Very small total limit
            max_file_size: 64 * 1024 * 1024,
            ..test_config(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);

        // Write until we hit the size limit
        let mut hit_limit = false;
        for seq in 1..=1000 {
            match writer.append(&sample_entry(seq)) {
                Ok(_) => {}
                Err(JournalError::SizeLimitExceeded { .. }) => {
                    hit_limit = true;
                    break;
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
        assert!(hit_limit, "Expected size limit to be hit");
    }

    #[test]
    fn test_timestamp_per_entry() {
        let entry1 = JournalEntry::new(1, 1_000_000_000, "A".into(), vec![]);
        let entry2 = JournalEntry::new(2, 2_000_000_000, "B".into(), vec![]);
        assert_ne!(entry1.timestamp, entry2.timestamp);
        assert_eq!(entry1.timestamp, 1_000_000_000);
        assert_eq!(entry2.timestamp, 2_000_000_000);
    }

    #[test]
    fn test_checksum_differs_for_different_payloads() {
        let e1 = JournalEntry::new(1, 100, "X".into(), vec![1]);
        let e2 = JournalEntry::new(1, 100, "X".into(), vec![2]);
        assert_ne!(e1.checksum, e2.checksum);
    }

    #[test]
    fn test_sync_flushes_to_disk() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            flush_policy: FlushPolicy::EveryN(1000), // Don't auto-flush
            fsync_policy: FsyncPolicy::OnRotation,   // Don't auto-fsync
            ..test_config(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);

        writer.append(&sample_entry(1)).unwrap();
        writer.sync().unwrap();

        let size = fs::metadata(writer.current_file_path()).unwrap().len();
        assert!(size > 0);
    }

    #[test]
    fn test_journal_file_naming() {
        let path = JournalWriter::journal_path(Path::new("/tmp"), 42);
        assert_eq!(path, PathBuf::from("/tmp/journal-000042.bin"));
    }

    #[test]
    fn test_fsync_policy_every_n() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            fsync_policy: FsyncPolicy::EveryN(5),
            ..test_config(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);

        // Write 10 entries — fsync should trigger at 5 and 10
        for seq in 1..=10 {
            writer.append(&sample_entry(seq)).unwrap();
        }
        assert_eq!(writer.next_sequence(), 11);
    }
}
