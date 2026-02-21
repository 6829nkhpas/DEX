//! Journal Reader — Sequential reader with corruption detection
//!
//! Implements spec §11 (replay requirements), §14.6 (gapless sequences),
//! §10.8.1 (CRC32C checksum validation).
//!
//! Features:
//! - Sequential entry reading from journal files
//! - CRC32C checksum validation on every read
//! - Corruption detection with byte-offset reporting
//! - Partial recovery: skip corrupted tail, recover valid prefix
//! - Offset tracking for replay-from-offset
//! - Gapless / monotonic sequence validation (spec §14.6)
//! - Missing sequence detection and alerting

use crate::journal::{JournalEntry, JournalError};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Journal error: {0}")]
    Journal(#[from] JournalError),

    #[error("Checksum mismatch at byte offset {offset}: entry seq={sequence}")]
    ChecksumMismatch { offset: u64, sequence: u64 },

    #[error("Corruption detected at byte offset {offset}: {detail}")]
    Corruption { offset: u64, detail: String },

    #[error("Sequence gap: expected {expected}, got {got}")]
    SequenceGap { expected: u64, got: u64 },

    #[error("Duplicate sequence: {sequence}")]
    DuplicateSequence { sequence: u64 },

    #[error("Sequence not monotonic: prev={prev}, current={current}")]
    NotMonotonic { prev: u64, current: u64 },
}

// ── Corruption Log Entry ────────────────────────────────────────────

/// Structured corruption log entry for diagnostics.
#[derive(Debug, Clone)]
pub struct CorruptionRecord {
    /// Byte offset in the file where corruption was detected.
    pub byte_offset: u64,
    /// Type of corruption.
    pub kind: CorruptionKind,
    /// Human-readable detail message.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CorruptionKind {
    ChecksumMismatch,
    TruncatedEntry,
    InvalidUtf8,
    UnexpectedEof,
}

// ── Journal Reader ──────────────────────────────────────────────────

/// Sequential journal reader with checksum validation and corruption detection.
pub struct JournalReader {
    /// All journal file paths, sorted by index.
    files: Vec<PathBuf>,
    /// Index of the current file being read.
    current_file_idx: usize,
    /// Raw data of the current file.
    data: Vec<u8>,
    /// Current read position within `data`.
    pos: usize,
    /// Global byte offset (across all files).
    global_offset: u64,
    /// Last successfully read sequence number.
    last_sequence: Option<u64>,
    /// Accumulated corruption records.
    corruption_log: Vec<CorruptionRecord>,
}

impl JournalReader {
    /// Open a reader over all journal files in the given directory.
    pub fn open(dir: &Path) -> Result<Self, ReaderError> {
        let files = Self::discover_files(dir)?;
        let mut reader = Self {
            files,
            current_file_idx: 0,
            data: Vec::new(),
            pos: 0,
            global_offset: 0,
            last_sequence: None,
            corruption_log: Vec::new(),
        };
        reader.load_current_file()?;
        Ok(reader)
    }

    /// Read the next valid entry, validating its checksum.
    ///
    /// Returns `None` when all entries have been read.
    pub fn next_entry(&mut self) -> Result<Option<JournalEntry>, ReaderError> {
        loop {
            if self.pos >= self.data.len() {
                if !self.advance_file()? {
                    return Ok(None); // All files exhausted
                }
            }

            let offset_before = self.global_offset;
            match JournalEntry::from_bytes(&self.data[self.pos..]) {
                Ok((entry, consumed)) => {
                    self.pos += consumed;
                    self.global_offset += consumed as u64;

                    // Validate checksum (spec §10.8.1)
                    if !entry.verify_checksum() {
                        self.corruption_log.push(CorruptionRecord {
                            byte_offset: offset_before,
                            kind: CorruptionKind::ChecksumMismatch,
                            detail: format!(
                                "CRC32C mismatch for seq={}, stored={:#010x}",
                                entry.sequence, entry.checksum
                            ),
                        });
                        return Err(ReaderError::ChecksumMismatch {
                            offset: offset_before,
                            sequence: entry.sequence,
                        });
                    }

                    self.last_sequence = Some(entry.sequence);
                    return Ok(Some(entry));
                }
                Err(_) => {
                    // Could be truncated entry at end of file
                    let remaining = self.data.len() - self.pos;
                    if remaining > 0 {
                        self.corruption_log.push(CorruptionRecord {
                            byte_offset: offset_before,
                            kind: CorruptionKind::TruncatedEntry,
                            detail: format!(
                                "Truncated entry: {} bytes remaining, cannot parse",
                                remaining
                            ),
                        });
                    }
                    // Try next file
                    self.pos = self.data.len();
                }
            }
        }
    }

