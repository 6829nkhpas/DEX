//! WebSocket streaming layer for the Market Data Service
//!
//! Provides real-time WebSocket feeds for traders subscribing to:
//! - Order book updates (snapshots + deltas)
//! - Trade streams
//! - OHLCV candle updates
//!
//! Implements spec §9.3.8: real-time order book updates and spec §7.1:
//! WebSocket event subscriptions.
//!
//! Flow: subscribe → receive snapshot → receive deltas.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Channels available for subscription.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Channel {
    /// Order book depth updates: `book@{symbol}`
    Book { symbol: String },
    /// Public trade stream: `trades@{symbol}`
    Trades { symbol: String },
    /// OHLCV candle updates: `candles@{symbol}@{timeframe}`
    Candles { symbol: String, timeframe: String },
}

impl Channel {
    /// Parse a channel string into a Channel enum.
    ///
    /// Formats:
    /// - `book@BTC/USDT`
    /// - `trades@BTC/USDT`
    /// - `candles@BTC/USDT@M1`
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('@').collect();
        match parts.as_slice() {
            ["book", symbol] => Some(Channel::Book {
                symbol: symbol.to_string(),
            }),
            ["trades", symbol] => Some(Channel::Trades {
                symbol: symbol.to_string(),
            }),
            ["candles", symbol, timeframe] => Some(Channel::Candles {
                symbol: symbol.to_string(),
                timeframe: timeframe.to_string(),
            }),
            _ => None,
        }
    }

    /// Serialize as channel string.
    pub fn to_channel_string(&self) -> String {
        match self {
            Channel::Book { symbol } => format!("book@{}", symbol),
            Channel::Trades { symbol } => format!("trades@{}", symbol),
            Channel::Candles { symbol, timeframe } => {
                format!("candles@{}@{}", symbol, timeframe)
            }
        }
    }
}

/// Client subscription request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeMessage {
    /// Action: "subscribe" or "unsubscribe"
    pub action: String,
    /// Channels to subscribe/unsubscribe to
    pub channels: Vec<String>,
}

/// Server response to a subscription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResponse {
    pub action: String,
    pub channels: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// Parse a raw JSON message into a SubscribeMessage.
pub fn parse_subscribe_message(json: &str) -> Option<SubscribeMessage> {
    serde_json::from_str(json).ok()
}

/// Unique client identifier.
pub type ClientId = u64;

/// Tracks per-client state: subscriptions and sequence tracking.
#[derive(Debug, Clone)]
pub struct ClientState {
    pub client_id: ClientId,
    /// Channels this client is subscribed to.
    pub subscriptions: BTreeSet<Channel>,
    /// Last sequence number sent to this client per channel.
    pub last_sequence: BTreeMap<String, u64>,
    /// Whether the client has received the initial snapshot.
    pub snapshot_sent: BTreeSet<Channel>,
    /// Heartbeat tracking: last pong received (Unix nanos).
    pub last_pong: i64,
    /// Whether client is authenticated.
    pub authenticated: bool,
    /// Number of messages sent in current rate limit window.
    pub messages_in_window: u32,
    /// Start of current rate limit window (Unix nanos).
    pub window_start: i64,
}

impl ClientState {
    /// Create a new client state.
    pub fn new(client_id: ClientId, timestamp: i64) -> Self {
        Self {
            client_id,
            subscriptions: BTreeSet::new(),
            last_sequence: BTreeMap::new(),
            snapshot_sent: BTreeSet::new(),
            last_pong: timestamp,
            authenticated: false,
            messages_in_window: 0,
            window_start: timestamp,
        }
    }

    /// Subscribe to a channel.
    pub fn subscribe(&mut self, channel: Channel) {
        self.subscriptions.insert(channel);
    }

    /// Unsubscribe from a channel.
    pub fn unsubscribe(&mut self, channel: &Channel) {
        self.subscriptions.remove(channel);
        self.snapshot_sent.remove(channel);
    }

    /// Check if subscribed to a channel.
    pub fn is_subscribed(&self, channel: &Channel) -> bool {
        self.subscriptions.contains(channel)
    }

    /// Mark that the snapshot has been sent for this channel.
    pub fn mark_snapshot_sent(&mut self, channel: Channel) {
        self.snapshot_sent.insert(channel);
    }

    /// Whether this channel needs a snapshot (subscribed but not yet sent).
    pub fn needs_snapshot(&self, channel: &Channel) -> bool {
        self.subscriptions.contains(channel) && !self.snapshot_sent.contains(channel)
    }

    /// Record a pong from the client.
    pub fn record_pong(&mut self, timestamp: i64) {
        self.last_pong = timestamp;
    }

    /// Check if the client is stale (no pong within timeout).
    pub fn is_stale(&self, now: i64, timeout_nanos: i64) -> bool {
        now - self.last_pong > timeout_nanos
    }

