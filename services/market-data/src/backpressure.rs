//! Backpressure and flow control for WebSocket broadcasting
//!
//! Manages per-client outbound queues with bounded capacity, enforces
//! drop policies for lagging clients, and provides adaptive batching
//! to handle volatility spikes.
//!
//! Without backpressure, the server would crash under sustained high-
//! frequency market events during volatile periods.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Drop policy when a client's outbound queue overflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DropPolicy {
    /// Disconnect the lagging client immediately.
    Disconnect,
    /// Drop oldest messages to make room for newer ones.
    DropOldest,
}

/// A queued outbound message for a client.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    /// Serialized message payload.
    pub payload: String,
    /// Sequence number of the event this message represents.
    pub sequence: u64,
    /// Timestamp when the message was queued.
    pub queued_at: i64,
}

/// Per-client outbound queue with bounded capacity.
#[derive(Debug)]
pub struct ClientQueue {
    /// Messages waiting to be sent.
    messages: Vec<OutboundMessage>,
    /// Maximum queue capacity.
    capacity: usize,
    /// Drop policy on overflow.
    drop_policy: DropPolicy,
    /// Total messages dropped for this client.
    messages_dropped: u64,
    /// Whether this client is marked as lagging.
    is_lagging: bool,
}

impl ClientQueue {
    pub fn new(capacity: usize, drop_policy: DropPolicy) -> Self {
        Self {
            messages: Vec::with_capacity(capacity),
            capacity,
            drop_policy,
            messages_dropped: 0,
            is_lagging: false,
        }
    }

    /// Enqueue a message. Returns Err if client should be disconnected.
    pub fn enqueue(
        &mut self,
        message: OutboundMessage,
    ) -> Result<(), BackpressureAction> {
        if self.messages.len() >= self.capacity {
            self.is_lagging = true;

            match self.drop_policy {
                DropPolicy::Disconnect => {
                    return Err(BackpressureAction::DisconnectClient);
                }
                DropPolicy::DropOldest => {
                    self.messages.remove(0);
                    self.messages_dropped += 1;
                }
            }
        }

        self.messages.push(message);

        // Clear lagging flag if queue is below 50% capacity
        if self.messages.len() < self.capacity / 2 {
            self.is_lagging = false;
        }

        Ok(())
    }

    /// Drain all queued messages for sending.
    pub fn drain(&mut self) -> Vec<OutboundMessage> {
        let mut drained = Vec::new();
        std::mem::swap(&mut drained, &mut self.messages);
        self.is_lagging = false;
        drained
    }

    /// Number of messages currently queued.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Whether this client is currently lagging.
    pub fn is_lagging(&self) -> bool {
        self.is_lagging
    }

    /// Total messages dropped for this client.
    pub fn messages_dropped(&self) -> u64 {
        self.messages_dropped
    }
}

/// Action to take when backpressure is triggered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackpressureAction {
    /// Client should be disconnected.
    DisconnectClient,
}

/// Configuration for the backpressure system.
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Maximum queue size per client.
    pub queue_capacity: usize,
    /// Drop policy on overflow.
    pub drop_policy: DropPolicy,
    /// Adaptive batch size: increase batch size when lagging clients > threshold.
    pub adaptive_batch_threshold: usize,
    /// Normal batch size (messages per flush).
    pub normal_batch_size: usize,
    /// Stressed batch size (larger batches under load).
    pub stressed_batch_size: usize,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 1000,
            drop_policy: DropPolicy::Disconnect,
            adaptive_batch_threshold: 5,
            normal_batch_size: 10,
            stressed_batch_size: 50,
        }
    }
}

/// Manages backpressure across all connected clients.
///
/// Uses BTreeMap for deterministic iteration (spec §12).
pub struct BackpressureManager {
    queues: BTreeMap<u64, ClientQueue>,
    config: BackpressureConfig,
    /// Total backpressure incidents logged.
    total_incidents: u64,
}