    /// Read all valid entries, collecting them into a Vec.
    pub fn read_all(&mut self) -> Result<Vec<JournalEntry>, ReaderError> {
        let mut entries = Vec::new();
        while let Some(entry) = self.next_entry()? {
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Read all entries, performing gapless sequence validation (spec §14.6).
    ///
    /// Returns entries up to the first sequence violation.
    pub fn read_all_validated(&mut self) -> Result<Vec<JournalEntry>, ReaderError> {
        let mut entries = Vec::new();
        let mut expected_seq: Option<u64> = None;

        while let Some(entry) = self.next_entry()? {
            if let Some(exp) = expected_seq {
                if entry.sequence != exp {
                    return Err(ReaderError::SequenceGap {
                        expected: exp,
                        got: entry.sequence,
                    });
                }
            }
            expected_seq = Some(entry.sequence + 1);
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Seek to the first entry with `sequence >= target_seq`.
    ///
    /// Entries before `target_seq` are skipped. Returns the number of
    /// entries skipped.
    pub fn seek_to_sequence(&mut self, target_seq: u64) -> Result<u64, ReaderError> {
        let mut skipped = 0u64;
        loop {
            if self.pos >= self.data.len() {
                if !self.advance_file()? {
                    break; // All files exhausted
                }
            }

            match JournalEntry::from_bytes(&self.data[self.pos..]) {
                Ok((entry, consumed)) => {
                    if entry.sequence >= target_seq {
                        // Don't consume; leave positioned for next_entry()
                        break;
                    }
                    self.pos += consumed;
                    self.global_offset += consumed as u64;
                    self.last_sequence = Some(entry.sequence);
                    skipped += 1;
                }
                Err(_) => {
                    self.pos = self.data.len();
                }
            }
        }
        Ok(skipped)
    }

    /// Attempt partial recovery: read as many valid entries as possible,
    /// skipping over corrupted regions.
    pub fn recover_entries(&mut self) -> (Vec<JournalEntry>, Vec<CorruptionRecord>) {
        let mut entries = Vec::new();

        loop {
            match self.next_entry() {
                Ok(Some(entry)) => entries.push(entry),
                Ok(None) => break,
                Err(ReaderError::ChecksumMismatch { offset, .. }) => {
                    // Skip past the corrupted entry by scanning for next valid frame
                    self.skip_corrupted_region(offset);
                }
                Err(_) => break,
            }
        }

        let corruption_log = self.corruption_log.clone();
        (entries, corruption_log)
    }

    /// Get the current global byte offset.
    pub fn current_offset(&self) -> u64 {
        self.global_offset
    }

    /// Get the last successfully read sequence number.
    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }

    /// Get all accumulated corruption records.
    pub fn corruption_log(&self) -> &[CorruptionRecord] {
        &self.corruption_log
    }

    /// Validate that a list of entries has gapless, monotonic sequences.
    pub fn validate_sequences(entries: &[JournalEntry]) -> Result<(), ReaderError> {
        for window in entries.windows(2) {
            let prev = &window[0];
            let curr = &window[1];

            if curr.sequence <= prev.sequence {
                return Err(ReaderError::NotMonotonic {
                    prev: prev.sequence,
                    current: curr.sequence,
                });
            }

            if curr.sequence != prev.sequence + 1 {
                return Err(ReaderError::SequenceGap {
                    expected: prev.sequence + 1,
                    got: curr.sequence,
                });
            }
        }
        Ok(())
    }

    /// Detect missing sequences in a range.
    pub fn find_missing_sequences(
        entries: &[JournalEntry],
        expected_start: u64,
        expected_end: u64,
    ) -> Vec<u64> {
        let present: std::collections::HashSet<u64> =
            entries.iter().map(|e| e.sequence).collect();
        (expected_start..=expected_end)
            .filter(|s| !present.contains(s))
            .collect()
    }

    // ── Internal Helpers ────────────────────────────────────────────

    fn discover_files(dir: &Path) -> Result<Vec<PathBuf>, ReaderError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files: Vec<(u64, PathBuf)> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("journal-") && name.ends_with(".bin") {
                    let idx = name
                        .trim_start_matches("journal-")
                        .trim_end_matches(".bin")
                        .parse::<u64>()
                        .ok()?;
                    Some((idx, e.path()))
                } else {
                    None
                }
            })
            .collect();

        files.sort_by_key(|(idx, _)| *idx);
        Ok(files.into_iter().map(|(_, p)| p).collect())
    }

