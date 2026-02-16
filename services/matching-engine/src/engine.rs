//! Matching engine core
//!
//! Main coordinator for order book and matching logic

use std::collections::HashMap;
use types::ids::{MarketId, OrderId};
use types::numeric::{Price, Quantity};
use types::order::{Order, Side};
use types::trade::Trade;

use crate::book::{AskBook, BidBook};
use crate::matching::{crossing, executor::{MatchExecutor, MatchError}};

/// Main matching engine
pub struct MatchingEngine {
    /// Order books per symbol
    books: HashMap<String, OrderBook>,
    /// Trade executor with sequence generation
    executor: MatchExecutor,
}

/// Order book for a single symbol
struct OrderBook {
    symbol: MarketId,
    bids: BidBook,
    asks: AskBook,
}

/// Result of submitting an order
pub enum SubmitResult {
    /// Order was added to book (no match)
    Resting,
    /// Order was partially filled
    PartiallyFilled { trades: Vec<Trade>, remaining: Order },
    /// Order was completely filled
    Filled { trades: Vec<Trade> },
}

impl MatchingEngine {
    /// Create a new matching engine with starting sequence
    pub fn new(starting_sequence: u64) -> Self {
        Self {
            books: HashMap::new(),
            executor: MatchExecutor::new(starting_sequence),
        }
    }

    /// Submit an order to the matching engine
    ///
    /// This is the main entry point. The order will be matched against
    /// the book and any resulting trades will be returned.
    pub fn submit_order(&mut self, mut order: Order, timestamp: i64) -> Result<SubmitResult, EngineError> {
        let symbol_key = order.symbol.as_str().to_string();
        
        // Get or create order book for this symbol
        if !self.books.contains_key(&symbol_key) {
            self.books.insert(symbol_key.clone(), OrderBook {
                symbol: order.symbol.clone(),
                bids: BidBook::new(),
                asks: AskBook::new(),
            });
        }

        // Match the order against the book
        // Split borrows: book + executor separately
        let trades = {
            let book = self.books.get_mut(&symbol_key).unwrap();
            let executor = &mut self.executor;
            
            match order.side {
                Side::BUY => Self::match_buy_order_impl(book, executor, &mut order, timestamp)?,
                Side::SELL => Self::match_sell_order_impl(book, executor, &mut order, timestamp)?,
            }
        };

        if order.is_filled() {
            Ok(SubmitResult::Filled { trades })
        } else if !trades.is_empty() {
            Ok(SubmitResult::PartiallyFilled {
                trades,
                remaining: order,
            })
        } else {
            // No matches, add to book
            let book = self.books.get_mut(&symbol_key).unwrap();
            match order.side {
                Side::BUY => book.bids.insert(&order),
                Side::SELL => book.asks.insert(&order),
            }
            Ok(SubmitResult::Resting)
        }
    }

