# Fee System Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the complete fee structure for the distributed exchange, including trading fees, withdrawal fees, liquidation fees, and fee distribution.

## 2. Trading Fees

### 2.1 Maker-Taker Model

| Role | Definition | Fee Rate (Default) |
|------|------------|-------------------|
| Maker | Adds liquidity (limit order in book) | -0.01% to 0.02% |
| Taker | Removes liquidity (market order) | 0.04% to 0.10% |

**Negative Fee** (Maker Rebate): Makers receive payment for providing liquidity

### 2.2 Fee Calculation

```
trade_fee = trade_value × fee_rate

trade_value = quantity × price
```

**Rounding**: Round UP to 8 decimal places (never undercharge)

### 2.3 Volume-Based Tiers

| 30-Day Volume (USDT) | Maker Fee | Taker Fee |
|---------------------|-----------|-----------|
| 0 - 100,000 | 0.02% | 0.10% |
| 100,001 - 1,000,000 | 0.01% | 0.08% |
| 1,000,001 - 10,000,000 | 0.00% | 0.06% |
| 10,000,001 - 100,000,000 | -0.01% | 0.04% |
| 100,000,001+ | -0.02% | 0.02% |

**Update Frequency**: Daily at 00:00 UTC

### 2.4 VIP Tiers

Custom rates negotiated for:
- Market makers (volume > $100M/month)
- Institutional clients
- Strategic partners

**Min Rates**: Maker: -0.05%, Taker: 0.01%

## 3. Liquidation Fees

Defined in liquidation-process.md:
- 0.50% to 2.00% based on margin ratio
- Cap: 5% of position value
- Allocation: 50% insurance fund, 30% operations, 20% revenue

## 4. Withdrawal Fees

### 4.1 Blockchain Fees

| Asset | Withdrawal Fee | Min Withdrawal |
|-------|---------------|----------------|
| BTC | 0.0005 BTC | 0.001 BTC |
| ETH | 0.005 ETH | 0.01 ETH |
| USDT (TRC20) | 1 USDT | 10 USDT |
| USDT (ERC20) | 5 USDT | 20 USDT |
| Others | Dynamic | Varies |

**Dynamic Fees**: Adjusted based on network congestion

### 4.2 Fiat Withdrawals

| Method | Fee | Processing Time |
|--------|-----|----------------|
| Bank Wire | $25 flat | 1-3 business days |
| ACH | $5 flat | 3-5 business days |
| SEPA | €3 flat | 1-2 business days |

## 5. Deposit Fees

**Policy**: FREE for all deposits (crypto and fiat)

## 6. Fee Collection

### 6.1 Timing

**Trading Fees**: Deducted immediately during settlement  
**Withdrawal Fees**: Deducted from withdrawal amount  
**Liquidation Fees**: Deducted during liquidation settlement

### 6.2 Currency

**Trading**: Fees paid in quote currency (e.g., USDT for BTC/USDT)  
**Withdrawal**: Fees paid in withdrawal currency  
**Conversion**: If fee token balance insufficient, auto-convert from available balance

## 7. Fee Distribution

### 7.1 Trading Fee Allocation

- 70% Exchange Revenue
- 20% Insurance Fund
- 10% Referral Program / Marketing

### 7.2 Liquidation Fee Allocation

- 50% Insurance Fund
- 30% Operations (liquidation engine costs)
- 20% Exchange Revenue

## 8. Fee Token Program (Optional)

**Platform Token**: DEX Token (hypothetical)  
**Benefit**: 25% fee discount when paying fees with DEX tokens  
**Example**: 0.10% taker fee → 0.075% when using DEX token

## 9. Deterministic Calculation

```
// CORRECT
fee = Decimal::from(value) * Decimal::from(rate)
fee = fee.round_up(8)  // Always round up

// WRONG  
fee = value * rate  // Floating point error
```

**Guarantee**: Same inputs → same fee amount

## 10. Fee Caps

- Max trading fee: 2% of trade value
- Max withdrawal fee: 1% of withdrawal amount (crypto)
- Fiat withdrawal: $100 max

## 11. Fee Waivers

**Conditions**:
- VIP tier 5+
- Market maker agreement
- Promotional campaigns
- Bug bounty rewards

**Implementation**: Fee rate = 0 for specific accounts

## 12. Refunds

**When**:
- Exchange error caused incorrect fee
- System malfunction
- Agreed compensation

**Process**: Manual review + approval + credit to account

## 13. Transparency

**Public Display**:
- Current fee rates by tier
- User's current tier
- 30-day volume tracker
- Next tier requirements

**Fee History**: Downloadable CSV of all fees paid

## 14. Invariants

1. `collected_fees = Σ all_trades(fee_amount)`
2. `fee_amount >= 0` (no negative fees except maker rebates)
3. `maker_rebate <= trade_value × max_rebate_rate`

## 15. Versioning

**Current Version**: v1.0.0  
**Rate Changes**: 30-day notice for increases, immediate for decreases  
**Tier Structure Changes**: 60-day notice
