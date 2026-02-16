//! Crossing detection logic
//!
//! Determines when a bid and ask can match based on price compatibility

use types::numeric::Price;
use types::order::Side;

/// Check if a bid and ask can match at given prices
///
/// For a buy order to match with a sell order:
/// - Buy price must be >= sell price
///
/// This implements spec §3.3.1 (Matching Conditions)
pub fn can_match(bid_price: Price, ask_price: Price) -> bool {
    bid_price >= ask_price
}

/// Check if an incoming order can match against resting order
///
/// Returns true if the incoming order price crosses the resting order price
pub fn incoming_can_match(incoming_side: Side, incoming_price: Price, resting_price: Price) -> bool {
    match incoming_side {
        Side::BUY => incoming_price >= resting_price,   // Buy crosses sell if bid >= ask
        Side::SELL => incoming_price <= resting_price,  // Sell crosses buy if ask <= bid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_match_crossing() {
        let bid = Price::from_u64(50000);
        let ask = Price::from_u64(49000);
        assert!(can_match(bid, ask), "Bid >= ask should match");
    }

    #[test]
    fn test_can_match_exact() {
        let price = Price::from_u64(50000);
        assert!(can_match(price, price), "Equal prices should match");
    }

    #[test]
    fn test_can_match_no_cross() {
        let bid = Price::from_u64(49000);
        let ask = Price::from_u64(50000);
        assert!(!can_match(bid, ask), "Bid < ask should not match");
    }

    #[test]
    fn test_incoming_buy_can_match() {
        let buy_price = Price::from_u64(50000);
        let sell_price = Price::from_u64(49000);
        assert!(incoming_can_match(Side::BUY, buy_price, sell_price));
    }

    #[test]
    fn test_incoming_sell_can_match() {
        let sell_price = Price::from_u64(49000);
        let buy_price = Price::from_u64(50000);
        assert!(incoming_can_match(Side::SELL, sell_price, buy_price));
    }
}
