//! Observability and metrics for the Market Data Service
//!
//! Provides metric collection for Prometheus-style monitoring.
//! Tracks event processing latency, broadcast performance,
//! queue depths, dropped messages, and resource usage.
//!
//! Implements spec §10: failure detection via observable metrics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Core metrics for the Market Data Service.
pub struct ServiceMetrics {
    // Event processing
    pub events_processed: AtomicU64,
    pub events_dropped: AtomicU64,
    pub event_processing_ns: Mutex<LatencyTracker>,

    // Broadcasting
    pub messages_broadcast: AtomicU64,
    pub broadcast_latency_ns: Mutex<LatencyTracker>,

    // Snapshots
    pub snapshots_built: AtomicU64,
    pub snapshot_build_ns: Mutex<LatencyTracker>,

    // Replay
    pub replay_events: AtomicU64,
    pub replay_duration_ms: AtomicU64,

    // Per-client metrics
    pub connected_clients: AtomicU64,
    pub messages_dropped_backpressure: AtomicU64,

    // Alerts
    pub alerts: Mutex<Vec<Alert>>,
}

impl ServiceMetrics {
    pub fn new() -> Self {
        Self {
            events_processed: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
            event_processing_ns: Mutex::new(LatencyTracker::new(1000)),
            messages_broadcast: AtomicU64::new(0),
            broadcast_latency_ns: Mutex::new(LatencyTracker::new(1000)),
            snapshots_built: AtomicU64::new(0),
            snapshot_build_ns: Mutex::new(LatencyTracker::new(100)),
            replay_events: AtomicU64::new(0),
            replay_duration_ms: AtomicU64::new(0),
            connected_clients: AtomicU64::new(0),
            messages_dropped_backpressure: AtomicU64::new(0),
            alerts: Mutex::new(Vec::new()),
        }
    }

    /// Record an event processed.
    pub fn record_event_processed(&self, latency_ns: u64) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tracker) = self.event_processing_ns.lock() {
            tracker.record(latency_ns);
        }
    }

    /// Record an event dropped.
    pub fn record_event_dropped(&self) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a broadcast sent.
    pub fn record_broadcast(&self, latency_ns: u64) {
        self.messages_broadcast.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tracker) = self.broadcast_latency_ns.lock() {
            tracker.record(latency_ns);
        }
    }

    /// Record a snapshot built.
    pub fn record_snapshot(&self, build_ns: u64) {
        self.snapshots_built.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut tracker) = self.snapshot_build_ns.lock() {
            tracker.record(build_ns);
        }
    }

    /// Record replay metrics.
    pub fn record_replay(&self, events: u64, duration_ms: u64) {
        self.replay_events.store(events, Ordering::Relaxed);
        self.replay_duration_ms.store(duration_ms, Ordering::Relaxed);
    }

    /// Update connected client count.
    pub fn set_connected_clients(&self, count: u64) {
        self.connected_clients.store(count, Ordering::Relaxed);
    }

    /// Record a message dropped due to backpressure.
    pub fn record_backpressure_drop(&self) {
        self.messages_dropped_backpressure.fetch_add(1, Ordering::Relaxed);
    }

    /// Check alert thresholds and generate alerts.
    pub fn check_thresholds(&self, thresholds: &AlertThresholds) -> Vec<Alert> {
        let mut alerts = Vec::new();

        let dropped = self.events_dropped.load(Ordering::Relaxed);
        if dropped > thresholds.max_events_dropped {
            alerts.push(Alert {
                level: AlertLevel::Warning,
                metric: "events_dropped".to_string(),
                message: format!("Events dropped: {} > threshold {}", dropped, thresholds.max_events_dropped),
            });
        }

        let bp_drops = self.messages_dropped_backpressure.load(Ordering::Relaxed);
        if bp_drops > thresholds.max_backpressure_drops {
            alerts.push(Alert {
                level: AlertLevel::Critical,
                metric: "backpressure_drops".to_string(),
                message: format!("Backpressure drops: {} > threshold {}", bp_drops, thresholds.max_backpressure_drops),
            });
        }

        if let Ok(tracker) = self.event_processing_ns.lock() {
            if let Some(p99) = tracker.percentile(99) {
                if p99 > thresholds.max_event_processing_p99_ns {
                    alerts.push(Alert {
                        level: AlertLevel::Warning,
                        metric: "event_processing_p99".to_string(),
                        message: format!(
                            "Event processing p99: {}ns > threshold {}ns",
                            p99, thresholds.max_event_processing_p99_ns
                        ),
                    });
                }
            }
        }

        if let Ok(mut alert_store) = self.alerts.lock() {
            alert_store.extend(alerts.clone());
        }

        alerts
    }

    /// Export metrics as a BTreeMap for Prometheus-style exposition.
    pub fn export(&self) -> BTreeMap<String, u64> {
        let mut m = BTreeMap::new();
        m.insert("events_processed".to_string(), self.events_processed.load(Ordering::Relaxed));
        m.insert("events_dropped".to_string(), self.events_dropped.load(Ordering::Relaxed));
        m.insert("messages_broadcast".to_string(), self.messages_broadcast.load(Ordering::Relaxed));
        m.insert("snapshots_built".to_string(), self.snapshots_built.load(Ordering::Relaxed));
        m.insert("replay_events".to_string(), self.replay_events.load(Ordering::Relaxed));
        m.insert("replay_duration_ms".to_string(), self.replay_duration_ms.load(Ordering::Relaxed));
        m.insert("connected_clients".to_string(), self.connected_clients.load(Ordering::Relaxed));
        m.insert("messages_dropped_backpressure".to_string(), self.messages_dropped_backpressure.load(Ordering::Relaxed));
        m
    }
}

