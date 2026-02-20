//! Market Data Service
//!
//! Consumes matching-engine events and produces:
//! - Order book mirrors with depth aggregation
//! - Book deltas for incremental client updates
//! - Full depth snapshots for reconnect logic
//! - Public trade streams
//! - OHLCV candle aggregation (multi-timeframe)
//! - WebSocket real-time feeds with backpressure
//!
//! Implements spec §9 section 3.8 (Market Data Service) with deterministic
//! behavior per §12 (Determinism Rules) and §14 (Sequence Numbering).
//!
//! # Architecture
//!
//! ```text
//! MatchingEngine Events
//!        │
//!    ┌───▼───┐
//!    │Ingest │  ← Validates, dedupes, orders events
//!    └───┬───┘
//!        │
//!   ┌────┴─────┬────────────┐
//!   │          │            │
//! ┌─▼──┐  ┌───▼───┐  ┌────▼────┐
//! │Book│  │Trades │  │Candles  │
//! └─┬──┘  └───┬───┘  └────┬────┘
//!   │         │            │
//! ┌─▼──────┐  │            │
//! │Deltas  │  │            │
//! └─┬──────┘  │            │
//!   │         │            │
//! ┌─▼─────────▼────────────▼──┐
//! │   WebSocket Broadcast     │
//! └───────────────────────────┘
//! ```

pub mod events;
pub mod ingestion;

// Library version
pub const SERVICE_VERSION: &str = "0.1.0";