impl BackpressureManager {
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            queues: BTreeMap::new(),
            config,
            total_incidents: 0,
        }
    }

    /// Register a new client queue.
    pub fn register_client(&mut self, client_id: u64) {
        self.queues.insert(
            client_id,
            ClientQueue::new(
                self.config.queue_capacity,
                self.config.drop_policy,
            ),
        );
        debug!(client_id, "Registered client queue");
    }

    /// Remove a client queue.
    pub fn remove_client(&mut self, client_id: u64) {
        self.queues.remove(&client_id);
        debug!(client_id, "Removed client queue");
    }

    /// Enqueue a message for a specific client.
    ///
    /// Returns `Some(client_id)` if the client should be disconnected.
    pub fn enqueue(
        &mut self,
        client_id: u64,
        message: OutboundMessage,
    ) -> Option<u64> {
        if let Some(queue) = self.queues.get_mut(&client_id) {
            match queue.enqueue(message) {
                Ok(()) => None,
                Err(BackpressureAction::DisconnectClient) => {
                    self.total_incidents += 1;
                    warn!(
                        client_id,
                        total_incidents = self.total_incidents,
                        "Backpressure: disconnecting lagging client"
                    );
                    Some(client_id)
                }
            }
        } else {
            None
        }
    }

    /// Broadcast a message to all clients that have a queue.
    ///
    /// Returns list of client IDs that should be disconnected.
    pub fn broadcast(&mut self, message: OutboundMessage) -> Vec<u64> {
        let client_ids: Vec<u64> = self.queues.keys().copied().collect();
        let mut to_disconnect = Vec::new();

        for client_id in client_ids {
            if let Some(disconnect_id) =
                self.enqueue(client_id, message.clone())
            {
                to_disconnect.push(disconnect_id);
            }
        }

        to_disconnect
    }

    /// Drain messages for a specific client.
    pub fn drain_client(&mut self, client_id: u64) -> Vec<OutboundMessage> {
        self.queues
            .get_mut(&client_id)
            .map(|q| q.drain())
            .unwrap_or_default()
    }

    /// Get the adaptive batch size based on current load.
    pub fn adaptive_batch_size(&self) -> usize {
        let lagging_count = self
            .queues
            .values()
            .filter(|q| q.is_lagging())
            .count();

        if lagging_count >= self.config.adaptive_batch_threshold {
            self.config.stressed_batch_size
        } else {
            self.config.normal_batch_size
        }
    }

    /// Get IDs of all lagging clients.
    pub fn lagging_clients(&self) -> Vec<u64> {
        self.queues
            .iter()
            .filter(|(_, q)| q.is_lagging())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Get queue depth for a specific client.
    pub fn queue_depth(&self, client_id: u64) -> usize {
        self.queues
            .get(&client_id)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// Total backpressure incidents since creation.
    pub fn total_incidents(&self) -> u64 {
        self.total_incidents
    }

    /// Number of registered client queues.
    pub fn client_count(&self) -> usize {
        self.queues.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message(seq: u64) -> OutboundMessage {
        OutboundMessage {
            payload: format!("{{\"seq\":{}}}", seq),
            sequence: seq,
            queued_at: 1708123456789000000 + (seq as i64 * 1000),
        }
    }

    #[test]
    fn test_client_queue_basic() {
        let mut queue = ClientQueue::new(10, DropPolicy::Disconnect);

        queue.enqueue(make_message(1)).unwrap();
        queue.enqueue(make_message(2)).unwrap();

        assert_eq!(queue.len(), 2);
        assert!(!queue.is_lagging());
    }

    #[test]
    fn test_queue_overflow_disconnect() {
        let mut queue = ClientQueue::new(2, DropPolicy::Disconnect);

        queue.enqueue(make_message(1)).unwrap();
        queue.enqueue(make_message(2)).unwrap();

        let result = queue.enqueue(make_message(3));
        assert_eq!(result.unwrap_err(), BackpressureAction::DisconnectClient);
    }

    #[test]
    fn test_queue_overflow_drop_oldest() {
        let mut queue = ClientQueue::new(2, DropPolicy::DropOldest);

        queue.enqueue(make_message(1)).unwrap();
        queue.enqueue(make_message(2)).unwrap();
        queue.enqueue(make_message(3)).unwrap(); // Should drop seq 1

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.messages_dropped(), 1);

        let drained = queue.drain();
        assert_eq!(drained[0].sequence, 2);
        assert_eq!(drained[1].sequence, 3);
    }

    #[test]
    fn test_backpressure_manager_broadcast() {
        let config = BackpressureConfig {
            queue_capacity: 100,
            ..BackpressureConfig::default()
        };
        let mut mgr = BackpressureManager::new(config);

        mgr.register_client(1);
        mgr.register_client(2);

        let disconnects = mgr.broadcast(make_message(1));
        assert!(disconnects.is_empty());

        assert_eq!(mgr.queue_depth(1), 1);
        assert_eq!(mgr.queue_depth(2), 1);
    }

    #[test]
    fn test_backpressure_disconnect_on_overflow() {
        let config = BackpressureConfig {
            queue_capacity: 2,
            drop_policy: DropPolicy::Disconnect,
            ..BackpressureConfig::default()
        };
        let mut mgr = BackpressureManager::new(config);
        mgr.register_client(1);

        mgr.enqueue(1, make_message(1));
        mgr.enqueue(1, make_message(2));

        let disconnect = mgr.enqueue(1, make_message(3));
        assert_eq!(disconnect, Some(1));
        assert_eq!(mgr.total_incidents(), 1);
    }

    #[test]
    fn test_adaptive_batch_size() {
        let config = BackpressureConfig {
            queue_capacity: 3,
            drop_policy: DropPolicy::DropOldest,
            adaptive_batch_threshold: 1,
            normal_batch_size: 10,
            stressed_batch_size: 50,
            ..BackpressureConfig::default()
        };
        let mut mgr = BackpressureManager::new(config);
        mgr.register_client(1);

        // Normal batch size
        assert_eq!(mgr.adaptive_batch_size(), 10);

        // Fill queue to trigger lagging
        for i in 0..4 {
            mgr.enqueue(1, make_message(i));
        }

        // Should now use stressed batch size
        assert_eq!(mgr.adaptive_batch_size(), 50);
    }

    #[test]
    fn test_drain_client() {
        let mut mgr = BackpressureManager::new(BackpressureConfig::default());
        mgr.register_client(1);

        mgr.enqueue(1, make_message(1));
        mgr.enqueue(1, make_message(2));

        let drained = mgr.drain_client(1);
        assert_eq!(drained.len(), 2);
        assert_eq!(mgr.queue_depth(1), 0);
    }

    #[test]
    fn test_lagging_clients() {
        let config = BackpressureConfig {
            queue_capacity: 3,
            drop_policy: DropPolicy::DropOldest,
            ..BackpressureConfig::default()
        };
        let mut mgr = BackpressureManager::new(config);
        mgr.register_client(1);
        mgr.register_client(2);

        // Fill client 1 past capacity to trigger lagging
        for i in 0..4 {
            mgr.enqueue(1, make_message(i));
        }

        let lagging = mgr.lagging_clients();
        assert!(lagging.contains(&1));
        assert!(!lagging.contains(&2));
    }
}