    /// Update the last sequence sent for a channel.
    pub fn update_last_sequence(&mut self, channel_key: String, sequence: u64) {
        self.last_sequence.insert(channel_key, sequence);
    }
}

/// Configuration for the WebSocket server.
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// Heartbeat interval in nanoseconds (default: 30s).
    pub heartbeat_interval_nanos: i64,
    /// Client stale timeout in nanoseconds (default: 90s).
    pub stale_timeout_nanos: i64,
    /// Max messages per client per rate limit window.
    pub rate_limit_max_messages: u32,
    /// Rate limit window duration in nanoseconds (default: 1s).
    pub rate_limit_window_nanos: i64,
    /// Max subscriptions per client.
    pub max_subscriptions_per_client: usize,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_nanos: 30 * 1_000_000_000,
            stale_timeout_nanos: 90 * 1_000_000_000,
            rate_limit_max_messages: 100,
            rate_limit_window_nanos: 1_000_000_000,
            max_subscriptions_per_client: 50,
        }
    }
}

/// Client registry: tracks all connected clients and their subscriptions.
///
/// Uses BTreeMap for deterministic iteration (spec §12).
pub struct ClientRegistry {
    clients: BTreeMap<ClientId, ClientState>,
    next_id: ClientId,
    config: WsConfig,
}

impl ClientRegistry {
    pub fn new(config: WsConfig) -> Self {
        Self {
            clients: BTreeMap::new(),
            next_id: 1,
            config,
        }
    }

    /// Register a new client and return its ID.
    pub fn register(&mut self, timestamp: i64) -> ClientId {
        let id = self.next_id;
        self.next_id += 1;
        self.clients.insert(id, ClientState::new(id, timestamp));
        id
    }

    /// Remove a client (disconnect).
    pub fn disconnect(&mut self, client_id: ClientId) -> Option<ClientState> {
        self.clients.remove(&client_id)
    }

    /// Get a client by ID.
    pub fn get(&self, client_id: ClientId) -> Option<&ClientState> {
        self.clients.get(&client_id)
    }

    /// Get a mutable reference to a client.
    pub fn get_mut(&mut self, client_id: ClientId) -> Option<&mut ClientState> {
        self.clients.get_mut(&client_id)
    }

    /// Subscribe a client to a channel.
    pub fn subscribe(
        &mut self,
        client_id: ClientId,
        channel: Channel,
    ) -> Result<(), String> {
        let max_subs = self.config.max_subscriptions_per_client;
        if let Some(client) = self.clients.get_mut(&client_id) {
            if client.subscriptions.len() >= max_subs {
                return Err(format!(
                    "Max subscriptions ({}) reached",
                    max_subs
                ));
            }
            client.subscribe(channel);
            Ok(())
        } else {
            Err("Client not found".to_string())
        }
    }