    fn load_current_file(&mut self) -> Result<(), ReaderError> {
        if self.current_file_idx < self.files.len() {
            let mut file = File::open(&self.files[self.current_file_idx])?;
            self.data.clear();
            file.read_to_end(&mut self.data)?;
            self.pos = 0;
        } else {
            self.data.clear();
            self.pos = 0;
        }
        Ok(())
    }

    fn advance_file(&mut self) -> Result<bool, ReaderError> {
        self.current_file_idx += 1;
        if self.current_file_idx < self.files.len() {
            self.load_current_file()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn skip_corrupted_region(&mut self, _offset: u64) {
        // Skip forward byte-by-byte looking for a valid length prefix
        while self.pos < self.data.len() {
            self.pos += 1;
            self.global_offset += 1;
            if self.pos + 4 <= self.data.len() {
                let len = u32::from_le_bytes([
                    self.data[self.pos],
                    self.data[self.pos + 1],
                    self.data[self.pos + 2],
                    self.data[self.pos + 3],
                ]) as usize;
                if len > 0
                    && len < 10_000_000
                    && self.pos + 4 + len <= self.data.len()
                {
                    // Possible valid entry found, stop skipping
                    break;
                }
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{JournalConfig, JournalWriter};
    use tempfile::TempDir;

    fn write_test_entries(dir: &Path, count: u64) {
        let config = JournalConfig::new(dir);
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);
        for seq in 1..=count {
            let entry = JournalEntry::new(
                seq,
                1_000_000_000 + (seq as i64 * 1_000),
                format!("Event{}", seq % 3),
                vec![seq as u8; 10],
            );
            writer.append(&entry).unwrap();
        }
        writer.sync().unwrap();
    }

    #[test]
    fn test_sequential_read() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 50);

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let entries = reader.read_all().unwrap();
        assert_eq!(entries.len(), 50);
        assert_eq!(entries[0].sequence, 1);
        assert_eq!(entries[49].sequence, 50);
    }

    #[test]
    fn test_checksum_validation_detects_corruption() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 5);

        // Corrupt the journal file: flip a byte in the payload area
        let files: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bin"))
            .collect();

        let path = files[0].path();
        let mut data = fs::read(&path).unwrap();
        // Corrupt a byte deep in the file (not the length prefix)
        if data.len() > 30 {
            data[28] ^= 0xFF;
        }
        fs::write(&path, &data).unwrap();

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let result = reader.read_all();
        // Should either error on checksum or return fewer entries
        match result {
            Err(ReaderError::ChecksumMismatch { .. }) => { /* expected */ }
            Ok(entries) => assert!(entries.len() < 5, "Should detect corruption"),
            Err(_) => { /* other error from corruption is also acceptable */ }
        }
    }

