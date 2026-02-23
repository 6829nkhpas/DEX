//! Slow-client attack simulation.
//! Simulates a connection monitor that disconnects clients who send data too slowly
//! (like a Slowloris attack) by enforcing an idle_timeout.

use std::time::{Instant, Duration};

/// Simulates tracking connection health at the gateway level.
pub struct ConnectionMonitor {
    pub connection_id: u64,
    pub last_activity: Instant,
    pub idle_timeout: Duration,
}

impl ConnectionMonitor {
    /// Initialize a new connection with a given timeout threshold.
    pub fn new(connection_id: u64, timeout: Duration, current_time: Instant) -> Self {
        Self {
            connection_id,
            last_activity: current_time,
            idle_timeout: timeout,
        }
    }

    /// Record activity (e.g. receiving a byte or a ping).
    pub fn receive_data(&mut self, current_time: Instant) {
        self.last_activity = current_time;
    }

    /// Checks if the connection should be dropped due to inactivity.
    pub fn is_timed_out(&self, current_time: Instant) -> bool {
        current_time.duration_since(self.last_activity) > self.idle_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slow_client_mitigation() {
        let mut mock_time = Instant::now();
        let timeout = Duration::from_secs(30);

        let mut monitor = ConnectionMonitor::new(1, timeout, mock_time);

        // Client sends headers 1 byte at a time but exceedingly slowly.
        for _ in 0..10 {
            // Client waits 5 seconds between bytes
            mock_time += Duration::from_secs(5);
            
            // Should not timeout yet because max idle time is 30s
            assert!(!monitor.is_timed_out(mock_time));
            
            monitor.receive_data(mock_time);
        }

        // Now client stops responding entirely
        mock_time += Duration::from_secs(31);

        // Should timeout and drop connection
        assert!(monitor.is_timed_out(mock_time));
    }
}
