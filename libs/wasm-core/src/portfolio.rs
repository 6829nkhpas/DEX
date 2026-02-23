//! Portfolio Engine — Client-side portfolio aggregation
//!
//! Implements spec §4.9 (Account Aggregates) with deterministic computation.
//! Uses `BTreeMap` for sorted iteration per spec §12.3.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use types::account::Balance;
use types::ids::AccountId;
use types::numeric::Price;
use types::position::{Position, PositionSide};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Internal precision for intermediate calculations (spec §12, 18 dp).
const INTERNAL_DP: u32 = 18;

/// Display precision for output values (8 dp, spec §7.2).
const DISPLAY_DP: u32 = 8;

// ---------------------------------------------------------------------------
// Portfolio struct
// ---------------------------------------------------------------------------

/// Client-side portfolio snapshot.
///
/// All fields use deterministic containers (`BTreeMap`) so iteration order
/// is identical across nodes and runs (spec §12.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Portfolio {
    /// Owning account
    pub account_id: AccountId,

    /// Per-asset balances keyed by asset symbol (sorted)
    pub balances: BTreeMap<String, Balance>,

    /// Open positions keyed by symbol (sorted)
    pub positions: BTreeMap<String, Position>,

    /// Mark / index prices keyed by symbol (sorted)
    pub prices: BTreeMap<String, Price>,
}

// ---------------------------------------------------------------------------
// Portfolio summary (serialization bridge)
// ---------------------------------------------------------------------------