    /// Match incoming buy order against asks (implementation)
    fn match_buy_order_impl(
        book: &mut OrderBook,
        executor: &mut MatchExecutor,
        order: &mut Order,
        timestamp: i64,
    ) -> Result<Vec<Trade>, EngineError> {
        let mut trades = Vec::new();

        // Match against asks (sell orders)
        while let Some((ask_price, ask_level)) = book.asks.best_ask_level_mut() {
            // Check if prices cross
            if !crossing::can_match(order.price, ask_price) {
                break;
            }

            // Get front order from ask level
            if let Some((maker_order_id, maker_account_id, maker_quantity)) = ask_level.peek_front() {
                // Determine match quantity
                let match_qty = if order.remaining_quantity <= maker_quantity {
                    order.remaining_quantity
                } else {
                    maker_quantity
                };

                // Create trade (execution price is maker's price)
                let trade = executor.execute_trade(
                    book.symbol.clone(),
                    maker_order_id,
                    order.order_id,
                    maker_account_id,  // Use maker's account_id
                    order.account_id,  // Use taker's account_id
                    Side::BUY,
                    ask_price, // Maker's price
                    match_qty,
                    timestamp,
                ).map_err(EngineError::MatchError)?;

                trades.push(trade);

                // Update order quantities
                order.add_fill(match_qty, timestamp);

                // Update maker order in book
                let new_maker_qty = Quantity::try_new(
                    maker_quantity.as_decimal() - match_qty.as_decimal()
                ).unwrap_or(Quantity::zero());
                ask_level.update_front_quantity(new_maker_qty);

                // If incoming order is filled, we're done
                if order.is_filled() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(trades)
    }

    /// Match incoming sell order against bids (implementation)
    fn match_sell_order_impl(
        book: &mut OrderBook,
        executor: &mut MatchExecutor,
        order: &mut Order,
        timestamp: i64,
    ) -> Result<Vec<Trade>, EngineError> {
        let mut trades = Vec::new();

        // Match against bids (buy orders)
        while let Some((bid_price, bid_level)) = book.bids.best_bid_level_mut() {
            // Check if prices cross
            if !crossing::can_match(bid_price, order.price) {
                break;
            }

            // Get front order from bid level
            if let Some((maker_order_id, maker_account_id, maker_quantity)) = bid_level.peek_front() {
                // Determine match quantity
                let match_qty = if order.remaining_quantity <= maker_quantity {
                    order.remaining_quantity
                } else {
                    maker_quantity
                };

                // Create trade (execution price is maker's price)
                let trade = executor.execute_trade(
                    book.symbol.clone(),
                    maker_order_id,
                    order.order_id,
                    maker_account_id,  // Use maker's account_id
                    order.account_id,  // Use taker's account_id
                    Side::SELL,
                    bid_price, // Maker's price
                    match_qty,
                    timestamp,
                ).map_err(EngineError::MatchError)?;

                trades.push(trade);

                // Update order quantities
                order.add_fill(match_qty, timestamp);

                // Update maker order in book
                let new_maker_qty = Quantity::try_new(
                    maker_quantity.as_decimal() - match_qty.as_decimal()
                ).unwrap_or(Quantity::zero());
                bid_level.update_front_quantity(new_maker_qty);

                // If incoming order is filled, we're done
                if order.is_filled() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(trades)
    }

    /// Cancel an order
    pub fn cancel_order(&mut self, symbol: &str, order_id: &OrderId, price: Price, side: Side) -> bool {
        if let Some(book) = self.books.get_mut(symbol) {
            match side {
                Side::BUY => book.bids.remove(order_id, price),
                Side::SELL => book.asks.remove(order_id, price),
            }
        } else {
            false
        }
    }

    /// Get order book snapshot
    pub fn get_order_book(&self, symbol: &str, depth: usize) -> Option<OrderBookSnapshot> {
        self.books.get(symbol).map(|book| OrderBookSnapshot {
            symbol: symbol.to_string(),
            bids: book.bids.depth_snapshot(depth),
            asks: book.asks.depth_snapshot(depth),
        })
    }
}

/// Order book snapshot for market data
#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub symbol: String,
    pub bids: Vec<(Price, Quantity)>,
    pub asks: Vec<(Price, Quantity)>,
}

/// Engine errors
#[derive(Debug)]
pub enum EngineError {
    MatchError(MatchError),
    InvalidOrder(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::AccountId;
    use types::order::TimeInForce;

    fn create_order_with_account(account_id: AccountId, side: Side, price: u64, qty: &str) -> Order {
        Order::new(
            account_id,
            MarketId::new("BTC/USDT"),
            side,
            Price::from_u64(price),
            Quantity::from_str(qty).unwrap(),
            TimeInForce::GTC,
            1708123456789000000,
        )
    }

    #[test]
    fn test_engine_resting_order() {
        let mut engine = MatchingEngine::new(1000);
        let order = create_order_with_account(AccountId::new(), Side::BUY, 50000, "1.0");

        let result = engine.submit_order(order, 1708123456789000000).unwrap();
        
        assert!(matches!(result, SubmitResult::Resting));
    }

    #[test]
    fn test_engine_full_match() {
        let mut engine = MatchingEngine::new(1000);
        let account1 = AccountId::new();
        let account2 = AccountId::new();
        
        // Submit resting sell order from account1
        let sell_order = create_order_with_account(account1, Side::SELL, 50000, "1.0");
        engine.submit_order(sell_order, 1708123456789000000).unwrap();

        // Submit matching buy order from account2 (different account)
        let buy_order = create_order_with_account(account2, Side::BUY, 50000, "1.0");
        let result = engine.submit_order(buy_order, 1708123456790000000).unwrap();

        match result {
            SubmitResult::Filled { trades } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].quantity, Quantity::from_str("1.0").unwrap());
            }
            _ => panic!("Expected Filled result"),
        }
    }

    #[test]
    fn test_engine_partial_match() {
        let mut engine = MatchingEngine::new(1000);
        let account1 = AccountId::new();
        let account2 = AccountId::new();
        
        // Submit resting sell order (smaller) from account1
        let sell_order = create_order_with_account(account1, Side::SELL, 50000, "0.5");
        engine.submit_order(sell_order, 1708123456789000000).unwrap();

        // Submit larger buy order from account2
        let buy_order = create_order_with_account(account2, Side::BUY, 50000, "1.0");
        let result = engine.submit_order(buy_order, 1708123456790000000).unwrap();

        match result {
            SubmitResult::PartiallyFilled { trades, remaining } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].quantity, Quantity::from_str("0.5").unwrap());
                assert_eq!(remaining.remaining_quantity, Quantity::from_str("0.5").unwrap());
            }
            _ => panic!("Expected PartiallyFilled result"),
        }
    }

    #[test]
    fn test_engine_no_cross() {
        let mut engine = MatchingEngine::new(1000);
        
        // Submit sell order at 51000
        let sell_order = create_order_with_account(AccountId::new(), Side::SELL, 51000, "1.0");
        engine.submit_order(sell_order, 1708123456789000000).unwrap();

        // Submit buy order at 50000 (doesn't cross)
        let buy_order = create_order_with_account(AccountId::new(), Side::BUY, 50000, "1.0");
        let result = engine.submit_order(buy_order, 1708123456790000000).unwrap();

        assert!(matches!(result, SubmitResult::Resting));
    }
}
