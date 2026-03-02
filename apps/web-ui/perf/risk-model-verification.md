# Risk Model Verification Report

**Generated**: 2026-03-02T13:34:07.783Z
**Total Snapshots**: 8
**Passed**: 8
**Failed**: 0
**Confidence**: HIGH

## Results

### GS-001: Single long BTC position, 10x leverage, at entry
**Status**: ✅ PASS

### GS-002: Single long BTC, mark dropped 5% (underwater)
**Status**: ✅ PASS

### GS-003: Short ETH position, mark rose (losing)
**Status**: ✅ PASS

### GS-004: Multiple positions, near danger zone
**Status**: ✅ PASS

### GS-005: Zero-size position (edge case)
**Status**: ✅ PASS

### GS-006: Very large position, 125x leverage
**Status**: ✅ PASS

### GS-007: Liquidation zone — margin ratio < 1.1
**Status**: ✅ PASS

### GS-008: Warning zone — margin ratio between 1.5 and 2.0
**Status**: ✅ PASS

## Methodology

- Margin calculations use decimal.js with ROUND_UP (favor safety).
- Liquidation prices solved algebraically from margin_ratio = 1.1.
- Tolerance for numeric comparison: ±0.01.
- Golden snapshots sourced from spec §05/§06 examples and edge cases.

## Known Limitations

- Cross-margin multi-position liquidation cascade is approximate.
- Portfolio-margin (VaR) mode is not yet implemented.
- Concentration risk add-on is not included in these calculations.
