//! Trade execution logic
//!
//! Handles full and partial matches, generates trades, calculates fees

use rust_decimal::Decimal;
use types::ids::{AccountId, OrderId};
use types::numeric::{Price, Quantity};
use types::order::Side;
use types::trade::Trade;

/// Match executor for handling trade generation
pub struct MatchExecutor {
    sequence_counter: u64,
}

impl MatchExecutor {
    /// Create a new match executor with starting sequence number
    pub fn new(starting_sequence: u64) -> Self {
        Self {
            sequence_counter: starting_sequence,
        }
    }

    /// Get next sequence number (monotonically increasing)
    fn next_sequence(&mut self) -> u64 {
        let seq = self.sequence_counter;
        self.sequence_counter += 1;
        seq
    }

    /// Execute a trade between maker and taker orders
    ///
    /// Returns a Trade struct with all details including fees
    #[allow(clippy::too_many_arguments)]
    pub fn execute_trade(
        &mut self,
        symbol: types::ids::MarketId,
        maker_order_id: OrderId,
        taker_order_id: OrderId,
        maker_account_id: AccountId,
        taker_account_id: AccountId,
        side: Side,  // From taker perspective
        price: Price,  // Execution price (maker's price per price-time priority)
        quantity: Quantity,
        timestamp: i64,
    ) -> Result<Trade, MatchError> {
        // Self-trade prevention per spec §12 (Determinism Rules)
        if maker_account_id == taker_account_id {
            return Err(MatchError::SelfTrade);
        }

        // Calculate fees per spec §7 (Fee System)
        let (maker_fee, taker_fee) = self.calculate_fees(price, quantity);

        let sequence = self.next_sequence();

        Ok(Trade::new(
            sequence,
            symbol,
            maker_order_id,
            taker_order_id,
            maker_account_id,
            taker_account_id,
            side,
            price,
            quantity,
            maker_fee,
            taker_fee,
            timestamp,
        ))
    }

    /// Calculate maker and taker fees per spec §7
    ///
    /// Fee formula: fee = quantity × price × fee_rate
    /// - Maker: -0.01% (rebate) to 0.10%
    /// - Taker: 0.02% to 0.30%
    ///
    /// Using default rates for now: maker = 0.00%, taker = 0.05%
    fn calculate_fees(&self, price: Price, quantity: Quantity) -> (Decimal, Decimal) {
        let trade_value = quantity.as_decimal() * price.as_decimal();
        
        // Default fee rates (can be made configurable later)
        let maker_rate = Decimal::ZERO; // 0% (or could be negative for rebate)
        let taker_rate = Decimal::new(5, 4); // 0.05% = 0.0005
        
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        
        (maker_fee, taker_fee)
    }
}

/// Match execution errors
#[derive(Debug, Clone, PartialEq)]
pub enum MatchError {
    /// Self-trade prevention triggered
    SelfTrade,
    /// Invalid quantity (zero or negative)
    InvalidQuantity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::ids::MarketId;

    #[test]
    fn test_execute_trade() {
        let mut executor = MatchExecutor::new(1000);
        
        let trade = executor.execute_trade(
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            1708123456789000000,
        ).unwrap();

        assert_eq!(trade.sequence, 1000);
        assert_eq!(trade.price, Price::from_u64(50000));
        assert_eq!(trade.quantity, Quantity::from_str("0.5").unwrap());
    }

    #[test]
    fn test_self_trade_prevention() {
        let mut executor = MatchExecutor::new(1000);
        let account_id = AccountId::new();
        
        let result = executor.execute_trade(
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            account_id,  // Same account for both maker and taker
            account_id,
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            1708123456789000000,
        );

        assert_eq!(result, Err(MatchError::SelfTrade));
    }

    #[test]
    fn test_sequence_monotonic() {
        let mut executor = MatchExecutor::new(1000);
        
        let trade1 = executor.execute_trade(
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.5").unwrap(),
            1708123456789000000,
        ).unwrap();

        let trade2 = executor.execute_trade(
            MarketId::new("BTC/USDT"),
            OrderId::new(),
            OrderId::new(),
            AccountId::new(),
            AccountId::new(),
            Side::BUY,
            Price::from_u64(50000),
            Quantity::from_str("0.3").unwrap(),
            1708123456790000000,
        ).unwrap();

        assert_eq!(trade1.sequence, 1000);
        assert_eq!(trade2.sequence, 1001);
    }

    #[test]
    fn test_fee_calculation() {
        let executor = MatchExecutor::new(0);
        
        let price = Price::from_u64(50000);
        let qty = Quantity::from_str("1.0").unwrap();
        
        let (maker_fee, taker_fee) = executor.calculate_fees(price, qty);
        
        // With 0% maker and 0.05% taker:
        // Trade value = 50000
        // Maker fee = 0
        // Taker fee = 50000 * 0.0005 = 25
        assert_eq!(maker_fee, Decimal::ZERO);
        assert_eq!(taker_fee, Decimal::new(25, 0));
    }
}
