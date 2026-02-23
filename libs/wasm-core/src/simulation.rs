//! Order Simulation — Fill estimation against mock order books
//!
//! Implements spec §3 (Trade Lifecycle) simulation for client-side previews.
//! All calculations are deterministic: fixed-point `Decimal`, no system calls,
//! sorted price levels by definition.

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use types::fee::FeeTier;
use types::numeric::{Price, Quantity};
use types::order::Side;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Display precision (8 dp, spec §7.2).
const DISPLAY_DP: u32 = 8;

/// Fee rounding precision (8 dp, spec §7.2: round UP to 8 dp).
const FEE_DP: u32 = 8;

// ---------------------------------------------------------------------------
// Mock order book
// ---------------------------------------------------------------------------

/// A single price level in the order book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Price,
    pub quantity: Quantity,
}

/// Mock order book for client-side simulation.
///
/// Bids are sorted descending by price (best bid first).
/// Asks are sorted ascending by price (best ask first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockOrderBook {
    /// Bid levels, sorted descending (highest price first)
    pub bids: Vec<PriceLevel>,
    /// Ask levels, sorted ascending (lowest price first)
    pub asks: Vec<PriceLevel>,
}

impl MockOrderBook {
    /// Create a new mock order book.
    ///
    /// Bids are automatically sorted desc, asks asc.
    pub fn new(mut bids: Vec<PriceLevel>, mut asks: Vec<PriceLevel>) -> Self {
        bids.sort_by(|a, b| b.price.cmp(&a.price)); // descending
        asks.sort_by(|a, b| a.price.cmp(&b.price)); // ascending
        Self { bids, asks }
    }

    /// Best bid price, if any.
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.first().map(|l| l.price)
    }

    /// Best ask price, if any.
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.first().map(|l| l.price)
    }
}

// ---------------------------------------------------------------------------
// Simulated order
// ---------------------------------------------------------------------------

/// A hypothetical order to simulate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimOrder {
    pub side: Side,
    pub quantity: Quantity,
    /// If `None`, simulate as market order taking best available price.
    pub limit_price: Option<Price>,
}

// ---------------------------------------------------------------------------
// Fill record
// ---------------------------------------------------------------------------

/// A single fill produced during simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimFill {
    pub price: Price,
    pub quantity: Quantity,
    pub value: Decimal,
}

// ---------------------------------------------------------------------------
// Simulation result
// ---------------------------------------------------------------------------

/// Result of simulating an order against the mock order book.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimResult {
    /// Individual fills at each price level
    pub fills: Vec<SimFill>,
    /// Total quantity filled
    pub filled_quantity: Decimal,
    /// Unfilled quantity (for partial fills)
    pub unfilled_quantity: Decimal,
    /// Volume-weighted average execution price
    pub avg_execution_price: Decimal,
    /// Slippage relative to best price (percentage, e.g., 0.0012 = 0.12%)
    pub slippage: Decimal,
    /// Estimated fee (based on provided fee tier)
    pub estimated_fee: Decimal,
    /// Total cost/proceeds including fee
    pub total_cost: Decimal,
    /// Whether the order was fully filled
    pub is_fully_filled: bool,
}

// ---------------------------------------------------------------------------
// Cancellation simulation result
// ---------------------------------------------------------------------------

/// Result of simulating an order cancellation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelSimResult {
    /// Margin that would be released
    pub margin_released: Decimal,
    /// Unfilled quantity being cancelled
    pub cancelled_quantity: Decimal,
}

// ---------------------------------------------------------------------------
// Simulation engine (frozen API)
// ---------------------------------------------------------------------------

/// Deterministic order simulation engine.
///
/// Public API is frozen — methods are stable and will not change.
pub struct SimulationEngine {
    book: MockOrderBook,
    fee_tier: FeeTier,
}

impl SimulationEngine {
    /// Create a new simulation engine.
    pub fn new(book: MockOrderBook, fee_tier: FeeTier) -> Self {
        Self { book, fee_tier }
    }