impl Default for ServiceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks latency samples for percentile calculation.
pub struct LatencyTracker {
    samples: Vec<u64>,
    max_samples: usize,
}

impl LatencyTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    /// Record a latency sample.
    pub fn record(&mut self, value: u64) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(value);
    }

    /// Get a percentile value (0-100).
    pub fn percentile(&self, p: usize) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }

        let mut sorted = self.samples.clone();
        sorted.sort_unstable();

        let idx = (p as f64 / 100.0 * (sorted.len() - 1) as f64) as usize;
        Some(sorted[idx.min(sorted.len() - 1)])
    }

    /// Average latency.
    pub fn average(&self) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: u64 = self.samples.iter().sum();
        Some(sum / self.samples.len() as u64)
    }

    /// Number of samples recorded.
    pub fn count(&self) -> usize {
        self.samples.len()
    }
}

/// Alert severity level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// An alert triggered by threshold breach.
#[derive(Debug, Clone)]
pub struct Alert {
    pub level: AlertLevel,
    pub metric: String,
    pub message: String,
}

/// Configurable alert thresholds.
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Max events dropped before alert.
    pub max_events_dropped: u64,
    /// Max backpressure drops before critical alert.
    pub max_backpressure_drops: u64,
    /// Max event processing p99 latency in nanoseconds.
    pub max_event_processing_p99_ns: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_events_dropped: 100,
            max_backpressure_drops: 50,
            max_event_processing_p99_ns: 1_000_000, // 1ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let metrics = ServiceMetrics::new();

        metrics.record_event_processed(500);
        metrics.record_event_processed(1000);
        metrics.record_event_dropped();

        let exported = metrics.export();
        assert_eq!(exported["events_processed"], 2);
        assert_eq!(exported["events_dropped"], 1);
    }

    #[test]
    fn test_latency_tracker_percentile() {
        let mut tracker = LatencyTracker::new(100);

        for i in 1..=100 {
            tracker.record(i);
        }

        let p50 = tracker.percentile(50).unwrap();
        assert!(p50 >= 49 && p50 <= 51);

        let p99 = tracker.percentile(99).unwrap();
        assert!(p99 >= 98 && p99 <= 100);
    }

    #[test]
    fn test_latency_tracker_average() {
        let mut tracker = LatencyTracker::new(100);
        tracker.record(100);
        tracker.record(200);
        tracker.record(300);

        assert_eq!(tracker.average().unwrap(), 200);
    }

    #[test]
    fn test_alert_thresholds() {
        let metrics = ServiceMetrics::new();
        let thresholds = AlertThresholds {
            max_events_dropped: 5,
            max_backpressure_drops: 3,
            max_event_processing_p99_ns: 500,
        };

        // Under threshold
        let alerts = metrics.check_thresholds(&thresholds);
        assert!(alerts.is_empty());

        // Exceed dropped threshold
        for _ in 0..10 {
            metrics.record_event_dropped();
        }
        let alerts = metrics.check_thresholds(&thresholds);
        assert!(alerts.iter().any(|a| a.metric == "events_dropped"));
    }

    #[test]
    fn test_metrics_export() {
        let metrics = ServiceMetrics::new();
        metrics.record_event_processed(100);
        metrics.record_broadcast(200);
        metrics.record_snapshot(300);
        metrics.set_connected_clients(5);

        let exported = metrics.export();
        assert_eq!(exported["events_processed"], 1);
        assert_eq!(exported["messages_broadcast"], 1);
        assert_eq!(exported["snapshots_built"], 1);
        assert_eq!(exported["connected_clients"], 5);
    }

    #[test]
    fn test_replay_metrics_recording() {
        let metrics = ServiceMetrics::new();
        metrics.record_replay(1000, 500);

        let exported = metrics.export();
        assert_eq!(exported["replay_events"], 1000);
        assert_eq!(exported["replay_duration_ms"], 500);
    }

    #[test]
    fn test_backpressure_drop_metric() {
        let metrics = ServiceMetrics::new();

        for _ in 0..5 {
            metrics.record_backpressure_drop();
        }

        let exported = metrics.export();
        assert_eq!(exported["messages_dropped_backpressure"], 5);
    }

    #[test]
    fn test_latency_tracker_window_eviction() {
        let mut tracker = LatencyTracker::new(3);

        tracker.record(10);
        tracker.record(20);
        tracker.record(30);
        tracker.record(40); // Should evict 10

        assert_eq!(tracker.count(), 3);
        assert_eq!(tracker.average().unwrap(), 30); // (20+30+40)/3
    }
}
