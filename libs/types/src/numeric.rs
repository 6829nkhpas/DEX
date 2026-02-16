//! Fixed-point decimal types for prices and quantities
//!
//! Uses rust_decimal for deterministic arithmetic (no floating-point errors).
//! All calculations use HALF_UP rounding per spec §3 (Trade Lifecycle).

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// Price type with fixed-point decimal representation
///
/// Ensures deterministic pricing across all nodes. Must always be positive.
/// Serialized as string to prevent JSON number precision loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Price(Decimal);

impl Price {
    /// Create a new Price from a Decimal
    ///
    /// # Panics
    /// Panics if the price is negative or zero
    pub fn new(value: Decimal) -> Self {
        assert!(value > Decimal::ZERO, "Price must be positive");
        Self(value)
    }

    /// Try to create a Price, returning None if invalid
    pub fn try_new(value: Decimal) -> Option<Self> {
        if value > Decimal::ZERO {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Create from integer (for testing/convenience)
    pub fn from_u64(value: u64) -> Self {
        Self::new(Decimal::from(value))
    }

    /// Create from string
    pub fn from_str(s: &str) -> Result<Self, rust_decimal::Error> {
        let decimal = Decimal::from_str(s)?;
        Ok(Self::new(decimal))
    }

    /// Get the inner decimal value
    pub fn as_decimal(&self) -> Decimal {
        self.0
    }

    /// Round to specified decimal places using HALF_UP strategy
    pub fn round_dp(&self, dp: u32) -> Self {
        Self(self.0.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero))
    }
}

// Arithmetic operations
impl Add for Price {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Price {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        assert!(self.0 >= rhs.0, "Price subtraction would result in negative");
        Self(self.0 - rhs.0)
    }
}

impl Mul<Decimal> for Price {
    type Output = Decimal;

    fn mul(self, rhs: Decimal) -> Self::Output {
        self.0 * rhs
    }
}

impl Div<Decimal> for Price {
    type Output = Price;

    fn div(self, rhs: Decimal) -> Self::Output {
        assert!(rhs != Decimal::ZERO, "Division by zero");
        Price(self.0 / rhs)
    }
}

// Custom serialization to preserve precision
impl Serialize for Price {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Price {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decimal = Decimal::from_str(&s).map_err(serde::de::Error::custom)?;
        Self::try_new(decimal).ok_or_else(|| serde::de::Error::custom("Price must be positive"))
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Quantity type with fixed-point decimal representation
///
/// Ensures deterministic quantity calculations. Must always be positive.
/// Serialized as string to prevent JSON number precision loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quantity(Decimal);

impl Quantity {
    /// Create a new Quantity from a Decimal
    ///
    /// # Panics
    /// Panics if the quantity is negative or zero
    pub fn new(value: Decimal) -> Self {
        assert!(value > Decimal::ZERO, "Quantity must be positive");
        Self(value)
    }

    /// Try to create a Quantity, returning None if invalid
    pub fn try_new(value: Decimal) -> Option<Self> {
        if value > Decimal::ZERO {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Create zero quantity (for filled quantities in rejected orders)
    pub fn zero() -> Self {
        Self(Decimal::ZERO)
    }

    /// Create from integer (for testing/convenience)
    pub fn from_u64(value: u64) -> Self {
        Self::new(Decimal::from(value))
    }

    /// Create from string
    pub fn from_str(s: &str) -> Result<Self, rust_decimal::Error> {
        let decimal = Decimal::from_str(s)?;
        Ok(Self::new(decimal))
    }

    /// Get the inner decimal value
    pub fn as_decimal(&self) -> Decimal {
        self.0
    }

    /// Round to specified decimal places using HALF_UP strategy
    pub fn round_dp(&self, dp: u32) -> Self {
        Self(self.0.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero))
    }

    /// Check if quantity is zero
    pub fn is_zero(&self) -> bool {
        self.0 == Decimal::ZERO
    }
}

// Arithmetic operations
impl Add for Quantity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Quantity {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        assert!(self.0 >= rhs.0, "Quantity subtraction would result in negative");
        Self(self.0 - rhs.0)
    }
}

impl Mul<Decimal> for Quantity {
    type Output = Decimal;

    fn mul(self, rhs: Decimal) -> Self::Output {
        self.0 * rhs
    }
}

impl Mul<Price> for Quantity {
    type Output = Decimal;

    fn mul(self, rhs: Price) -> Self::Output {
        self.0 * rhs.as_decimal()
    }
}

// Custom serialization to preserve precision
impl Serialize for Quantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let decimal = Decimal::from_str(&s).map_err(serde::de::Error::custom)?;
        // Allow zero or positive (zero is used for filled_quantity in pending orders)
        if decimal >= Decimal::ZERO {
            Ok(Self(decimal))
        } else {
            Err(serde::de::Error::custom("Quantity cannot be negative"))
        }
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_creation() {
        let price = Price::from_u64(50000);
        assert_eq!(price.as_decimal(), Decimal::from(50000));
    }

    #[test]
    #[should_panic(expected = "Price must be positive")]
    fn test_price_negative_panics() {
        Price::new(Decimal::from(-100));
    }

    #[test]
    fn test_price_arithmetic() {
        let p1 = Price::from_u64(100);
        let p2 = Price::from_u64(50);
        
        let sum = p1 + p2;
        assert_eq!(sum, Price::from_u64(150));
        
        let diff = p1 - p2;
        assert_eq!(diff, Price::from_u64(50));
    }

    #[test]
    fn test_price_serialization() {
        let price = Price::from_str("50000.25").unwrap();
        let json = serde_json::to_string(&price).unwrap();
        assert_eq!(json, "\"50000.25\"");
        
        let deserialized: Price = serde_json::from_str(&json).unwrap();
        assert_eq!(price, deserialized);
    }

    #[test]
    fn test_quantity_creation() {
        let qty = Quantity::from_str("1.5").unwrap();
        assert_eq!(qty.as_decimal(), Decimal::from_str("1.5").unwrap());
    }

    #[test]
    fn test_quantity_zero() {
        let qty = Quantity::zero();
        assert!(qty.is_zero());
    }

    #[test]
    fn test_quantity_arithmetic() {
        let q1 = Quantity::from_str("2.5").unwrap();
        let q2 = Quantity::from_str("1.5").unwrap();
        
        let sum = q1 + q2;
        assert_eq!(sum.as_decimal(), Decimal::from_str("4.0").unwrap());
        
        let diff = q1 - q2;
        assert_eq!(diff.as_decimal(), Decimal::from_str("1.0").unwrap());
    }

    #[test]
    fn test_quantity_price_multiplication() {
        let qty = Quantity::from_str("1.5").unwrap();
        let price = Price::from_u64(100);
        
        let value = qty * price;
        assert_eq!(value, Decimal::from(150));
    }

    #[test]
    fn test_price_rounding() {
        let price = Price::from_str("50000.123456789").unwrap();
        let rounded = price.round_dp(8);
        assert_eq!(rounded.to_string(), "50000.12345679");
    }

    #[test]
    fn test_deterministic_calculation() {
        // Ensure same inputs always produce same output
        let qty1 = Quantity::from_str("0.123456789").unwrap();
        let price1 = Price::from_str("50000.987654321").unwrap();
        
        let result1 = qty1 * price1;
        
        let qty2 = Quantity::from_str("0.123456789").unwrap();
        let price2 = Price::from_str("50000.987654321").unwrap();
        
        let result2 = qty2 * price2;
        
        assert_eq!(result1, result2, "Deterministic calculation failed");
    }
}