    /// Simulate a single order fill against the order book.
    pub fn simulate(&self, order: &SimOrder) -> SimResult {
        let levels = match order.side {
            Side::BUY => &self.book.asks,
            Side::SELL => &self.book.bids,
        };

        let best_price = levels.first().map(|l| l.price.as_decimal());

        let mut fills = Vec::new();
        let mut remaining = order.quantity.as_decimal();
        let mut total_value = Decimal::ZERO;
        let mut total_filled = Decimal::ZERO;

        for level in levels {
            if remaining <= Decimal::ZERO {
                break;
            }

            // Respect limit price
            if let Some(limit) = order.limit_price {
                match order.side {
                    Side::BUY => {
                        if level.price.as_decimal() > limit.as_decimal() {
                            break;
                        }
                    }
                    Side::SELL => {
                        if level.price.as_decimal() < limit.as_decimal() {
                            break;
                        }
                    }
                }
            }

            let fill_qty = remaining.min(level.quantity.as_decimal());
            let fill_value = round_display(fill_qty * level.price.as_decimal());

            fills.push(SimFill {
                price: level.price,
                quantity: Quantity::try_new(fill_qty).unwrap_or(Quantity::zero()),
                value: fill_value,
            });

            total_value += fill_value;
            total_filled += fill_qty;
            remaining -= fill_qty;
        }

        let avg_price = if total_filled > Decimal::ZERO {
            round_display(total_value / total_filled)
        } else {
            Decimal::ZERO
        };

        let slippage = match best_price {
            Some(bp) if bp > Decimal::ZERO && total_filled > Decimal::ZERO => {
                let diff = (avg_price - bp).abs();
                round_display(diff / bp)
            }
            _ => Decimal::ZERO,
        };

        // Fee: taker fee for market orders (round UP, spec §7.2)
        let fee = round_up_fee(total_value * self.fee_tier.taker_rate);

        let total_cost = match order.side {
            Side::BUY => round_display(total_value + fee),
            Side::SELL => round_display(total_value - fee),
        };

        let unfilled = round_display(order.quantity.as_decimal() - total_filled);

        SimResult {
            fills,
            filled_quantity: round_display(total_filled),
            unfilled_quantity: unfilled,
            avg_execution_price: avg_price,
            slippage,
            estimated_fee: fee,
            total_cost,
            is_fully_filled: remaining <= Decimal::ZERO,
        }
    }

    /// Simulate a batch of orders (deterministic order of processing).
    pub fn simulate_batch(&self, orders: &[SimOrder]) -> Vec<SimResult> {
        orders.iter().map(|order| self.simulate(order)).collect()
    }

    /// Simulate cancellation of an existing order.
    ///
    /// Returns the margin that would be released and the cancelled quantity.
    pub fn simulate_cancel(
        &self,
        unfilled_quantity: Quantity,
        order_price: Price,
        leverage: u8,
    ) -> CancelSimResult {
        let notional = round_display(
            unfilled_quantity.as_decimal() * order_price.as_decimal(),
        );
        let margin_released = round_display(notional / Decimal::from(leverage));

        CancelSimResult {
            margin_released,
            cancelled_quantity: round_display(unfilled_quantity.as_decimal()),
        }
    }

    /// Estimate execution price for a given quantity (convenience wrapper).
    pub fn estimate_execution_price(
        &self,
        side: Side,
        quantity: Quantity,
    ) -> Decimal {
        let order = SimOrder {
            side,
            quantity,
            limit_price: None,
        };
        self.simulate(&order).avg_execution_price
    }

    /// Estimate slippage for a given quantity (convenience wrapper).
    pub fn estimate_slippage(&self, side: Side, quantity: Quantity) -> Decimal {
        let order = SimOrder {
            side,
            quantity,
            limit_price: None,
        };
        self.simulate(&order).slippage
    }