/// Lightweight summary suitable for JSON transport to the UI layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioSummary {
    pub account_id: AccountId,
    pub total_equity: Decimal,
    pub total_balance_value: Decimal,
    pub total_unrealized_pnl: Decimal,
    pub total_realized_pnl: Decimal,
    pub position_count: usize,
    pub asset_count: usize,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Portfolio {
    /// Create an empty portfolio for the given account.
    pub fn new(account_id: AccountId) -> Self {
        Self {
            account_id,
            balances: BTreeMap::new(),
            positions: BTreeMap::new(),
            prices: BTreeMap::new(),
        }
    }

    // -- mutators ----------------------------------------------------------

    /// Insert or replace a balance entry.
    pub fn set_balance(&mut self, balance: Balance) {
        self.balances.insert(balance.asset.clone(), balance);
    }

    /// Insert or replace a position entry.
    pub fn set_position(&mut self, position: Position) {
        self.positions
            .insert(position.symbol.as_str().to_owned(), position);
    }

    /// Insert or replace a price entry.
    pub fn set_price(&mut self, symbol: &str, price: Price) {
        self.prices.insert(symbol.to_owned(), price);
    }

    // -- balance queries ---------------------------------------------------

    /// Retrieve balance for a single asset.
    pub fn get_balance(&self, asset: &str) -> Option<&Balance> {
        self.balances.get(asset)
    }

    /// Compute the total value of all balances in quote terms.
    ///
    /// For each asset, value = `balance.total × price`.  If no price is
    /// available the asset is treated as a stablecoin with price = 1.
    ///
    /// Rounding: intermediate at `INTERNAL_DP`, final at `DISPLAY_DP` HALF_UP.
    pub fn total_balance_value(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for (asset, balance) in &self.balances {
            let price = self
                .prices
                .get(asset)
                .map(|p| p.as_decimal())
                .unwrap_or(Decimal::ONE);
            let value = balance.total * price;
            total += value
                .round_dp_with_strategy(INTERNAL_DP, RoundingStrategy::MidpointAwayFromZero);
        }
        round_display(total)
    }

    // -- position aggregation ---------------------------------------------

    /// All positions sorted deterministically by symbol.
    pub fn sorted_positions(&self) -> Vec<&Position> {
        self.positions.values().collect()
    }

    /// Number of open positions.
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    // -- PnL computation --------------------------------------------------

    /// Total unrealized PnL across every open position.
    ///
    /// Per spec §4.4.3:
    ///   LONG:  (mark_price − entry_price) × size
    ///   SHORT: (entry_price − mark_price) × size
    pub fn total_unrealized_pnl(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for position in self.positions.values() {
            total += compute_unrealized_pnl(position);
        }
        round_display(total)
    }

    /// Total realized PnL across every open position.
    ///
    /// This is the sum of `position.realized_pnl` for all tracked positions.
    pub fn total_realized_pnl(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for position in self.positions.values() {
            total += position.realized_pnl;
        }
        round_display(total)
    }

    // -- equity ------------------------------------------------------------

    /// Total equity per spec §4.9.1:
    ///
    /// ```text
    /// portfolio_value = Σ(balance[asset] × price[asset])
    ///                 + Σ(position[symbol].unrealized_pnl)
    /// ```
    pub fn total_equity(&self) -> Decimal {
        let balance_value = self.total_balance_value();
        let unrealized = self.total_unrealized_pnl();
        round_display(balance_value + unrealized)
    }

    // -- serialization bridge ----------------------------------------------

    /// Produce a lightweight summary for JSON transport.
    pub fn summary(&self) -> PortfolioSummary {
        PortfolioSummary {
            account_id: self.account_id,
            total_equity: self.total_equity(),
            total_balance_value: self.total_balance_value(),
            total_unrealized_pnl: self.total_unrealized_pnl(),
            total_realized_pnl: self.total_realized_pnl(),
            position_count: self.positions.len(),
            asset_count: self.balances.len(),
        }
    }

    /// Serialize the full portfolio to deterministic JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a portfolio from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Compute unrealized PnL for a single position (spec §4.4.3).
fn compute_unrealized_pnl(position: &Position) -> Decimal {
    let size = position.size.as_decimal();
    let entry = position.entry_price.as_decimal();
    let mark = position.mark_price.as_decimal();

    let pnl = match position.side {
        PositionSide::LONG => (mark - entry) * size,
        PositionSide::SHORT => (entry - mark) * size,
    };

    pnl.round_dp_with_strategy(INTERNAL_DP, RoundingStrategy::MidpointAwayFromZero)
}

/// Round a value to display precision using HALF_UP (spec §12.4.2).
fn round_display(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(DISPLAY_DP, RoundingStrategy::MidpointAwayFromZero)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::{AccountId, MarketId};
    use types::numeric::{Price, Quantity};
    use types::position::PositionSide;

    /// Build a test portfolio with realistic data.
    fn sample_portfolio() -> Portfolio {
        let account_id = AccountId::new();
        let mut portfolio = Portfolio::new(account_id);

        // Balances
        portfolio.set_balance(Balance::new("USDT", Decimal::from(10_000)));
        portfolio.set_balance(Balance::new("BTC", Decimal::from(1)));

        // Prices
        portfolio.set_price("BTC", Price::from_u64(50_000));
        portfolio.set_price("USDT", Price::from_u64(1));

        // Position: Long 1 BTC, entry 48 000, mark 50 000 → uPnL = +2 000
        let pos = Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(48_000),
            Price::from_u64(50_000),
            Price::from_u64(47_500),
            Decimal::from(4_800),
            Decimal::from(480),
            10,
            1_708_123_456_789_000_000,
        );
        portfolio.set_position(pos);

        portfolio
    }

    #[test]
    fn test_empty_portfolio() {
        let p = Portfolio::new(AccountId::new());
        assert_eq!(p.total_balance_value(), Decimal::ZERO);
        assert_eq!(p.total_unrealized_pnl(), Decimal::ZERO);
        assert_eq!(p.total_realized_pnl(), Decimal::ZERO);
        assert_eq!(p.total_equity(), Decimal::ZERO);
        assert_eq!(p.position_count(), 0);
    }

    #[test]
    fn test_balance_value() {
        let p = sample_portfolio();
        // 10 000 USDT × 1 + 1 BTC × 50 000 = 60 000
        assert_eq!(p.total_balance_value(), Decimal::from(60_000));
    }

    #[test]
    fn test_unrealized_pnl_long() {
        let p = sample_portfolio();
        // Long 1 BTC: (50 000 − 48 000) × 1 = 2 000
        assert_eq!(p.total_unrealized_pnl(), Decimal::from(2_000));
    }

    #[test]
    fn test_unrealized_pnl_short() {
        let account_id = AccountId::new();
        let mut p = Portfolio::new(account_id);

        let pos = Position::new(
            account_id,
            MarketId::new("ETH/USDT"),
            PositionSide::SHORT,
            Quantity::from_str("10.0").unwrap(),
            Price::from_u64(3_000),
            Price::from_u64(2_800),
            Price::from_u64(3_100),
            Decimal::from(3_000),
            Decimal::from(300),
            10,
            1_708_123_456_789_000_000,
        );
        p.set_position(pos);
        // Short 10 ETH: (3000 − 2800) × 10 = 2 000
        assert_eq!(p.total_unrealized_pnl(), Decimal::from(2_000));
    }

    #[test]
    fn test_total_equity() {
        let p = sample_portfolio();
        // balance_value = 60 000, uPnL = 2 000 → equity = 62 000
        assert_eq!(p.total_equity(), Decimal::from(62_000));
    }

    #[test]
    fn test_realized_pnl() {
        let p = sample_portfolio();
        // Default realized_pnl on new position is 0
        assert_eq!(p.total_realized_pnl(), Decimal::ZERO);
    }

    #[test]
    fn test_realized_pnl_nonzero() {
        let account_id = AccountId::new();
        let mut p = Portfolio::new(account_id);

        let mut pos = Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(51_000),
            Price::from_u64(49_500),
            Decimal::from(5_000),
            Decimal::from(500),
            10,
            1_708_123_456_789_000_000,
        );
        pos.realized_pnl = Decimal::from(500);
        p.set_position(pos);

        assert_eq!(p.total_realized_pnl(), Decimal::from(500));
    }

    #[test]
    fn test_multiple_positions_aggregate() {
        let account_id = AccountId::new();
        let mut p = Portfolio::new(account_id);

        // Long 1 BTC: entry 50k, mark 52k → +2000
        let pos1 = Position::new(
            account_id,
            MarketId::new("BTC/USDT"),
            PositionSide::LONG,
            Quantity::from_str("1.0").unwrap(),
            Price::from_u64(50_000),
            Price::from_u64(52_000),
            Price::from_u64(49_500),
            Decimal::from(5_000),
            Decimal::from(500),
            10,
            1_708_123_456_789_000_000,
        );
        p.set_position(pos1);

        // Short 5 ETH: entry 3000, mark 2900 → +500
        let pos2 = Position::new(
            account_id,
            MarketId::new("ETH/USDT"),
            PositionSide::SHORT,
            Quantity::from_str("5.0").unwrap(),
            Price::from_u64(3_000),
            Price::from_u64(2_900),
            Price::from_u64(3_100),
            Decimal::from(1_500),
            Decimal::from(150),
            10,
            1_708_123_456_789_000_000,
        );
        p.set_position(pos2);

        assert_eq!(p.total_unrealized_pnl(), Decimal::from(2_500));
    }

    #[test]
    fn test_sorted_positions_deterministic() {
        let account_id = AccountId::new();
        let mut p = Portfolio::new(account_id);

        let make_pos = |sym: &str| {
            Position::new(
                account_id,
                MarketId::new(sym),
                PositionSide::LONG,
                Quantity::from_str("1.0").unwrap(),
                Price::from_u64(100),
                Price::from_u64(100),
                Price::from_u64(90),
                Decimal::from(10),
                Decimal::from(1),
                10,
                1_708_123_456_789_000_000,
            )
        };

        p.set_position(make_pos("SOL/USDT"));
        p.set_position(make_pos("BTC/USDT"));
        p.set_position(make_pos("ETH/USDT"));

        let symbols: Vec<&str> = p
            .sorted_positions()
            .iter()
            .map(|pos| pos.symbol.as_str())
            .collect();
        assert_eq!(symbols, vec!["BTC/USDT", "ETH/USDT", "SOL/USDT"]);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let p = sample_portfolio();
        let json = p.to_json().unwrap();
        let restored = Portfolio::from_json(&json).unwrap();
        assert_eq!(p.account_id, restored.account_id);
        assert_eq!(p.balances.len(), restored.balances.len());
        assert_eq!(p.positions.len(), restored.positions.len());
    }

    #[test]
    fn test_summary_fields() {
        let p = sample_portfolio();
        let s = p.summary();
        assert_eq!(s.total_equity, Decimal::from(62_000));
        assert_eq!(s.total_balance_value, Decimal::from(60_000));
        assert_eq!(s.total_unrealized_pnl, Decimal::from(2_000));
        assert_eq!(s.total_realized_pnl, Decimal::ZERO);
        assert_eq!(s.position_count, 1);
        assert_eq!(s.asset_count, 2);
    }

    #[test]
    fn test_deterministic_replay() {
        // Same inputs must produce identical outputs (spec §12)
        let p1 = sample_portfolio();
        let p2 = sample_portfolio();
        assert_eq!(p1.total_equity(), p2.total_equity());
        assert_eq!(p1.total_unrealized_pnl(), p2.total_unrealized_pnl());
        assert_eq!(p1.total_realized_pnl(), p2.total_realized_pnl());
    }

    #[test]
    fn test_balance_without_price_defaults_to_one() {
        let mut p = Portfolio::new(AccountId::new());
        // No price entry for USDT → treated as stablecoin (price = 1)
        p.set_balance(Balance::new("USDT", Decimal::from(5_000)));
        assert_eq!(p.total_balance_value(), Decimal::from(5_000));
    }

    #[test]
    fn test_precision_handling() {
        let account_id = AccountId::new();
        let mut p = Portfolio::new(account_id);

        p.set_balance(Balance::new("BTC", Decimal::from_str_exact("0.123456789012345678").unwrap()));
        p.set_price("BTC", Price::from_str("50000.12345678").unwrap());

        // Result should be rounded to 8 dp
        let val = p.total_balance_value();
        assert!(val.scale() <= DISPLAY_DP);
    }

    #[test]
    fn test_summary_serialization() {
        let p = sample_portfolio();
        let s = p.summary();
        let json = serde_json::to_string(&s).unwrap();
        let restored: PortfolioSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(s, restored);
    }
}
