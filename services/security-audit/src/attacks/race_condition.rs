//! Race condition simulation.
//! Simulates high-concurrency event ingestion pointing out that sequence numbering
//! must be atomic to ensure no gaps or duplicate numbers exist in the event stream.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct AtomicSequenceGenerator {
    current: AtomicU64,
}

impl AtomicSequenceGenerator {
    pub fn new(start: u64) -> Self {
        Self {
            current: AtomicU64::new(start),
        }
    }

    pub fn next(&self) -> u64 {
        self.current.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for AtomicSequenceGenerator {
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_race_condition_sequence_generation() {
        let generator = Arc::new(AtomicSequenceGenerator::new(1));
        let num_threads = 10;
        let num_iterations = 1000;

        let mut handles = vec![];

        for _ in 0..num_threads {
            let gen_clone = Arc::clone(&generator);
            let handle = thread::spawn(move || {
                let mut local_seqs = Vec::with_capacity(num_iterations);
                for _ in 0..num_iterations {
                    local_seqs.push(gen_clone.next());
                }
                local_seqs
            });
            handles.push(handle);
        }

        let mut all_sequences = HashSet::new();

        for handle in handles {
            let local_seqs = handle.join().unwrap();
            for seq in local_seqs {
                // Ensure no duplicate sequence numbers were generated
                assert!(
                    all_sequences.insert(seq),
                    "Duplicate sequence detected: {}",
                    seq
                );
            }
        }

        // Ensure all numbers from 1 to (num_threads * num_iterations) were generated (no gaps)
        let total = num_threads * num_iterations;
        assert_eq!(all_sequences.len(), total);

        for i in 1..=(total as u64) {
            assert!(
                all_sequences.contains(&i),
                "Gap in sequence detected at: {}",
                i
            );
        }
    }
}
