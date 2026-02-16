# Margin Methodology Specification

**Version**: 1.0.0  
**Status**: FROZEN  
**Authority**: Global Spec

## 1. Overview

This document defines the margin calculation methodology for the distributed exchange, covering initial margin, maintenance margin, leverage, and risk-based margining.

## 2. Margin Types

### 2.1 Initial Margin (IM)

**Definition**: Minimum collateral required to open a position  
**Purpose**: Cover potential losses during normal market conditions  
**Formula**:
```
initial_margin = (position_size × entry_price) / leverage
```

**Example**:
- Position: 1 BTC @ $50,000
- Leverage: 10x
- Initial margin: $50,000 / 10 = $5,000

### 2.2 Maintenance Margin (MM)

**Definition**: Minimum collateral to keep position open  
**Purpose**: Trigger liquidation before account goes negative  
**Formula**:
```
maintenance_margin = position_value × maintenance_margin_rate
```

**Typical Rates**:
- 1x-10x leverage: 0.5% (0.005)
- 11x-20x leverage: 1.0% (0.01)
- 21x-50x leverage: 2.0% (0.02)
- 51x-100x leverage: 5.0% (0.05)
- 101x-125x leverage: 10.0% (0.10)

**Invariant**: `maintenance_margin < initial_margin` always

### 2.3 Cross Margin vs Isolated Margin

| Aspect | Cross Margin | Isolated Margin |
|--------|--------------|-----------------|
| Collateral Pool | Entire account balance | Per-position allocation |
| Risk | All positions share risk | Risk limited to position margin |
| Liquidation | All positions liquidated | Only specific position |
| Margin Efficiency | Higher (shared collateral) | Lower (isolated) |
| Complexity | Lower | Higher |

**Default Mode**: Cross Margin (user can opt-in to isolated)

## 3. Margin Calculations

### 3.1 Required Margin for New Order

**Formula**:
```
order_margin = (order_quantity × order_price) / leverage
```

**Validation**:
```
available_margin = equity - margin_used - locked_margin
IF order_margin > available_margin THEN reject
```

### 3.2 Position Margin

**After Trade Execution**:
```
position_margin = (average_entry_price × position_size) / leverage
```

**With Multiple Fills**:
```
new_avg_price = (old_size × old_avg + fill_size × fill_price) / (old_size + fill_size)
new_margin = (new_avg_price × new_size) / leverage
```

### 3.3 Unrealized PnL Impact

**Equity Calculation**:
```
equity = total_balance + unrealized_pnl

unrealized_pnl = Σ positions [(mark_price - entry_price) × size × direction]
  where direction = +1 for LONG, -1 for SHORT
```

**Margin Ratio**:
```
margin_ratio = equity / maintenance_margin
```

**Health Check**:
- `margin_ratio >= 2.0`: Healthy (can open more positions)
- `1.5 <= margin_ratio < 2.0`: Warning
- `1.1 <= margin_ratio < 1.5`: Danger (reduce positions recommended)
- `margin_ratio < 1.1`: Liquidation triggered

## 4. Leverage Tiers

### 4.1 Leverage Limits by Position Size

| Position Value (USDT) | Max Leverage | Initial Margin Rate | Maintenance Margin Rate |
|-----------------------|--------------|---------------------|-------------------------|
| 0 - 50,000 | 125x | 0.80% | 0.40% |
| 50,001 - 250,000 | 100x | 1.00% | 0.50% |
| 250,001 - 1,000,000 | 50x | 2.00% | 1.00% |
| 1,000,001 - 5,000,000 | 20x | 5.00% | 2.50% |
| 5,000,001 - 20,000,000 | 10x | 10.00% | 5.00% |
| 20,000,001+ | 5x | 20.00% | 10.00% |

**Auto-Deleveraging**: As position grows, leverage automatically reduces

### 4.2 Symbol-Specific Leverage

**Volatility-Based**:
```
High Volatility (e.g., altcoins): Max 20x
Medium Volatility (e.g., ETH): Max 50x
Low Volatility (e.g., BTC, stablecoins): Max 125x
```