    /// Get all clients subscribed to a channel.
    pub fn subscribers(&self, channel: &Channel) -> Vec<ClientId> {
        self.clients
            .iter()
            .filter(|(_, state)| state.is_subscribed(channel))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Check rate limit for a client. Returns true if within limit.
    pub fn check_rate_limit(
        &mut self,
        client_id: ClientId,
        now: i64,
    ) -> bool {
        if let Some(client) = self.clients.get_mut(&client_id) {
            // Reset window if expired
            if now - client.window_start >= self.config.rate_limit_window_nanos {
                client.messages_in_window = 0;
                client.window_start = now;
            }

            if client.messages_in_window < self.config.rate_limit_max_messages {
                client.messages_in_window += 1;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Find and remove stale clients (no heartbeat response).
    pub fn remove_stale_clients(&mut self, now: i64) -> Vec<ClientId> {
        let timeout = self.config.stale_timeout_nanos;
        let stale: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|(_, state)| state.is_stale(now, timeout))
            .map(|(id, _)| *id)
            .collect();

        for id in &stale {
            self.clients.remove(id);
        }

        stale
    }

    /// Number of connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// All connected client IDs.
    pub fn client_ids(&self) -> Vec<ClientId> {
        self.clients.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_registry() -> ClientRegistry {
        ClientRegistry::new(WsConfig::default())
    }

    #[test]
    fn test_channel_parse() {
        let book = Channel::parse("book@BTC/USDT").unwrap();
        assert_eq!(
            book,
            Channel::Book {
                symbol: "BTC/USDT".to_string()
            }
        );

        let trades = Channel::parse("trades@ETH/USDC").unwrap();
        assert_eq!(
            trades,
            Channel::Trades {
                symbol: "ETH/USDC".to_string()
            }
        );

        let candles = Channel::parse("candles@BTC/USDT@M1").unwrap();
        assert_eq!(
            candles,
            Channel::Candles {
                symbol: "BTC/USDT".to_string(),
                timeframe: "M1".to_string(),
            }
        );

        assert!(Channel::parse("invalid").is_none());
    }

    #[test]
    fn test_channel_to_string() {
        let ch = Channel::Book {
            symbol: "BTC/USDT".to_string(),
        };
        assert_eq!(ch.to_channel_string(), "book@BTC/USDT");
    }

    #[test]
    fn test_client_registration() {
        let mut reg = default_registry();
        let id1 = reg.register(1708123456789000000);
        let id2 = reg.register(1708123456790000000);

        assert_ne!(id1, id2);
        assert_eq!(reg.client_count(), 2);
    }

    #[test]
    fn test_client_subscribe() {
        let mut reg = default_registry();
        let id = reg.register(1708123456789000000);
        let ch = Channel::Book {
            symbol: "BTC/USDT".to_string(),
        };

        reg.subscribe(id, ch.clone()).unwrap();

        let client = reg.get(id).unwrap();
        assert!(client.is_subscribed(&ch));
        assert!(client.needs_snapshot(&ch));
    }

    #[test]
    fn test_subscribers() {
        let mut reg = default_registry();
        let id1 = reg.register(1708123456789000000);
        let id2 = reg.register(1708123456790000000);

        let ch = Channel::Trades {
            symbol: "BTC/USDT".to_string(),
        };
        reg.subscribe(id1, ch.clone()).unwrap();
        reg.subscribe(id2, ch.clone()).unwrap();

        let subs = reg.subscribers(&ch);
        assert_eq!(subs.len(), 2);
    }

    #[test]
    fn test_disconnect() {
        let mut reg = default_registry();
        let id = reg.register(1708123456789000000);
        assert_eq!(reg.client_count(), 1);

        reg.disconnect(id);
        assert_eq!(reg.client_count(), 0);
    }

    #[test]
    fn test_rate_limiting() {
        let config = WsConfig {
            rate_limit_max_messages: 2,
            rate_limit_window_nanos: 1_000_000_000,
            ..WsConfig::default()
        };
        let mut reg = ClientRegistry::new(config);
        let id = reg.register(1708123456789000000);

        let now = 1708123456789000000;
        assert!(reg.check_rate_limit(id, now));
        assert!(reg.check_rate_limit(id, now));
        assert!(!reg.check_rate_limit(id, now)); // Exceeded

        // After window reset
        let later = now + 2_000_000_000;
        assert!(reg.check_rate_limit(id, later)); // New window
    }

    #[test]
    fn test_stale_client_detection() {
        let config = WsConfig {
            stale_timeout_nanos: 10_000_000_000, // 10s
            ..WsConfig::default()
        };
        let mut reg = ClientRegistry::new(config);
        let id = reg.register(1708123456789000000);

        // Not stale yet
        let now = 1708123456789000000 + 5_000_000_000;
        let stale = reg.remove_stale_clients(now);
        assert!(stale.is_empty());

        // Now stale
        let now = 1708123456789000000 + 20_000_000_000;
        let stale = reg.remove_stale_clients(now);
        assert_eq!(stale, vec![id]);
        assert_eq!(reg.client_count(), 0);
    }

    #[test]
    fn test_snapshot_tracking() {
        let mut reg = default_registry();
        let id = reg.register(1708123456789000000);
        let ch = Channel::Book {
            symbol: "BTC/USDT".to_string(),
        };

        reg.subscribe(id, ch.clone()).unwrap();
        let client = reg.get(id).unwrap();
        assert!(client.needs_snapshot(&ch));

        // Mark snapshot sent
        let client = reg.get_mut(id).unwrap();
        client.mark_snapshot_sent(ch.clone());
        assert!(!client.needs_snapshot(&ch));
    }

    #[test]
    fn test_parse_subscribe_message() {
        let json = r#"{"action":"subscribe","channels":["book@BTC/USDT","trades@BTC/USDT"]}"#;
        let msg = parse_subscribe_message(json).unwrap();
        assert_eq!(msg.action, "subscribe");
        assert_eq!(msg.channels.len(), 2);
    }

    #[test]
    fn test_max_subscriptions() {
        let config = WsConfig {
            max_subscriptions_per_client: 2,
            ..WsConfig::default()
        };
        let mut reg = ClientRegistry::new(config);
        let id = reg.register(1708123456789000000);

        reg.subscribe(
            id,
            Channel::Book {
                symbol: "BTC/USDT".to_string(),
            },
        )
        .unwrap();
        reg.subscribe(
            id,
            Channel::Trades {
                symbol: "BTC/USDT".to_string(),
            },
        )
        .unwrap();

        let result = reg.subscribe(
            id,
            Channel::Candles {
                symbol: "BTC/USDT".to_string(),
                timeframe: "M1".to_string(),
            },
        );
        assert!(result.is_err());
    }
}
