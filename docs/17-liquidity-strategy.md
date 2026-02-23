# Liquidity Strategy

**Phase**: Launch & Operational Docs  
**Component**: Business & Operations

## 1. Overview

A high-performance Matching Engine is only as valuable as the liquidity it facilitates. This document outlines the strategies employed to bootstrap and maintain deep liquidity on the exchange from Day 1.

## 2. Maker Incentives

The core mechanism for attracting passive liquidity is the **Maker Rebate Model** (defined in Spec 07).

- **Base Rebate**: Market Makers receive a -0.01% to -0.05% rebate on all resting limit orders that are filled.
- **Volume Tiers**: Sub-accounts hitting >$100M 30-day volume automatically qualify for the highest rebate tier to encourage continued capital deployment.

## 3. Designated Market Maker (DMM) Program

The exchange will engage with 3-5 institutional Designated Market Makers for the launch. 

### 3.1 DMM Obligations
- **Uptime**: Must maintain quotes on both sides of the CLOB (Central Limit Order Book) for 99% of trading hours.
- **Max Spread**: Must maintain a bid-ask spread of < 0.1% for BTC/USDT and < 0.2% for ETH/USDT.
- **Minimum Size**: Quotes must meet minimum notional sizes defined per tier.

### 3.2 DMM Benefits
- Negative fees (up to -0.05% maker).
- Preferential API rate limits (bypassing standard 100 req/sec caps).
- Direct cross-connects to the Matching Engine VPC (if applicable).

## 4. Bootstrapping New Pairs

When deciding to list a new asset (governed by Spec 17):
1. Allocate targeted capital to DMMs to seed the initial book.
2. Launch trading in "Post-Only" mode for the first 15 minutes to allow the book to build depth without abrupt slippage.
3. Transition to standard matching.

## 5. Monitoring Liquidity Health

Liquidity is actively monitored on Grafana dashboards:
1. **Spread Depth**: Total notional value available within 1%, 2%, and 5% of the Mark Price.
2. **Slippage Analytics**: Average slippage calculated for standard order sizes ($10k, $100k, $1M).
3. **Liquidation Absorption**: Ensuring the book is deep enough to gracefully handle the liquidation of the open interest's 95th percentile position without triggering ADL (Auto-Deleveraging).
