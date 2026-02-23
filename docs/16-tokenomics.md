# Tokenomics Outline

**Phase**: Launch & Operational Docs  
**Component**: Business

## 1. Overview

The exchange generates revenue exclusively through its transparent Fee System (Spec 07). This document outlines the economic flow of the DEX ecosystem.

## 2. Value Capture

Value is captured through two primary mechanisms:
1. **Trading Fees**: Deducted on each trade settlement.
2. **Liquidation Fees**: Deducted during forced position closures.

(Note: All deposits are 100% free of charge. Fiat withdrawals are flat-fee).

## 3. Trading Fee Structure
The exchange uses a Maker-Taker model designed to incentivize liquidity provision:
- **Base Maker Fee**: -0.01% to 0.02% (providing a rebate for top-tier liquidity providers).
- **Base Taker Fee**: 0.04% to 0.10%.

## 4. Fee Distribution 

Collected fees are distributed algorithmically on a daily basis:

**Trading Fees Allocation**:
- **70%**: Exchange Revenue (Operations, team, validators).
- **20%**: Insurance Fund (Backstops the liquidation engine).
- **10%**: Referral Program (Growth).

**Liquidation Fees Allocation**:
- **50%**: Insurance Fund.
- **30%**: Operations (Covers liquidation engine compute costs).
- **20%**: Exchange Revenue.

## 5. Ecosystem Token Integration (Future Phase)

While the v1.0.0 exchange spec does not mandate a native platform token, the architecture supports incorporating a `DEX Token`.
**Token Utilities**:
1. **Fee Discounts**: Holding or burning the token provides a 25% discount on maker/taker fees.
2. **Governance Decentralization**: Transitioning the current Centralized Multi-Sig governance to a DAO structure where token holders vote on Fee Tier adjustments and Asset Listings.