**Override**: Risk team can adjust per symbol

## 5. Risk-Based Margining

### 5.1 Portfolio Margin

**For Advanced Accounts** (opt-in):

**Calculation**:
```
margin_requirement = VaR(portfolio, confidence=99%, horizon=1h)
```

**Value at Risk (VaR)**:
- Historical simulation (past 30 days)
- Monte Carlo simulation (10,000 scenarios)
- Take max of both methods

**Benefit**: Lower margin for hedged portfolios

**Example**:
- Long 1 BTC futures + Short 1 BTC spot
- Standard margin: 2x initial margin
- Portfolio margin: ~0.1x (highly hedged)

### 5.2 Concentration Risk

**Additional Margin** for large single-symbol positions:
```
concentration_add_on = base_margin × concentration_factor

concentration_factor = max(0, (position_size / daily_volume) - 5%)
```

**Example**:
- Position: 1000 BTC
- Daily Volume: 10,000 BTC
- Concentration: 10%
- Add-on: base_margin × 5% = extra margin required

### 5.3 Volatility Adjustment

**Dynamic Margin** based on recent volatility:
```
volatility_multiplier = current_volatility / baseline_volatility

adjusted_margin = base_margin × max(1.0, volatility_multiplier)
```

**Baseline**: 30-day historical volatility  
**Current**: 24-hour realized volatility  
**Update Frequency**: Every hour

## 6. Margin Calls

### 6.1 Warning Levels

| Margin Ratio | Action | Notification |
|--------------|--------|--------------|
| < 2.0 | Warning email | Low priority |
| < 1.5 | Warning + SMS | Medium priority |
| < 1.2 | Margin call + block new orders | High priority |
| < 1.1 | Liquidation initiated | Critical |

### 6.2 Margin Call Process

1. **Detect**: Margin ratio falls below 1.2
2. **Notify**: Send email + SMS + in-app notification
3. **Action**: Block new position-increasing orders
4. **Grace Period**: 15 minutes to add collateral or reduce position
5. **Auto-Close**: If not resolved, close positions (smallest first)

**Note**: No grace period if margin ratio < 1.1 (immediate liquidation)

## 7. Margin Addition and Withdrawal

### 7.1 Adding Margin

**Effect**:
```
available_margin += deposit_amount
equity += deposit_amount
margin_ratio = equity / maintenance_margin  // Improves
```

**Instant**: Takes effect immediately

### 7.2 Withdrawing Margin

**Validation**:
```
withdrawal_amount <= available_margin - buffer

buffer = maintenance_margin × 0.2  // 20% safety buffer
```

**Rejection**: If withdrawal would cause `margin_ratio < 1.5`

### 7.3 Automatic Margin Transfer

**Cross-Margin Mode**:
- Profits from one position automatically support other positions
- Losses reduce overall available margin

**Isolated Mode**:
- Each position has dedicated margin pool
- No automatic transfer between positions
- Must manually transfer between positions

## 8. Leverage Adjustment

### 8.1 Increasing Leverage

**Requirement**: 
```
new_margin = position_value / new_leverage
available_margin >= (old_margin - new_margin)
```

**Effect**: Releases margin (increases risk)

**Restriction**: Cannot increase leverage if `margin_ratio < 2.0`

### 8.2 Decreasing Leverage

**Requirement**:
```
additional_margin = position_value × (1/new_leverage - 1/old_leverage)
available_balance >= additional_margin
```

**Effect**: Requires more margin (reduces risk)

**Allowed**: Always (improves account health)

## 9. Deterministic Calculations

### 9.1 Fixed-Point Arithmetic

**Requirement**: No floating-point math  
**Implementation**: Use fixed-point Decimal with 18 decimal places

**Example**:
```rust
// WRONG (non-deterministic)
let margin = (position_value as f64) / (leverage as f64);

// CORRECT (deterministic)
let margin = Decimal::from(position_value) / Decimal::from(leverage);
```

### 9.2 Rounding Rules