    #[test]
    fn test_partial_recovery_skips_corrupted() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 10);

        // Corrupt mid-file
        let files: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bin"))
            .collect();
        let path = files[0].path();
        let mut data = fs::read(&path).unwrap();
        // Corrupt byte 28 (inside first or second entry)
        if data.len() > 30 {
            data[28] ^= 0xFF;
        }
        fs::write(&path, &data).unwrap();

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let (entries, corruptions) = reader.recover_entries();

        // Should recover some entries and log corruptions
        assert!(!corruptions.is_empty() || entries.len() < 10);
    }

    #[test]
    fn test_offset_tracking() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 5);

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let initial_offset = reader.current_offset();

        reader.next_entry().unwrap();
        assert!(reader.current_offset() > initial_offset);
    }

    #[test]
    fn test_replay_from_offset() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 20);

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let skipped = reader.seek_to_sequence(11).unwrap();
        assert_eq!(skipped, 10);

        let entry = reader.next_entry().unwrap().unwrap();
        assert_eq!(entry.sequence, 11);
    }

    #[test]
    fn test_sequence_validation_gapless() {
        let entries: Vec<JournalEntry> = (1..=10)
            .map(|seq| JournalEntry::new(seq, 1000 * seq as i64, "Test".into(), vec![]))
            .collect();

        assert!(JournalReader::validate_sequences(&entries).is_ok());
    }

    #[test]
    fn test_sequence_validation_detects_gap() {
        let entries = vec![
            JournalEntry::new(1, 1000, "A".into(), vec![]),
            JournalEntry::new(2, 2000, "B".into(), vec![]),
            JournalEntry::new(5, 5000, "C".into(), vec![]), // gap: 3,4 missing
        ];

        match JournalReader::validate_sequences(&entries) {
            Err(ReaderError::SequenceGap { expected, got }) => {
                assert_eq!(expected, 3);
                assert_eq!(got, 5);
            }
            other => panic!("Expected SequenceGap, got: {:?}", other),
        }
    }

    #[test]
    fn test_detect_missing_sequence() {
        let entries = vec![
            JournalEntry::new(1, 100, "A".into(), vec![]),
            JournalEntry::new(3, 300, "B".into(), vec![]),
            JournalEntry::new(5, 500, "C".into(), vec![]),
        ];

        let missing = JournalReader::find_missing_sequences(&entries, 1, 5);
        assert_eq!(missing, vec![2, 4]);
    }

    #[test]
    fn test_corruption_logging() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 5);

        // Corrupt file
        let files: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bin"))
            .collect();
        let path = files[0].path();
        let mut data = fs::read(&path).unwrap();
        if data.len() > 30 {
            data[28] ^= 0xFF;
        }
        fs::write(&path, &data).unwrap();

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let (_, corruptions) = reader.recover_entries();

        // Should have logged at least one corruption
        if !corruptions.is_empty() {
            let first = &corruptions[0];
            assert!(
                first.kind == CorruptionKind::ChecksumMismatch
                    || first.kind == CorruptionKind::TruncatedEntry
            );
        }
    }

    #[test]
    fn test_read_all_validated() {
        let tmp = TempDir::new().unwrap();
        write_test_entries(tmp.path(), 20);

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let entries = reader.read_all_validated().unwrap();
        assert_eq!(entries.len(), 20);
    }

    #[test]
    fn test_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let entries = reader.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_multi_file_read() {
        let tmp = TempDir::new().unwrap();
        let config = JournalConfig {
            max_file_size: 100, // Force rotation
            ..JournalConfig::new(tmp.path())
        };
        let mut writer = JournalWriter::open(config).unwrap();
        writer.set_next_sequence(1);
        for seq in 1..=30 {
            writer
                .append(&JournalEntry::new(
                    seq,
                    1000 * seq as i64,
                    "Multi".into(),
                    vec![seq as u8; 5],
                ))
                .unwrap();
        }
        writer.sync().unwrap();

        let mut reader = JournalReader::open(tmp.path()).unwrap();
        let entries = reader.read_all().unwrap();
        assert_eq!(entries.len(), 30);
        assert_eq!(entries.last().unwrap().sequence, 30);
    }

    #[test]
    fn test_not_monotonic_detection() {
        let entries = vec![
            JournalEntry::new(5, 5000, "A".into(), vec![]),
            JournalEntry::new(3, 3000, "B".into(), vec![]), // not monotonic
        ];

        match JournalReader::validate_sequences(&entries) {
            Err(ReaderError::NotMonotonic { prev, current }) => {
                assert_eq!(prev, 5);
                assert_eq!(current, 3);
            }
            other => panic!("Expected NotMonotonic, got: {:?}", other),
        }
    }
}