    /// Estimate fee for a given quantity (convenience wrapper).
    pub fn estimate_fee(&self, side: Side, quantity: Quantity) -> Decimal {
        let order = SimOrder {
            side,
            quantity,
            limit_price: None,
        };
        self.simulate(&order).estimated_fee
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Round to display precision, HALF_UP.
fn round_display(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(DISPLAY_DP, RoundingStrategy::MidpointAwayFromZero)
}

/// Round fee UP (never undercharge, spec §7.2).
fn round_up_fee(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(FEE_DP, RoundingStrategy::AwayFromZero)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn sample_book() -> MockOrderBook {
        MockOrderBook::new(
            vec![
                PriceLevel {
                    price: Price::from_u64(49_900),
                    quantity: Quantity::from_str("2.0").unwrap(),
                },
                PriceLevel {
                    price: Price::from_u64(49_800),
                    quantity: Quantity::from_str("3.0").unwrap(),
                },
                PriceLevel {
                    price: Price::from_u64(49_700),
                    quantity: Quantity::from_str("5.0").unwrap(),
                },
            ],
            vec![
                PriceLevel {
                    price: Price::from_u64(50_100),
                    quantity: Quantity::from_str("1.0").unwrap(),
                },
                PriceLevel {
                    price: Price::from_u64(50_200),
                    quantity: Quantity::from_str("2.0").unwrap(),
                },
                PriceLevel {
                    price: Price::from_u64(50_300),
                    quantity: Quantity::from_str("5.0").unwrap(),
                },
            ],
        )
    }

    fn sample_fee_tier() -> FeeTier {
        FeeTier {
            volume_threshold: Decimal::ZERO,
            maker_rate: Decimal::from_str_exact("0.0002").unwrap(),
            taker_rate: Decimal::from_str_exact("0.0005").unwrap(),
        }
    }

    fn sample_engine() -> SimulationEngine {
        SimulationEngine::new(sample_book(), sample_fee_tier())
    }

    #[test]
    fn test_best_bid_ask() {
        let book = sample_book();
        assert_eq!(book.best_bid(), Some(Price::from_u64(49_900)));
        assert_eq!(book.best_ask(), Some(Price::from_u64(50_100)));
    }

    #[test]
    fn test_buy_full_fill_single_level() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("1.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        assert!(result.is_fully_filled);
        assert_eq!(result.filled_quantity, Decimal::ONE);
        assert_eq!(result.unfilled_quantity, Decimal::ZERO);
        assert_eq!(result.avg_execution_price, Decimal::from(50_100));
        assert_eq!(result.slippage, Decimal::ZERO); // no slippage at best price
        assert_eq!(result.fills.len(), 1);
    }

    #[test]
    fn test_buy_multi_level_fill() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("2.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        assert!(result.is_fully_filled);
        assert_eq!(result.fills.len(), 2);
        // 1.0 @ 50100 + 1.0 @ 50200 = 100300 / 2 = 50150
        assert_eq!(result.avg_execution_price, Decimal::from(50_150));
    }

    #[test]
    fn test_sell_full_fill() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::SELL,
            quantity: Quantity::from_str("2.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        assert!(result.is_fully_filled);
        assert_eq!(result.avg_execution_price, Decimal::from(49_900));
    }

    #[test]
    fn test_partial_fill() {
        let engine = sample_engine();
        // Ask side only has 1+2+5=8 total
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("10.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        assert!(!result.is_fully_filled);
        assert_eq!(result.filled_quantity, Decimal::from(8));
        assert_eq!(result.unfilled_quantity, Decimal::from(2));
    }

    #[test]
    fn test_limit_order_respects_price() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("5.0").unwrap(),
            limit_price: Some(Price::from_u64(50_150)),
        };
        let result = engine.simulate(&order);

        // Should only fill at 50100 (1.0) — 50200 is above limit
        assert!(!result.is_fully_filled);
        assert_eq!(result.filled_quantity, Decimal::ONE);
    }

    #[test]
    fn test_slippage_calculation() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("3.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        // 1 @ 50100 + 2 @ 50200 = 150500 / 3 ≈ 50166.67
        // slippage = |50166.67 - 50100| / 50100 ≈ 0.00133
        assert!(result.slippage > Decimal::ZERO);
        assert!(result.slippage < Decimal::from_str_exact("0.002").unwrap());
    }

    #[test]
    fn test_fee_estimation() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("1.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        // value = 50100, taker_rate = 0.0005, fee = 25.05
        assert_eq!(result.estimated_fee, Decimal::from_str_exact("25.05").unwrap());
    }

    #[test]
    fn test_total_cost_buy() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("1.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        // total_cost = value + fee = 50100 + 25.05 = 50125.05
        assert_eq!(result.total_cost, Decimal::from_str_exact("50125.05").unwrap());
    }

    #[test]
    fn test_total_cost_sell() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::SELL,
            quantity: Quantity::from_str("2.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);

        // value = 99800, fee = 99800 * 0.0005 = 49.9, total = 99800 - 49.9 = 99750.1
        assert_eq!(result.total_cost, Decimal::from_str_exact("99750.1").unwrap());
    }

    #[test]
    fn test_batch_simulation() {
        let engine = sample_engine();
        let orders = vec![
            SimOrder {
                side: Side::BUY,
                quantity: Quantity::from_str("1.0").unwrap(),
                limit_price: None,
            },
            SimOrder {
                side: Side::SELL,
                quantity: Quantity::from_str("1.0").unwrap(),
                limit_price: None,
            },
        ];
        let results = engine.simulate_batch(&orders);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_fully_filled);
        assert!(results[1].is_fully_filled);
    }

    #[test]
    fn test_cancel_simulation() {
        let engine = sample_engine();
        let result = engine.simulate_cancel(
            Quantity::from_str("2.0").unwrap(),
            Price::from_u64(50_000),
            10,
        );
        // notional = 100 000, leverage 10 → margin = 10 000
        assert_eq!(result.margin_released, Decimal::from(10_000));
        assert_eq!(result.cancelled_quantity, Decimal::from(2));
    }

    #[test]
    fn test_estimate_convenience_methods() {
        let engine = sample_engine();
        let price = engine.estimate_execution_price(
            Side::BUY,
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(price, Decimal::from(50_100));

        let slip = engine.estimate_slippage(
            Side::BUY,
            Quantity::from_str("1.0").unwrap(),
        );
        assert_eq!(slip, Decimal::ZERO);

        let fee = engine.estimate_fee(
            Side::BUY,
            Quantity::from_str("1.0").unwrap(),
        );
        assert!(fee > Decimal::ZERO);
    }

    #[test]
    fn test_empty_book_buy() {
        let book = MockOrderBook::new(vec![], vec![]);
        let engine = SimulationEngine::new(book, sample_fee_tier());
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("1.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);
        assert!(!result.is_fully_filled);
        assert_eq!(result.filled_quantity, Decimal::ZERO);
    }

    #[test]
    fn test_deterministic_output() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("3.0").unwrap(),
            limit_price: None,
        };
        let r1 = engine.simulate(&order);
        let r2 = engine.simulate(&order);
        assert_eq!(r1, r2, "Simulation must be deterministic");
    }

    #[test]
    fn test_sim_result_serialization() {
        let engine = sample_engine();
        let order = SimOrder {
            side: Side::BUY,
            quantity: Quantity::from_str("1.0").unwrap(),
            limit_price: None,
        };
        let result = engine.simulate(&order);
        let json = serde_json::to_string(&result).unwrap();
        let restored: SimResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, restored);
    }
}