**Margin Calculations**:
- Initial margin: Round UP (favor safety)
- Maintenance margin: Round UP (favor safety)
- Available margin: Round DOWN (conservative)

**Mode**: HALF_UP for all other calculations

### 9.3 Timestamp Consistency

**Mark Price Source**: Use exchange timestamp, not system clock  
**Update Frequency**: Fixed intervals (e.g., every 1 second)  
**Determinism**: Same mark price → same margin calculations

## 10. Margin in Different Account Types

### 10.1 Spot Account

**Margin**: Not applicable (full payment required)  
**Leverage**: 1x only  
**Balance**: `available + locked = total`

### 10.2 Margin Account

**Initial Margin**: `position_value / leverage`  
**Maintenance Margin**: `position_value × mm_rate`  
**Max Leverage**: 10x  
**Borrowing**: Allowed (pay interest)

### 10.3 Futures Account

**Initial Margin**: Tiered by position size  
**Maintenance Margin**: 0.4% to 10% (leverage-dependent)  
**Max Leverage**: 125x  
**Contract Type**: Perpetual or dated futures

## 11. Edge Cases

### 11.1 Negative Equity

**Scenario**: Losses exceed collateral (gap down/up)  
**Insurance Fund**: Covers socialized losses  
**Auto-Deleveraging**: If insurance fund depleted

**Prevention**:
- Aggressive maintenance margin rates
- Real-time risk monitoring
- Circuit breakers on extreme moves

### 11.2 Dust Positions

**Definition**: Position too small to liquidate profitably  
**Threshold**: < $10 position value  
**Handling**: Auto-close at market price (no liquidation fee)

### 11.3 Multiple Positions Same Symbol

**Cross Margin**:
```
total_margin = net_position_value / leverage
```

**Isolated Margin**:
```
total_margin = Σ individual_position_margins
```

**Netting**: Long and short positions offset in cross margin mode

## 12. Performance Requirements

- Margin calculation: < 100μs per position
- Account-wide margin: < 1ms (up to 100 positions)
- Margin ratio update: < 5ms (including mark price fetch)
- Liquidation trigger: < 10ms from margin breach

## 13. Monitoring and Alerts

### 13.1 Real-Time Metrics

- System-wide margin utilization
- Distribution of margin ratios
- Average leverage per symbol
- Number of accounts near liquidation

### 13.2 Circuit Breakers

**Trigger Conditions**:
- > 10% of accounts below margin ratio 1.5
- > 5% of accounts in liquidation
- Mark price moves > 10% in 1 minute

**Action**: Pause new order entry (existing orders remain)

## 14. Compliance and Limits

### 14.1 Regulatory Limits

**Per Jurisdiction**:
- US: Max 2x leverage (retail)
- EU: Max 30x leverage (crypto)
- Offshore: Max 125x leverage

**Enforcement**: Geo-IP + KYC-based restrictions

### 14.2 Account Tier Limits

| Tier | Max Leverage | Max Position Size | Margin Call Ratio |
|------|--------------|-------------------|-------------------|
| Tier 1 (Retail) | 20x | $100,000 | 1.2 |
| Tier 2 (Intermediate) | 50x | $1,000,000 | 1.15 |
| Tier 3 (Professional) | 100x | $10,000,000 | 1.1 |
| Tier 4 (Institutional) | 125x | Unlimited | 1.05 |

## 15. Invariants

1. **Margin Hierarchy**: `maintenance_margin < initial_margin < position_value`
2. **Non-Negative**: All margin values >= 0
3. **Conservation**: `total_margin_reserved = Σ position_margins + Σ order_margins`
4. **Liquidation Trigger**: `margin_ratio < 1.1` ALWAYS triggers liquidation
5. **Leverage Bounds**: `1 <= leverage <= max_leverage_for_tier`

## 16. Versioning

**Current Version**: v1.0.0  
**Rate Changes**: Require 48-hour notice to users  
**Breaking Changes**: New major version with migration period  
**Backward Compatibility**: Old positions grandfathered for 30 days
